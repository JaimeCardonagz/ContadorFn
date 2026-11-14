//! Actualización automática.
//!
//! Medidas de seguridad:
//! - Solo se habla con direcciones fijas de GitHub, siempre por HTTPS (también
//!   en las redirecciones) y con el TLS del sistema. El programa no abre
//!   ningún puerto ni acepta conexiones: solo hace peticiones de salida.
//! - La versión que anuncia GitHub se valida como `N.N.N` antes de usarla para
//!   construir la dirección de descarga; nada más de la respuesta se usa.
//! - El ejecutable descargado debe llevar una firma Ed25519 válida, hecha con la
//!   clave privada del proyecto, sobre un manifiesto que une su versión y su
//!   SHA-256. Así no se acepta un archivo alterado, ni uno firmado para otra
//!   versión, ni una versión antigua (debe ser posterior a la instalada).
//! - Tamaño máximo de descarga y tiempos de espera.
//! - La interfaz no elige qué se instala: solo puede instalar la versión que
//!   esta misma comprobación encontró y verificó.

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Clave pública de firma del proyecto (la privada no está en el código).
const PUBLIC_KEY: [u8; 32] = [
    0xed, 0x11, 0x47, 0xb7, 0x8e, 0xa3, 0xb2, 0x85, 0xbd, 0xbb, 0xaf, 0xae, 0x2b, 0x61, 0x20, 0xd6,
    0xd1, 0xa8, 0x21, 0xfc, 0x6b, 0x4a, 0x85, 0x90, 0x8e, 0x55, 0xb9, 0x65, 0x0e, 0x77, 0x4d, 0x3d,
];
const LATEST_URL: &str = "https://api.github.com/repos/JaimeCardonagz/ContadorFn/releases/latest";
const DOWNLOAD_URL: &str = "https://github.com/JaimeCardonagz/ContadorFn/releases/download";
const ASSET: &str = "FortniteChestCounter.exe";
/// Ningún ejecutable del proyecto se acerca a esto (pesa unos 7 MB).
const MAX_EXE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TEXT_BYTES: u64 = 256 * 1024;

/// Versión `mayor.menor.parche`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u32, u32, u32);

impl Version {
    /// Acepta "1.2.3" o "v1.2.3" y nada más: tres números de hasta cuatro cifras.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let mut parts = text.split('.');
        let mut number = || {
            let part = parts.next()?;
            if part.is_empty() || part.len() > 4 || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()
        };
        let version = Version(number()?, number()?, number()?);
        parts.next().is_none().then_some(version)
    }

    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("versión del paquete válida")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// Lo que se firma: une la versión y el contenido exacto del archivo.
/// Debe coincidir con `manifest` en tools/firmar.py.
fn manifest(version: Version, sha256_hex: &str) -> String {
    format!("FortniteChestCounter update\nversion={version}\nsha256={sha256_hex}\n")
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Comprueba que `signature_b64` es una firma válida de `exe` para `version`.
fn verify_with(
    key: &[u8; 32],
    exe: &[u8],
    signature_b64: &str,
    version: Version,
) -> Result<(), String> {
    use base64::Engine;
    let key = VerifyingKey::from_bytes(key).map_err(|_| "clave pública no válida".to_string())?;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(signature_b64.trim())
        .map_err(|_| "firma mal formada".to_string())?;
    let raw: [u8; 64] = raw
        .try_into()
        .map_err(|_| "firma de tamaño incorrecto".to_string())?;
    let signature = Signature::from_bytes(&raw);
    key.verify_strict(manifest(version, &sha256_hex(exe)).as_bytes(), &signature)
        .map_err(|_| "la firma no coincide".to_string())
}

/// Cliente HTTPS: solo HTTPS (también al redirigir), con tiempos de espera.
fn agent() -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::new().map_err(|e| e.to_string())?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .https_only(true)
        .redirects(5)
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .user_agent(concat!("FortniteChestCounter/", env!("CARGO_PKG_VERSION")))
        .build())
}

/// Descarga `url` entera, con un tamaño máximo.
fn fetch(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let response = agent.get(url).call().map_err(|e| e.to_string())?;
    if !response.get_url().starts_with("https://") {
        return Err("redirección no segura".into());
    }
    let mut body = Vec::new();
    response
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() as u64 > limit {
        return Err("descarga demasiado grande".into());
    }
    Ok(body)
}

