//! Bucle de detección: mira la zona del aviso de experiencia unas diez veces
//! por segundo y cuenta un contenedor cada vez que aparece un aviso con un
//! tipo conocido.
//!
//! Es la primera versión funcional y solo usa el aviso (S2 + S5). Limitación
//! conocida: dos contenedores del mismo tipo abiertos antes de que el primer
//! aviso desaparezca (~2 s) cuentan como uno. Se resolverá al añadir el
//! recuadro "BUSCAR" y el anillo (`detect.rs`), que distinguen cada apertura.

use crate::capture::{self, Frame};
use crate::detect::{S1_PROMPT, S2_TYPE, S3_INPUT, S4_PROGRESS, S5_NOTICE};
use crate::diag::Diag;
use crate::fuse::{self, FuseCounter};
use crate::notice::{self, Reading};
use crate::now_ms;
use crate::scene::{self, MatchEvent, MatchWatcher};
use crate::store::{ChestType, EndReason};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;

const INTERVAL: Duration = Duration::from_millis(100);
/// Cada cuántas vueltas se mira si está el HUD de partida (una vez por segundo).
const SCENE_EVERY: u32 = 10;
/// Espera antes de reintentar cuando no se puede capturar.
const RETRY: Duration = Duration::from_secs(5);
/// Lecturas seguidas del mismo tipo para darlo por visto.
const CONFIRM_FRAMES: u32 = 2;
/// Lecturas seguidas sin el aviso para darlo por desaparecido.
const CLEAR_FRAMES: u32 = 5;
/// Avisos desconocidos que se guardan como muestra, como mucho, por ejecución.
const MAX_UNKNOWN_SAMPLES: u32 = 200;
const UNKNOWN_SAMPLE_GAP_MS: i64 = 3_000;
/// El botón de abrir cuenta como pulsado para un aviso si se pulsó en este
/// margen antes de verlo (anillo ~0,7 s + ~0,6 s hasta el aviso, con holgura).
const INPUT_WINDOW_MS: i64 = 4_000;

/// Convierte lecturas sucesivas del aviso en aperturas: una por cada aviso
/// nuevo, no una por cada frame en que se ve.
#[derive(Default)]
pub struct NoticeCounter {
    /// Aviso que está en pantalla y ya se contó.
    shown: Option<ChestType>,
    /// Tipo leído que aún no llega a `CONFIRM_FRAMES` lecturas seguidas.
    candidate: Option<(ChestType, u32)>,
    missing: u32,
}

impl NoticeCounter {
    /// Devuelve el tipo cuando hay que contar una apertura.
    pub fn step(&mut self, kind: Option<ChestType>) -> Option<ChestType> {
        if kind.is_some() && kind == self.shown {
            self.candidate = None;
            self.missing = 0;
            return None;
        }
        let Some(kind) = kind else {
            self.candidate = None;
            self.missing += 1;
            if self.missing >= CLEAR_FRAMES {
                self.shown = None;
            }
            return None;
        };
        let seen = match self.candidate {
            Some((candidate, seen)) if candidate == kind => seen + 1,
            _ => 1,
        };
        if seen < CONFIRM_FRAMES {
            self.candidate = Some((kind, seen));
            return None;
        }
        self.shown = Some(kind);
        self.candidate = None;
        self.missing = 0;
        Some(kind)
    }
}

fn grab_health_bar() -> Result<Frame, String> {
    let (_, height) = capture::screen_size()?;
    let (x, y, w, h) = scene::bar_rect(height);
    capture::grab_rect(x, y, w, h)
}

fn grab_notice_zone() -> Result<Frame, String> {
    let (width, height) = capture::screen_size()?;
    let (x, y, w, h) = notice::roi_rect(width, height);
    capture::grab_rect(x, y, w, h)
}

