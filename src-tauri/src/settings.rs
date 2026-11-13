//! Ajustes del usuario. Se guardan como JSON en la tabla `meta`.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

/// Con qué abre los contenedores el jugador.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "device", rename_all = "lowercase")]
pub enum Binding {
    /// Tecla: código de tecla virtual de Windows (coincide con `keyCode` web).
    Key { code: u16, label: String },
    /// Botón del ratón, numeración web: 0 izquierdo, 1 central, 2 derecho,
    /// 3 y 4 laterales.
    Mouse { button: u8 },
    /// Botón del mando, numeración del estándar web: 0 A/✕, 1 B/○, 2 X/□,
    /// 3 Y/△, 4 LB, 5 RB, 6 LT, 7 RT, 8 Atrás, 9 Inicio, 10 L3, 11 R3,
    /// 12-15 cruceta (arriba, abajo, izquierda, derecha).
    Pad { button: u8 },
}

impl Default for Binding {
    fn default() -> Self {
        Binding::Key {
            code: 0x45,
            label: "E".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Idioma de la interfaz: "es", "en" o "" (aún sin elegir).
    pub language: String,
    pub binding: Binding,
    /// Contar solo si ese botón se pulsó poco antes del aviso.
    pub require_input: bool,
    /// Excluir la ventana de la app de capturas y grabaciones. Así la propia
    /// app ve el juego aunque su ventana lo tape; a cambio no sale en OBS.
    pub hide_from_capture: bool,
    /// Mostrar el contador sobre el juego; si no, la app queda en segundo plano.
    pub show_overlay: bool,
    /// Guardar un recorte de cada aviso leído, para depurar fallos.
    pub save_samples: bool,
    /// Esquina del contador: "top-left", "top-right", "bottom-left" o "bottom-right".
    pub corner: String,
    /// Ya se preguntó al usuario cuántos cofres y partidas llevaba.
    pub setup_done: bool,
    /// Número de partida en el contador: "off", "day" (de la sesión) o "total".
    pub overlay_match: String,
    /// Mostrar el total histórico de cofres en el contador.
    pub overlay_total: bool,
    /// Cofres de la partida en el contador: "off", "sum" (los tres sumados) o
    /// "types" (tesoro, raro y espíritus por separado).
    pub overlay_chests: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: String::new(),
            binding: Binding::default(),
            require_input: false,
            hide_from_capture: true,
            show_overlay: true,
            save_samples: false,
            corner: "top-left".into(),
            setup_done: false,
            overlay_match: "total".into(),
            overlay_total: true,
            overlay_chests: "sum".into(),
        }
    }
}

impl Settings {
    pub fn from_json(json: Option<&str>) -> Self {
        json.and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// Fija el idioma de los mensajes que genera el backend.
pub fn set_language(language: &str) {
    ENGLISH.store(language == "en", Relaxed);
}

/// Elige el texto según el idioma de la interfaz.
pub fn tr<T>(es: T, en: T) -> T {
    if ENGLISH.load(Relaxed) { en } else { es }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_ajustes_guardados_usa_la_tecla_e() {
        let settings = Settings::from_json(None);
        assert_eq!(settings.binding, Binding::default());
        assert!(!settings.require_input);
        assert!(settings.hide_from_capture);
        assert_eq!(Settings::from_json(Some("no es json")), settings);
    }

    #[test]
    fn los_ajustes_sobreviven_a_guardarse_y_leerse() {
        for binding in [
            Binding::Key {
                code: 0x46,
                label: "F".into(),
            },
            Binding::Mouse { button: 3 },
            Binding::Pad { button: 2 },
        ] {
            let settings = Settings {
                language: "en".into(),
                binding,
                require_input: true,
                hide_from_capture: false,
                show_overlay: false,
                save_samples: true,
                corner: "bottom-right".into(),
                setup_done: true,
                overlay_match: "day".into(),
                overlay_total: false,
                overlay_chests: "types".into(),
            };
            assert_eq!(Settings::from_json(Some(&settings.to_json())), settings);
        }
    }

    #[test]
    fn el_formato_es_el_que_envia_la_interfaz() {
        let json =
            r#"{"language":"es","binding":{"device":"mouse","button":4},"requireInput":false}"#;
        let settings = Settings::from_json(Some(json));
        assert_eq!(settings.binding, Binding::Mouse { button: 4 });
        // Ajustes de una versión anterior, sin algún campo: se completan.
        let old = Settings::from_json(Some(r#"{"language":"en"}"#));
        assert_eq!(
            (old.language.as_str(), old.binding),
            ("en", Binding::default())
        );
        assert!(
            old.hide_from_capture,
            "lo que falta toma su valor por defecto"
        );
    }
}
