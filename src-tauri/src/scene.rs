//! Detección de partida: ¿está en pantalla el HUD de juego?
//!
//! La señal es la barra de vida, abajo a la izquierda. Medido en los vídeos
//! del usuario: aparece en la isla inicial, se oculta unos segundos durante el
//! autobús, vuelve al saltar y desaparece al ser eliminado o volver al lobby.
//! En la pantalla de eliminación hay otra barra verde (la del rival), pero en
//! otro sitio.

use crate::capture::Frame;

/// Parte izquierda de la barra de vida a 768 px de alto: x0, y0, x1, y1. El
/// HUD está anclado al borde izquierdo y escala con la altura de la pantalla.
/// Está llena siempre que queda algo más del 15 % de vida.
const BAR: (u32, u32, u32, u32) = (54, 702, 100, 720);
/// Fracción mínima de píxeles verdes para dar la barra por visible.
const MIN_GREEN: f32 = 0.6;

/// Lecturas seguidas con la barra visible para dar por empezada una partida.
const START_SAMPLES: u32 = 3;
/// Durante los primeros minutos la barra se oculta en el autobús: se tolera
/// más tiempo sin verla antes de dar la partida por terminada.
const EARLY_PHASE_MS: i64 = 180_000;
const EARLY_GRACE_MS: i64 = 60_000;
/// Después, sin barra durante este tiempo la partida ha terminado. Abrir el
/// mapa o el inventario la oculta solo unos segundos.
const GRACE_MS: i64 = 25_000;

/// Rectángulo de la barra en una pantalla de `height` píxeles de alto:
/// (x, y, ancho, alto).
pub fn bar_rect(height: u32) -> (u32, u32, u32, u32) {
    let scale = |v: u32| v * height / 768;
    let (x0, y0, x1, y1) = (scale(BAR.0), scale(BAR.1), scale(BAR.2), scale(BAR.3));
    (x0, y0, (x1 - x0).max(1), (y1 - y0).max(1))
}

/// ¿Es `zone` (un recorte de `bar_rect`) la barra de vida?
pub fn health_bar_visible(zone: &Frame) -> bool {
    let pixels = zone.rgb.as_chunks::<3>().0;
    if pixels.is_empty() {
        return false;
    }
    let green = pixels.iter().filter(|&&rgb| is_bar_green(rgb)).count();
    green as f32 / pixels.len() as f32 >= MIN_GREEN
}

