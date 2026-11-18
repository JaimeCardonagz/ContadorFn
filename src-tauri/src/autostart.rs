//! Iniciar con Windows: un valor en la clave de inicio del usuario
//! (HKCU\Software\Microsoft\Windows\CurrentVersion\Run). No necesita permisos
//! de administrador y se quita igual de fácil. Al arrancar así, el programa
//! va directo a la bandeja.

/// Argumento con el que Windows abre el programa al iniciar sesión.
pub const TRAY_ARG: &str = "--bandeja";
#[cfg_attr(not(windows), allow(dead_code))]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg_attr(not(windows), allow(dead_code))]
const VALUE_NAME: &str = "FortniteChestCounter";

/// Orden que se guarda: la ruta entre comillas (puede tener espacios) y el
/// argumento para arrancar en la bandeja.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn command_line(exe: &std::path::Path) -> String {
    format!("\"{}\" {TRAY_ARG}", exe.display())
}

/// ¿Se abrió el programa al iniciar sesión?
pub fn started_by_system(args: &[String]) -> bool {
    args.iter().any(|arg| arg == TRAY_ARG)
}

/// Activa o quita el inicio con Windows. Al activarlo se escribe la ruta
/// actual del programa, así que también se corrige si se movió el `.exe`.
#[cfg(windows)]
pub fn set(enabled: bool) -> Result<(), String> {
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
    };
    use windows::core::HSTRING;

    let key = HSTRING::from(RUN_KEY);
    let name = HSTRING::from(VALUE_NAME);
    // SAFETY: cadenas válidas terminadas en cero; el búfer de datos vive
    // durante la llamada y su tamaño en bytes incluye el cero final.
    let result = unsafe {
        if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let data: Vec<u16> = command_line(&exe).encode_utf16().chain([0]).collect();
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &key,
                &name,
                REG_SZ.0,
                Some(data.as_ptr().cast()),
                (data.len() * 2) as u32,
            )
        } else {
            let result = RegDeleteKeyValueW(HKEY_CURRENT_USER, &key, &name);
            if result == ERROR_FILE_NOT_FOUND {
                ERROR_SUCCESS
            } else {
                result
            }
        }
    };
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!(
            "no se pudo cambiar el inicio con Windows (código {})",
            result.0
        ))
    }
}

#[cfg(not(windows))]
pub fn set(_enabled: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_orden_lleva_la_ruta_entre_comillas_y_el_argumento_de_bandeja() {
        let exe = std::path::Path::new(r"C:\Users\Ana Pérez\Juegos\FortniteChestCounter.exe");
        assert_eq!(
            command_line(exe),
            r#""C:\Users\Ana Pérez\Juegos\FortniteChestCounter.exe" --bandeja"#
        );
    }

    #[test]
    fn reconoce_el_arranque_desde_el_inicio_de_sesion() {
        assert!(started_by_system(&["x.exe".into(), TRAY_ARG.into()]));
        assert!(!started_by_system(&["x.exe".into()]));
        assert!(!started_by_system(&[
            "x.exe".into(),
            "--esperar".into(),
            "12".into()
        ]));
    }
}
