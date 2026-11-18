//! El panel recuerda su tamaño y su posición entre aperturas del programa.
//! Se guardan en la base de datos (clave `panel` de `meta`), en píxeles
//! físicos. Al abrir se comprueba que siga cabiendo en alguna pantalla.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

/// Clave en la tabla `meta`.
pub const META_KEY: &str = "panel";
/// Alto de la franja superior (barra de título) que debe verse para poder
/// arrastrar el panel, y ancho mínimo visible de esa franja.
const GRIP_HEIGHT: i64 = 32;
const GRIP_MIN_WIDTH: i64 = 120;
/// Tamaño mínimo del panel (el mismo que impone la ventana).
const MIN_SIZE: (u32, u32) = (320, 360);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Geometry {
    /// Esquina superior izquierda de la ventana (con marco).
    pub x: i32,
    pub y: i32,
    /// Tamaño del contenido (sin marco).
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub maximized: bool,
}

/// Rectángulo útil de una pantalla (sin la barra de tareas): x, y, ancho, alto.
pub type Screen = (i32, i32, u32, u32);

/// Geometría guardada pendiente de escribir en la base de datos.
#[derive(Default)]
pub struct PanelState(Mutex<Option<Geometry>>);

/// Ajusta la geometría guardada a las pantallas de ahora. `None` si la barra
/// de título ya no se ve en ninguna (pantalla desconectada, resolución
/// menor…): entonces el panel vuelve a su sitio por defecto.
pub fn fit(saved: Geometry, screens: &[Screen]) -> Option<Geometry> {
    let grip_overlap = |&(sx, sy, sw, sh): &Screen| {
        let (left, right) = (
            i64::from(saved.x).max(sx.into()),
            (i64::from(saved.x) + i64::from(saved.width)).min(i64::from(sx) + i64::from(sw)),
        );
        let (top, bottom) = (
            i64::from(saved.y).max(sy.into()),
            (i64::from(saved.y) + GRIP_HEIGHT).min(i64::from(sy) + i64::from(sh)),
        );
        if right - left >= GRIP_MIN_WIDTH && bottom - top >= GRIP_HEIGHT / 2 {
            (right - left) * (bottom - top)
        } else {
            0
        }
    };
    let screen = screens
        .iter()
        .max_by_key(|screen| grip_overlap(screen))
        .filter(|screen| grip_overlap(screen) > 0)?;
    // Si es más grande que esa pantalla, se encoge para que quepa.
    let (_, _, sw, sh) = *screen;
    Some(Geometry {
        width: saved.width.clamp(MIN_SIZE.0, sw.max(MIN_SIZE.0)),
        height: saved.height.clamp(MIN_SIZE.1, sh.max(MIN_SIZE.1)),
        ..saved
    })
}

/// Pantallas conectadas, como rectángulos útiles.
fn screens(window: &WebviewWindow) -> Vec<Screen> {
    window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|monitor| {
            let area = monitor.work_area();
            (
                area.position.x,
                area.position.y,
                area.size.width,
                area.size.height,
            )
        })
        .collect()
}

/// Coloca el panel donde estaba la última vez, si sigue cabiendo.
pub fn restore(window: &WebviewWindow, saved: Option<&str>) {
    let Some(saved) = saved.and_then(|json| serde_json::from_str::<Geometry>(json).ok()) else {
        return;
    };
    match fit(saved, &screens(window)) {
        Some(geometry) => {
            let _ = window.set_size(PhysicalSize::new(geometry.width, geometry.height));
            let _ = window.set_position(PhysicalPosition::new(geometry.x, geometry.y));
            if geometry.maximized {
                let _ = window.maximize();
            }
        }
        None => {
            // Ya no se vería: tamaño guardado (encogido si no cabe en la
            // pantalla principal) y centrado.
            if let Ok(Some(monitor)) = window.primary_monitor() {
                let area = monitor.work_area();
                let _ = window.set_size(PhysicalSize::new(
                    saved
                        .width
                        .clamp(MIN_SIZE.0, area.size.width.max(MIN_SIZE.0)),
                    saved
                        .height
                        .clamp(MIN_SIZE.1, area.size.height.max(MIN_SIZE.1)),
                ));
            }
            let _ = window.center();
        }
    }
}