pub fn spawn(app: AppHandle, diag: Arc<Diag>) {
    std::thread::spawn(move || {
        // Recortes del aviso de cada detección y de avisos desconocidos: sirven
        // para comprobar aciertos y para crear plantillas de tipos nuevos.
        let samples = diag.dir().join("avisos");
        let _ = std::fs::create_dir_all(&samples);
        let save = |roi: &Frame, name: String| {
            let _ = image::save_buffer(
                samples.join(name),
                &roi.rgb,
                roi.width,
                roi.height,
                image::ExtendedColorType::Rgb8,
            );
        };

        let mut counter = NoticeCounter::default();
        let mut matches = MatchWatcher::default();
        let mut fuses = FuseCounter::default();
        let mut overlay_shown = false;
        let mut hud_visible = false;
        let mut turn = 0u32;
        let mut failing = false;
        let mut previous = Reading::Nothing;
        let mut unknown_samples = 0;
        let mut unknown_saved_at = 0;

        loop {
            let tick = Instant::now();
            let roi = match grab_notice_zone() {
                Ok(roi) => roi,
                Err(error) => {
                    if !failing {
                        diag.detection_error(Some(error));
                        failing = true;
                    }
                    std::thread::sleep(RETRY);
                    continue;
                }
            };
            if failing {
                diag.detection_error(None);
                failing = false;
            }

            // Inicio y fin de partida, por la barra de vida.
            turn = turn.wrapping_add(1);
            if turn.is_multiple_of(SCENE_EVERY)
                && let Ok(zone) = grab_health_bar()
            {
                let visible = scene::health_bar_visible(&zone);
                hud_visible = visible;
                diag.set_hud_visible(visible);
                let change = match matches.step(now_ms(), visible) {
                    Some(MatchEvent::Started) => {
                        diag.log("partida: empieza (se ve el HUD)");
                        Some(crate::update(&app, |tracker, now| {
                            tracker.match_started(now)
                        }))
                    }
                    Some(MatchEvent::Ended { at }) => {
                        diag.log("partida: termina (el HUD dejó de verse)");
                        Some(crate::update(&app, |tracker, _| {
                            tracker.match_ended(at, EndReason::Lobby)
                        }))
                    }
                    None => None,
                };
                if let Some(Err(error)) = change {
                    diag.log(&format!("no se pudo guardar el cambio de partida: {error}"));
                }
            }

            // El contador se muestra solo con el juego delante (o con el HUD de
            // partida a la vista, por si el título de la ventana no se reconoce).
            if turn.is_multiple_of(5) {
                let wanted = diag.show_overlay() && (capture::game_in_foreground() || hud_visible);
                if wanted != overlay_shown {
                    crate::set_overlay_visible(&app, wanted);
                    overlay_shown = wanted;
                }
            }

            // Cañón dorado: no da aviso de experiencia; cuenta como cofre de tesoro.
            let fuse_zone = capture::screen_size().and_then(|(width, height)| {
                let (x, y, w, h) = fuse::roi_rect(width, height);
                capture::grab_rect(x, y, w, h)
            });
            if let Ok(zone) = fuse_zone
                && fuses.step(fuse::read(&zone))
            {
                let pressed = diag.pressed_within(INPUT_WINDOW_MS);
                let counted = crate::update(&app, |tracker, now| {
                    tracker.chest_opened(now, ChestType::Normal, 0.4, S1_PROMPT | S4_PROGRESS)
                });
                match counted {
                    Ok(_) => diag.detected("normal", pressed, "cañón dorado"),
                    Err(error) => diag.log(&format!("no se pudo guardar el cañón: {error}")),
                }
            }

            let (reading, closest) = notice::read_scored(&roi);
            let closest = match closest {
                Some((kind, score)) => format!("más parecida: {} {score:.2}", kind.as_str()),
                None => "sin plantilla de ese tamaño".to_string(),
            };
            let now = now_ms();
            let counted_kind = counter
                .step(reading.kind())
                .filter(|kind| *kind != ChestType::Ammo || diag.count_ammo());
            if let Some(kind) = counted_kind {
                let pressed = diag.pressed_within(INPUT_WINDOW_MS);
                let name = kind.as_str();
                if diag.require_input() && !pressed {
                    diag.log(&format!("descartado: aviso de {name} sin pulsación previa"));
                    save(&roi, format!("{now}-{name}-sin-boton.png"));
                } else {
                    // Intervienen S2 y S5, y S3 si se vio la pulsación.
                    let (signals, confidence) = if pressed {
                        (S2_TYPE | S3_INPUT | S5_NOTICE, 0.6)
                    } else {
                        (S2_TYPE | S5_NOTICE, 0.4)
                    };
                    let counted = crate::update(&app, |tracker, now| {
                        tracker.chest_opened(now, kind, confidence, signals)
                    });
                    match counted {
                        Ok(_) => diag.detected(name, pressed, &closest),
                        Err(error) => diag.log(&format!("no se pudo guardar el cofre: {error}")),
                    }
                    save(&roi, format!("{now}-{name}.png"));
                }
            } else if reading == Reading::Unknown
                && previous != Reading::Unknown
                && unknown_samples < MAX_UNKNOWN_SAMPLES
                && now - unknown_saved_at >= UNKNOWN_SAMPLE_GAP_MS
            {
                save(&roi, format!("{now}-desconocido.png"));
                diag.log(&format!("aviso desconocido ({closest})"));
                unknown_samples += 1;
                unknown_saved_at = now;
            }
            previous = reading;

            std::thread::sleep(INTERVAL.saturating_sub(tick.elapsed()));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ChestType::{Ammo, Normal, Rare, Spirit};

    /// Pasa una secuencia de lecturas y devuelve lo que se contó.
    fn count(readings: &[(Option<ChestType>, usize)]) -> Vec<ChestType> {
        let mut counter = NoticeCounter::default();
        readings
            .iter()
            .flat_map(|&(kind, frames)| std::iter::repeat_n(kind, frames))
            .filter_map(|kind| counter.step(kind))
            .collect()
    }

    #[test]
    fn un_aviso_visible_muchos_frames_cuenta_una_sola_vez() {
        assert_eq!(
            count(&[(None, 30), (Some(Normal), 18), (None, 30)]),
            [Normal]
        );
    }

    #[test]
    fn una_lectura_suelta_no_cuenta() {
        assert_eq!(count(&[(None, 10), (Some(Normal), 1), (None, 10)]), []);
    }

    #[test]
    fn un_parpadeo_del_aviso_no_cuenta_dos_veces() {
        // El aviso se pierde un par de frames (compresión, animación) y vuelve.
        let readings = [
            (Some(Rare), 6),
            (None, 2),
            (Some(Rare), 6),
            (None, 3),
            (Some(Rare), 4),
            (None, 20),
        ];
        assert_eq!(count(&readings), [Rare]);
    }

    #[test]
    fn dos_avisos_separados_cuentan_dos() {
        let readings = [(Some(Normal), 17), (None, 8), (Some(Normal), 17), (None, 8)];
        assert_eq!(count(&readings), [Normal, Normal]);
    }

    #[test]
    fn un_aviso_que_cambia_de_tipo_sin_desaparecer_cuenta_los_dos() {
        // Caja de munición y, enseguida, un cofre raro.
        assert_eq!(
            count(&[(Some(Ammo), 10), (Some(Rare), 15), (None, 10)]),
            [Ammo, Rare]
        );
    }

    /// Secuencias reales: recortes de la zona del aviso a 10 fps de los
    /// vídeos completos (locales; ver tools/extraer_avisos.sh).
    #[test]
    fn videos_completos() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../Debug/avisos-10fps");
        let cases = [
            ("cofres-prueba", vec![Normal, Normal, Normal, Normal]),
            ("raro-municion", vec![Ammo, Rare]),
            ("espiritu", vec![Spirit]),
        ];
        for (name, expected) in cases {
            let Ok(entries) = std::fs::read_dir(root.join(name)) else {
                eprintln!("sin secuencia {name}: prueba omitida");
                continue;
            };
            let mut files: Vec<_> = entries.map(|e| e.unwrap().path()).collect();
            files.sort();
            assert!(files.len() > 100, "{name}: solo {} recortes", files.len());

            let mut counter = NoticeCounter::default();
            let mut counted = Vec::new();
            for (i, path) in files.iter().enumerate() {
                let image = image::open(path).unwrap().into_rgb8();
                let roi = Frame {
                    width: image.width(),
                    height: image.height(),
                    rgb: image.into_raw(),
                };
                if let Some(kind) = counter.step(notice::read(&roi).kind()) {
                    eprintln!("{name}: {kind:?} en t = {:.1} s", i as f32 / 10.0);
                    counted.push(kind);
                }
            }
            assert_eq!(counted, expected, "{name}");
        }
    }

    /// Ráfaga real grabada por la app en el equipo de pruebas: 99 capturas a
    /// 10 fps en las que se abre un cofre de tesoro.
    #[test]
    fn rafaga_real_del_equipo_de_pruebas() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../Debug/reales/rafaga");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            eprintln!("sin ráfaga real: prueba omitida");
            return;
        };
        let mut files: Vec<_> = entries.map(|e| e.unwrap().path()).collect();
        files.sort();
        let mut counter = NoticeCounter::default();
        let (mut counted, mut frames_read) = (Vec::new(), 0);
        for path in &files {
            let image = image::open(path).unwrap().into_rgb8();
            let roi = Frame {
                width: image.width(),
                height: image.height(),
                rgb: image.into_raw(),
            };
            let kind = notice::read(&roi).kind();
            frames_read += usize::from(kind == Some(Normal));
            counted.extend(counter.step(kind));
        }
        assert_eq!(counted, [Normal]);
        // El aviso dura unos 20 frames; se lee bien en casi todos, no solo en dos.
        assert!(frames_read >= 16, "solo se leyó en {frames_read} frames");
    }
}