/// Verde de la barra de vida: del verde puro al verde amarillento.
fn is_bar_green([r, g, b]: [u8; 3]) -> bool {
    // En i16: con u8 saturado, el blanco puro pasaría por verde.
    let (r, g, b) = (r as i16, g as i16, b as i16);
    g >= 150 && g - r >= 40 && g - b >= 70
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchEvent {
    Started,
    /// Terminó en el instante `at` (cuando dejó de verse el HUD).
    Ended {
        at: i64,
    },
}

/// Convierte lecturas sucesivas de la barra en inicio y fin de partida.
#[derive(Default)]
pub struct MatchWatcher {
    in_match: bool,
    started_at: i64,
    visible_streak: u32,
    absent_since: Option<i64>,
}

impl MatchWatcher {
    pub fn step(&mut self, now: i64, bar_visible: bool) -> Option<MatchEvent> {
        if !self.in_match {
            self.visible_streak = if bar_visible {
                self.visible_streak + 1
            } else {
                0
            };
            if self.visible_streak < START_SAMPLES {
                return None;
            }
            self.in_match = true;
            self.started_at = now;
            self.visible_streak = 0;
            self.absent_since = None;
            return Some(MatchEvent::Started);
        }
        if bar_visible {
            self.absent_since = None;
            return None;
        }
        let since = *self.absent_since.get_or_insert(now);
        let grace = if since - self.started_at < EARLY_PHASE_MS {
            EARLY_GRACE_MS
        } else {
            GRACE_MS
        };
        if now - since < grace {
            return None;
        }
        self.in_match = false;
        self.absent_since = None;
        Some(MatchEvent::Ended { at: since })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: i64 = 1_000;

    /// Pasa lecturas, una por segundo, y devuelve los eventos con su segundo.
    fn run(
        watcher: &mut MatchWatcher,
        from: i64,
        readings: &[(bool, i64)],
    ) -> Vec<(i64, MatchEvent)> {
        let mut events = Vec::new();
        let mut second = from;
        for &(visible, seconds) in readings {
            for _ in 0..seconds {
                if let Some(event) = watcher.step(second * SEC, visible) {
                    events.push((second, event));
                }
                second += 1;
            }
        }
        events
    }

    #[test]
    fn la_partida_empieza_al_ver_el_hud_varias_veces_seguidas() {
        let mut w = MatchWatcher::default();
        let events = run(&mut w, 0, &[(false, 10), (true, 1), (false, 2), (true, 5)]);
        // Una lectura suelta no basta; la tercera seguida sí.
        assert_eq!(events, [(15, MatchEvent::Started)]);
    }

    #[test]
    fn el_autobus_oculta_el_hud_sin_terminar_la_partida() {
        let mut w = MatchWatcher::default();
        // Isla inicial 30 s, autobús 45 s sin HUD, y luego la partida.
        let events = run(&mut w, 0, &[(true, 30), (false, 45), (true, 300)]);
        assert_eq!(events, [(2, MatchEvent::Started)]);
    }

    #[test]
    fn abrir_el_mapa_unos_segundos_no_termina_la_partida() {
        let mut w = MatchWatcher::default();
        let events = run(
            &mut w,
            0,
            &[
                (true, 400),
                (false, 20),
                (true, 100),
                (false, 24),
                (true, 50),
            ],
        );
        assert_eq!(events, [(2, MatchEvent::Started)]);
    }

    #[test]
    fn sin_hud_la_partida_termina_en_el_momento_en_que_desaparecio() {
        let mut w = MatchWatcher::default();
        let events = run(&mut w, 0, &[(true, 600), (false, 120)]);
        assert_eq!(
            events,
            [
                (2, MatchEvent::Started),
                // Se decide a los 25 s, pero el final es cuando dejó de verse.
                (625, MatchEvent::Ended { at: 600 * SEC }),
            ]
        );
    }

    #[test]
    fn salir_en_los_primeros_minutos_tambien_termina_la_partida() {
        let mut w = MatchWatcher::default();
        let events = run(&mut w, 0, &[(true, 40), (false, 200)]);
        assert_eq!(
            events,
            [
                (2, MatchEvent::Started),
                (100, MatchEvent::Ended { at: 40 * SEC })
            ]
        );
    }

    #[test]
    fn dos_partidas_seguidas_se_cuentan_por_separado() {
        let mut w = MatchWatcher::default();
        let events = run(
            &mut w,
            0,
            &[(true, 500), (false, 90), (true, 500), (false, 60)],
        );
        let kinds: Vec<_> = events.iter().map(|(_, e)| *e).collect();
        assert_eq!(
            kinds,
            [
                MatchEvent::Started,
                MatchEvent::Ended { at: 500 * SEC },
                MatchEvent::Started,
                MatchEvent::Ended { at: 1_090 * SEC },
            ]
        );
    }

    fn zone(color: [u8; 3]) -> Frame {
        let (_, _, width, height) = bar_rect(768);
        Frame {
            width,
            height,
            rgb: color.repeat((width * height) as usize),
        }
    }

    #[test]
    fn la_barra_es_verde_y_otros_colores_no_cuentan() {
        assert!(
            health_bar_visible(&zone([60, 200, 60])),
            "verde del lado izquierdo"
        );
        assert!(
            health_bar_visible(&zone([160, 230, 40])),
            "verde amarillento"
        );
        for other in [
            [30, 30, 40],
            [255, 255, 255],
            [40, 160, 230],
            [200, 60, 50],
            [90, 120, 80],
        ] {
            assert!(!health_bar_visible(&zone(other)), "{other:?}");
        }
    }

    #[test]
    fn la_zona_de_la_barra_escala_con_la_altura() {
        assert_eq!(bar_rect(768), (54, 702, 46, 18));
        assert_eq!(bar_rect(1080), (75, 987, 65, 25));
    }

    /// Recortes de la barra a 1 fps de los vídeos del usuario (locales; ver
    /// tools/extraer_avisos.sh). "Cofres Prueba" va del lobby a la eliminación.
    #[test]
    fn videos_completos() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../Debug/barra-1fps");
        let Ok(entries) = std::fs::read_dir(root.join("cofres-prueba")) else {
            eprintln!("sin recortes de la barra: prueba omitida");
            return;
        };
        let mut files: Vec<_> = entries.map(|e| e.unwrap().path()).collect();
        files.sort();
        let visible: Vec<bool> = files
            .iter()
            .map(|path| {
                let image = image::open(path).unwrap().into_rgb8();
                health_bar_visible(&Frame {
                    width: image.width(),
                    height: image.height(),
                    rgb: image.into_raw(),
                })
            })
            .collect();

        let mut w = MatchWatcher::default();
        let mut events = Vec::new();
        for (second, &bar) in visible.iter().enumerate() {
            if let Some(event) = w.step(second as i64 * SEC, bar) {
                events.push((second, event));
            }
        }
        // Lobby hasta el segundo 9; el HUD aparece en la isla inicial. Ni el
        // autobús (17-25 s) ni el cambio de ventana del 122 la terminan.
        assert_eq!(events.len(), 1, "{events:?}");
        assert!(
            matches!(events[0], (10..=13, MatchEvent::Started)),
            "{events:?}"
        );
        // Lo eliminan en el 128: desde ahí no se ve la barra, y con el vídeo
        // seguido de lobby la partida termina en ese momento.
        assert!(visible[128..].iter().all(|&bar| !bar));
        let mut end = None;
        for second in visible.len()..visible.len() + 120 {
            end = end.or(w.step(second as i64 * SEC, false));
        }
        assert!(
            matches!(end, Some(MatchEvent::Ended { at }) if (127..=129).contains(&(at / SEC))),
            "{end:?}"
        );
    }
}