/// Toma nota de dónde está el panel (al moverlo o cambiar su tamaño). Se
/// escribe en la base de datos más tarde, con `take`.
pub fn remember(window: &WebviewWindow) {
    let state = window.state::<PanelState>();
    let Ok(mut pending) = state.0.lock() else {
        return;
    };
    // Minimizado u oculto, la posición no sirve (Windows usa -32000).
    if window.is_minimized().unwrap_or(true) || !window.is_visible().unwrap_or(false) {
        return;
    }
    if window.is_maximized().unwrap_or(false) {
        // Se guarda que está maximizado, pero se conserva el tamaño normal.
        if let Some(geometry) = pending.as_mut() {
            geometry.maximized = true;
        }
        return;
    }
    if let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) {
        *pending = Some(Geometry {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
            maximized: false,
        });
    }
}

/// La geometría pendiente de guardar, en JSON (y la deja como guardada).
pub fn take(app: &AppHandle) -> Option<String> {
    let state = app.try_state::<PanelState>()?;
    let geometry = state.0.lock().ok()?.take()?;
    serde_json::to_string(&geometry).ok()
}

/// Deja anotada la geometría ya guardada, para que maximizar conserve el
/// tamaño normal aunque no se haya movido el panel en esta sesión.
pub fn seed(app: &AppHandle, saved: Option<&str>) {
    if let (Some(state), Some(geometry)) = (
        app.try_state::<PanelState>(),
        saved.and_then(|json| serde_json::from_str::<Geometry>(json).ok()),
    ) && let Ok(mut pending) = state.0.lock()
    {
        *pending = Some(geometry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_HD: Screen = (0, 0, 1920, 1040);

    fn at(x: i32, y: i32, width: u32, height: u32) -> Geometry {
        Geometry {
            x,
            y,
            width,
            height,
            maximized: false,
        }
    }

    #[test]
    fn si_cabe_se_queda_donde_estaba() {
        let saved = at(1300, 200, 440, 620);
        assert_eq!(fit(saved, &[FULL_HD]), Some(saved));
        // Medio fuera por la derecha, pero con la barra de título a mano.
        let partly = at(1700, 900, 440, 620);
        assert_eq!(fit(partly, &[FULL_HD]), Some(partly));
    }

    #[test]
    fn en_una_pantalla_que_ya_no_esta_vuelve_a_su_sitio() {
        // Estaba en un segundo monitor a la derecha, que se desconectó.
        assert_eq!(fit(at(2200, 100, 440, 620), &[FULL_HD]), None);
        // Con el segundo monitor conectado, se queda.
        let second: Screen = (1920, 0, 1280, 1024);
        assert!(fit(at(2200, 100, 440, 620), &[FULL_HD, second]).is_some());
        // Barra de título por encima de la pantalla: no se podría arrastrar.
        assert_eq!(fit(at(100, -500, 440, 620), &[FULL_HD]), None);
        // Solo asoma una esquina diminuta.
        assert_eq!(fit(at(1850, 100, 440, 620), &[FULL_HD]), None);
    }

    #[test]
    fn si_es_mas_grande_que_la_pantalla_se_encoge() {
        let small: Screen = (0, 0, 1366, 728);
        let fitted = fit(at(0, 0, 1600, 1000), &[small]).unwrap();
        assert_eq!((fitted.width, fitted.height), (1366, 728));
        let tiny = fit(at(0, 0, 200, 100), &[small]).unwrap();
        assert_eq!(
            (tiny.width, tiny.height),
            MIN_SIZE,
            "nunca menos del mínimo"
        );
    }

    #[test]
    fn la_geometria_se_guarda_y_se_lee() {
        let saved = Geometry {
            maximized: true,
            ..at(-1500, 40, 500, 700)
        };
        let json = serde_json::to_string(&saved).unwrap();
        assert_eq!(serde_json::from_str::<Geometry>(&json).unwrap(), saved);
        let old: Geometry = serde_json::from_str(r#"{"x":1,"y":2,"width":3,"height":4}"#).unwrap();
        assert!(!old.maximized);
    }
}
