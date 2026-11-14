//! Carpetas de la app. Se llaman como la app ("FortniteChestCounter"), no
//! como su identificador interno, que es largo y poco claro para el usuario.

use std::fs;
use std::io;
use std::path::Path;

/// Nombre de las carpetas de datos.
pub const APP_DIR: &str = "FortniteChestCounter";

/// Archivos de la base de datos (con su registro de escritura de SQLite).
const DATABASE_FILES: [&str; 3] = ["chests.db", "chests.db-wal", "chests.db-shm"];

/// Mueve la base de datos de la carpeta antigua (`old`) a la nueva (`new`) si
/// la nueva aún no tiene una. Devuelve `true` si movió algo. Si la carpeta
/// antigua queda sin base de datos, se borra.
pub fn migrate_database(old: &Path, new: &Path) -> io::Result<bool> {
    if old == new || !old.join(DATABASE_FILES[0]).is_file() || new.join(DATABASE_FILES[0]).exists()
    {
        return Ok(false);
    }
    fs::create_dir_all(new)?;
    for name in DATABASE_FILES {
        let from = old.join(name);
        if from.is_file() {
            move_file(&from, &new.join(name))?;
        }
    }
    let _ = fs::remove_dir_all(old);
    Ok(true)
}

/// Mueve un archivo; si renombrar no es posible (otro disco), lo copia y borra.
fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to)?;
    fs::remove_file(from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fcc-paths-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn mueve_la_base_de_datos_y_borra_la_carpeta_antigua() {
        let root = temp("mueve");
        let (old, new) = (root.join("io.github.ejemplo"), root.join(APP_DIR));
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("chests.db"), "base").unwrap();
        fs::write(old.join("chests.db-wal"), "registro").unwrap();

        assert!(migrate_database(&old, &new).unwrap());
        assert_eq!(fs::read_to_string(new.join("chests.db")).unwrap(), "base");
        assert_eq!(
            fs::read_to_string(new.join("chests.db-wal")).unwrap(),
            "registro"
        );
        assert!(!old.exists(), "la carpeta antigua desaparece");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_pisa_una_base_de_datos_que_ya_existe() {
        let root = temp("no-pisa");
        let (old, new) = (root.join("antigua"), root.join(APP_DIR));
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(old.join("chests.db"), "vieja").unwrap();
        fs::write(new.join("chests.db"), "nueva").unwrap();

        assert!(!migrate_database(&old, &new).unwrap());
        assert_eq!(fs::read_to_string(new.join("chests.db")).unwrap(), "nueva");
        assert!(old.join("chests.db").exists(), "no se borra nada");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sin_carpeta_antigua_no_hace_nada() {
        let root = temp("nada");
        assert!(!migrate_database(&root.join("no-existe"), &root.join(APP_DIR)).unwrap());
        assert!(!root.join(APP_DIR).exists());
        fs::remove_dir_all(root).unwrap();
    }
}
