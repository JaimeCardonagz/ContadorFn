//! ¿Consigue el programa ver el juego? Vigila los fallos que el usuario puede
//! arreglar (captura que falla, imagen en negro, ventana diminuta) para
//! avisarle en el panel con una pista de qué hacer.

use serde::Serialize;

/// Tiempo seguido viendo negro antes de avisar: los fundidos del juego duran
/// mucho menos.
const BLACK_MS: i64 = 20_000;
/// Intentos de captura fallidos seguidos antes de avisar (cada uno, 5 s).
const FAILURES: u32 = 2;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Problem {
    #[default]
    None,
    /// No se puede capturar la pantalla.
    Capture,
    /// La imagen del juego sale en negro (pantalla completa exclusiva).
    Black,
    /// La ventana del juego es demasiado pequeña para leerla.
    Small,
}

#[derive(Debug, Default)]
pub struct Vision {
    failures: u32,
    black_since: Option<i64>,
    problem: Problem,
}

impl Vision {
    #[cfg(test)]
    pub fn problem(&self) -> Problem {
        self.problem
    }

    /// Cambia el problema; devuelve el nuevo solo si es distinto.
    fn set(&mut self, problem: Problem) -> Option<Problem> {
        (self.problem != problem).then(|| {
            self.problem = problem;
            problem
        })
    }

    /// Un intento de captura falló.
    pub fn capture_failed(&mut self) -> Option<Problem> {
        self.failures += 1;
        if self.failures >= FAILURES {
            self.set(Problem::Capture)
        } else {
            None
        }
    }

    /// Se capturó un recorte del juego; `black` si es todo negro.
    pub fn frame(&mut self, now: i64, black: bool) -> Option<Problem> {
        self.failures = 0;
        if !black {
            self.black_since = None;
            return self.set(Problem::None);
        }
        let since = *self.black_since.get_or_insert(now);
        if now - since >= BLACK_MS {
            self.set(Problem::Black)
        } else if self.problem == Problem::Black {
            None
        } else {
            self.set(Problem::None)
        }
    }

    /// El juego no está delante; `small` si su ventana sí lo está pero es diminuta.
    pub fn no_game(&mut self, small: bool) -> Option<Problem> {
        self.failures = 0;
        self.black_since = None;
        self.set(if small { Problem::Small } else { Problem::None })
    }
}

/// ¿Es negro todo el recorte? Se corta en el primer píxel con luz, así que en
/// juego normal cuesta casi nada.
pub fn is_black(rgb: &[u8]) -> bool {
    !rgb.is_empty() && rgb.iter().all(|&value| value < 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avisa_si_la_captura_falla_dos_veces_seguidas() {
        let mut v = Vision::default();
        assert_eq!(v.capture_failed(), None, "un fallo suelto no avisa");
        assert_eq!(v.frame(0, false), None);
        assert_eq!(v.capture_failed(), None);
        assert_eq!(v.capture_failed(), Some(Problem::Capture));
        assert_eq!(v.capture_failed(), None, "solo avisa del cambio");
        assert_eq!(v.frame(1, false), Some(Problem::None));
    }

    #[test]
    fn avisa_del_negro_solo_si_dura_20_segundos() {
        let mut v = Vision::default();
        assert_eq!(v.frame(0, true), None);
        assert_eq!(v.frame(19_000, true), None, "un fundido no avisa");
        assert_eq!(v.frame(20_000, true), Some(Problem::Black));
        assert_eq!(v.frame(25_000, true), None);
        assert_eq!(v.frame(26_000, false), Some(Problem::None));
        // Un momento con imagen reinicia la cuenta.
        assert_eq!(v.frame(30_000, true), None);
        assert_eq!(v.frame(45_000, false), None);
        assert_eq!(v.frame(50_000, true), None);
        assert_eq!(v.frame(65_000, true), None);
        assert_eq!(v.frame(70_000, true), Some(Problem::Black));
    }

    #[test]
    fn sin_el_juego_delante_solo_avisa_de_la_ventana_diminuta() {
        let mut v = Vision::default();
        assert_eq!(v.no_game(false), None);
        assert_eq!(v.no_game(true), Some(Problem::Small));
        assert_eq!(v.no_game(false), Some(Problem::None));
        v.capture_failed();
        v.capture_failed();
        assert_eq!(v.problem(), Problem::Capture);
        assert_eq!(v.no_game(false), Some(Problem::None));
    }

    #[test]
    fn negro_es_sin_ningun_pixel_con_luz() {
        assert!(is_black(&[0, 3, 15, 2, 0, 0]));
        assert!(!is_black(&[0, 3, 16, 2, 0, 0]));
        assert!(!is_black(&[]));
    }
}