/// Versión publicada más reciente, si es posterior a la instalada.
pub fn newer_version() -> Result<Option<Version>, String> {
    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        prerelease: bool,
    }
    let body = fetch(&agent()?, LATEST_URL, MAX_TEXT_BYTES)?;
    let release: Release = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    Ok(Version::parse(&release.tag_name).filter(|v| *v > Version::current()))
}

/// Descarga el ejecutable de `version` y su firma, y lo devuelve solo si la
/// firma es válida y la versión es posterior a la instalada.
pub fn download_verified(version: Version) -> Result<Vec<u8>, String> {
    if version <= Version::current() {
        return Err("la versión no es posterior a la instalada".into());
    }
    let agent = agent()?;
    let base = format!("{DOWNLOAD_URL}/v{version}/{ASSET}");
    let exe = fetch(&agent, &base, MAX_EXE_BYTES)?;
    let signature = fetch(&agent, &format!("{base}.sig"), 1024)?;
    let signature = String::from_utf8(signature).map_err(|_| "firma mal formada".to_string())?;
    verify_with(&PUBLIC_KEY, &exe, &signature, version)?;
    if !exe.starts_with(b"MZ") {
        return Err("el archivo no es un ejecutable de Windows".into());
    }
    Ok(exe)
}

fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    exe.with_file_name(name)
}

/// Sustituye el ejecutable `exe` por `bytes`. Si algo falla, deja el original.
/// En Windows un ejecutable en uso se puede renombrar, pero no sobrescribir:
/// se aparta como `.old` y se borra en el siguiente arranque.
pub fn install_at(exe: &Path, bytes: &[u8]) -> Result<(), String> {
    let new = sibling(exe, ".new");
    let old = sibling(exe, ".old");
    let _ = fs::remove_file(&new);
    let _ = fs::remove_file(&old);
    let write = || -> std::io::Result<()> {
        let mut file = fs::File::create_new(&new)?;
        file.write_all(bytes)?;
        file.sync_all()
    };
    if let Err(error) = write() {
        let _ = fs::remove_file(&new);
        return Err(format!("no se pudo escribir la versión nueva: {error}"));
    }
    if let Err(error) = fs::rename(exe, &old) {
        let _ = fs::remove_file(&new);
        return Err(format!("no se pudo apartar la versión actual: {error}"));
    }
    if let Err(error) = fs::rename(&new, exe) {
        let _ = fs::rename(&old, exe);
        let _ = fs::remove_file(&new);
        return Err(format!("no se pudo colocar la versión nueva: {error}"));
    }
    Ok(())
}

