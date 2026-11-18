//! Copias de seguridad automáticas de las estadísticas: una al día en la
//! carpeta `copias` junto a la base de datos, guardando solo las 7 últimas.
//! Además, justo antes de reiniciar las estadísticas se guarda otra aparte.

use crate::tracker::Tracker;
use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Carpeta de las copias, dentro de la carpeta de datos.
pub const DIR: &str = "copias";
/// Copias diarias que se conservan.
const KEEP: usize = 7;
const PREFIX: &str = "estadisticas-";
const EXT: &str = ".db";
/// La copia de antes de reiniciar: solo una, la última.
const BEFORE_RESET: &str = "antes-de-reiniciar.db";

/// Fecha local de hoy como "AAAA-MM-DD" (en Windows; en otros sistemas, UTC).
pub fn today(now_ms: i64) -> String {
    #[cfg(windows)]
    {
        let _ = now_ms;
        // SAFETY: GetLocalTime solo rellena la estructura devuelta.
        let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay)
    }
    #[cfg(not(windows))]
    {
        utc_date(now_ms)
    }
}

/// Fecha UTC "AAAA-MM-DD" de un instante en milisegundos (algoritmo de días
/// civiles de Howard Hinnant).
#[cfg_attr(windows, allow(dead_code))]
fn utc_date(now_ms: i64) -> String {
    let days = now_ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Copias diarias que hay, de la más antigua a la más nueva.
fn daily_copies(dir: &Path) -> Vec<PathBuf> {
    let mut copies: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(PREFIX) && name.ends_with(EXT))
        })
        .collect();
    copies.sort();
    copies
}

/// Escribe la copia en un archivo temporal y lo renombra al final: así nunca
/// queda una copia a medias con el nombre bueno.
fn write_copy(tracker: &Tracker, target: &Path) -> io::Result<()> {
    let temp = target.with_extension("tmp");
    let _ = fs::remove_file(&temp);
    tracker
        .backup_to(&temp)
        .map_err(|e| io::Error::other(e.to_string()))?;
    fs::rename(&temp, target)
}

/// Hace la copia de hoy si aún no existe y borra las que sobran. Devuelve si
/// hizo una copia nueva.
pub fn daily(tracker: &Tracker, dir: &Path, day: &str) -> io::Result<bool> {
    fs::create_dir_all(dir)?;
    let target = dir.join(format!("{PREFIX}{day}{EXT}"));
    if target.exists() {
        return Ok(false);
    }
    write_copy(tracker, &target)?;
    let copies = daily_copies(dir);
    for old in copies.iter().take(copies.len().saturating_sub(KEEP)) {
        let _ = fs::remove_file(old);
    }
    Ok(true)
}

/// Copia de lo que había justo antes de reiniciar las estadísticas.
pub fn before_reset(tracker: &Tracker, dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let target = dir.join(BEFORE_RESET);
    let _ = fs::remove_file(&target);
    write_copy(tracker, &target)
}

/// Lo que enseña Configuración sobre las copias.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    /// Copias diarias guardadas.
    pub count: usize,
    /// Fecha ("AAAA-MM-DD") de la más reciente.
    pub latest: Option<String>,
}

pub fn info(dir: &Path) -> BackupInfo {
    let copies = daily_copies(dir);
    let latest = copies.last().and_then(|path| {
        let name = path.file_name()?.to_str()?;
        Some(name.strip_prefix(PREFIX)?.strip_suffix(EXT)?.to_string())
    });
    BackupInfo {
        count: copies.len(),
        latest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{ChestType, Store};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fcc-copias-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn tracker_with_chest() -> Tracker {
        let mut t = Tracker::new(Store::open_in_memory().unwrap(), 0, 3_600_000).unwrap();
        t.chest_opened(1_000, ChestType::Rare, 1.0, 0b11_1111)
            .unwrap();
        t
    }

    #[test]
    fn la_fecha_utc_es_correcta() {
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(951_782_400_000), "2000-02-29");
        assert_eq!(utc_date(1_795_000_000_000), "2026-11-18");
        assert_eq!(utc_date(1_795_046_399_999), "2026-11-18");
    }

    #[test]
    fn una_copia_al_dia_y_solo_las_siete_ultimas() {
        let dir = temp_dir("diarias");
        let t = tracker_with_chest();
        assert!(daily(&t, &dir, "2026-11-01").unwrap());
        assert!(!daily(&t, &dir, "2026-11-01").unwrap(), "una sola por día");
        for day in 2..=9 {
            daily(&t, &dir, &format!("2026-11-{day:02}")).unwrap();
        }
        let names: Vec<_> = daily_copies(&dir)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 7);
        assert_eq!(names[0], "estadisticas-2026-11-03.db");
        assert_eq!(names[6], "estadisticas-2026-11-09.db");
        let info = info(&dir);
        assert_eq!(
            (info.count, info.latest.as_deref()),
            (7, Some("2026-11-09"))
        );
        // La copia se puede abrir y tiene los datos.
        let copy = Store::open(&dir.join("estadisticas-2026-11-09.db")).unwrap();
        let back = Tracker::new(copy, 2_000, 3_600_000).unwrap();
        assert_eq!(back.stats().unwrap().total_counts.rare, 1);
        assert!(!dir.join("estadisticas-2026-11-09.tmp").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn antes_de_reiniciar_se_guarda_aparte_y_no_cuenta_como_diaria() {
        let dir = temp_dir("reinicio");
        let t = tracker_with_chest();
        before_reset(&t, &dir).unwrap();
        before_reset(&t, &dir).unwrap();
        assert!(dir.join(BEFORE_RESET).exists());
        assert_eq!(info(&dir).count, 0);
        fs::remove_dir_all(&dir).unwrap();
    }
}