/// Borra los restos de una actualización anterior (`.old` y `.new`).
pub fn cleanup(exe: &Path) {
    let _ = fs::remove_file(sibling(exe, ".old"));
    let _ = fs::remove_file(sibling(exe, ".new"));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Clave y firma de PRUEBA, generadas con tools/firmar.py y una semilla
    /// fija: prueban que lo que firma el script lo acepta la app.
    const TEST_KEY: [u8; 32] = [
        0xea, 0x4a, 0x6c, 0x63, 0xe2, 0x9c, 0x52, 0x0a, 0xbe, 0xf5, 0x50, 0x7b, 0x13, 0x2e, 0xc5,
        0xf9, 0x95, 0x47, 0x76, 0xae, 0xbe, 0xbe, 0x7b, 0x92, 0x42, 0x1e, 0xea, 0x69, 0x14, 0x46,
        0xd2, 0x2c,
    ];
    const TEST_EXE: &[u8] = b"MZ ejecutable de prueba";
    const TEST_SIG: &str =
        "XlT6oUvlohAfYIUI6JYJpJTwFoHkQaMyHvhgJ0/lsbuRNHJRo5y6dbYmATo/Dv/BjWiGeSWbcTO79TkzBzoPAw==";
    const V999: Version = Version(9, 9, 9);

    #[test]
    fn acepta_la_firma_hecha_por_el_script() {
        assert_eq!(verify_with(&TEST_KEY, TEST_EXE, TEST_SIG, V999), Ok(()));
        assert_eq!(
            verify_with(&TEST_KEY, TEST_EXE, &format!("{TEST_SIG}\n"), V999),
            Ok(())
        );
    }

    #[test]
    fn rechaza_un_archivo_alterado() {
        let mut altered = TEST_EXE.to_vec();
        altered[5] ^= 1;
        assert!(verify_with(&TEST_KEY, &altered, TEST_SIG, V999).is_err());
        assert!(verify_with(&TEST_KEY, b"MZ ejecutable de prueba!", TEST_SIG, V999).is_err());
    }

    #[test]
    fn rechaza_la_firma_de_otra_version() {
        assert!(verify_with(&TEST_KEY, TEST_EXE, TEST_SIG, Version(9, 9, 8)).is_err());
        assert!(verify_with(&TEST_KEY, TEST_EXE, TEST_SIG, Version(10, 0, 0)).is_err());
    }

    #[test]
    fn rechaza_otra_clave_y_firmas_mal_formadas() {
        assert!(
            verify_with(&PUBLIC_KEY, TEST_EXE, TEST_SIG, V999).is_err(),
            "clave real"
        );
        for bad in ["", "no es base64", "QUJD", &TEST_SIG[..40]] {
            assert!(
                verify_with(&TEST_KEY, TEST_EXE, bad, V999).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn la_clave_publica_del_proyecto_es_valida() {
        assert!(VerifyingKey::from_bytes(&PUBLIC_KEY).is_ok());
    }

    #[test]
    fn solo_acepta_versiones_bien_formadas() {
        assert_eq!(Version::parse("1.1.2"), Some(Version(1, 1, 2)));
        assert_eq!(Version::parse("v10.0.3"), Some(Version(10, 0, 3)));
        for bad in [
            "",
            "1.2",
            "1.2.3.4",
            "1..2",
            "1.2.x",
            "v",
            "1.2.3-beta",
            " 1.2.3",
            "1.2.3/../x",
            "99999.0.0",
            "+1.2.3",
            "1.2.3\n",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn compara_versiones_por_numero() {
        assert!(Version(1, 1, 10) > Version(1, 1, 9));
        assert!(Version(2, 0, 0) > Version(1, 99, 99));
        assert!(Version::current() >= Version(1, 1, 2));
    }

    #[test]
    fn no_descarga_versiones_que_no_sean_posteriores() {
        assert!(download_verified(Version::current()).is_err());
        assert!(download_verified(Version(1, 0, 0)).is_err());
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fcc-upd-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sustituye_el_ejecutable_y_deja_el_viejo_apartado() {
        let dir = temp("sustituye");
        let exe = dir.join("FortniteChestCounter.exe");
        fs::write(&exe, "viejo").unwrap();
        install_at(&exe, b"nuevo").unwrap();
        assert_eq!(fs::read_to_string(&exe).unwrap(), "nuevo");
        assert_eq!(
            fs::read_to_string(dir.join("FortniteChestCounter.exe.old")).unwrap(),
            "viejo"
        );
        cleanup(&exe);
        assert!(!dir.join("FortniteChestCounter.exe.old").exists());
        assert_eq!(fs::read_to_string(&exe).unwrap(), "nuevo");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn si_no_puede_instalar_deja_el_original() {
        let dir = temp("falla");
        let exe = dir.join("no-existe.exe"); // no hay nada que apartar
        assert!(install_at(&exe, b"nuevo").is_err());
        assert!(!exe.exists());
        assert!(!dir.join("no-existe.exe.new").exists(), "no quedan restos");
        fs::remove_dir_all(dir).unwrap();
    }

    /// Comprobación manual con la clave REAL: verifica un archivo firmado por
    /// tools/firmar.py. Uso: FCC_FIRMADO=<archivo> cargo test firma_real -- --ignored
    #[test]
    #[ignore]
    fn firma_real() {
        let path = std::env::var("FCC_FIRMADO").expect("falta FCC_FIRMADO");
        let exe = fs::read(&path).unwrap();
        let signature = fs::read_to_string(format!("{path}.sig")).unwrap();
        assert_eq!(
            verify_with(&PUBLIC_KEY, &exe, &signature, Version::current()),
            Ok(())
        );
        let mut altered = exe.clone();
        altered.push(0);
        assert!(verify_with(&PUBLIC_KEY, &altered, &signature, Version::current()).is_err());
    }

    /// Comprobación manual contra GitHub (red real).
    /// Uso: cargo test red_real -- --ignored --nocapture
    #[test]
    #[ignore]
    fn red_real() {
        let found = newer_version();
        println!("versión nueva según GitHub: {found:?}");
        assert!(found.is_ok(), "{found:?}");
        // Una versión publicada sin firma no se acepta.
        let unsigned = download_verified(Version(99, 0, 0));
        println!("descarga de una versión inexistente: {unsigned:?}");
        assert!(unsigned.is_err());
    }
}
