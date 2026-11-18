//! Detección de partida: ¿está en pantalla el HUD de juego?
//!
//! La señal es la barra de vida (y su icono "+"), abajo a la izquierda. Medido en los vídeos
//! del usuario: aparece en la isla inicial, se oculta unos segundos durante el
//! autobús, vuelve al saltar y desaparece al ser eliminado o volver al lobby.
//! En la pantalla de eliminación hay otra barra verde (la del rival), pero en
//! otro sitio.

use crate::capture::Frame;
use image::imageops::{self, FilterType};

/// Zona del HUD que se mira, a 768 px de alto: x0, y0, x1, y1. Contiene el
/// icono "+" de la vida y el arranque de la barra. El HUD está anclado al
/// borde izquierdo y escala con la altura de la pantalla.
const ZONE: (u32, u32, u32, u32) = (22, 698, 102, 724);
const ZONE_W: usize = (ZONE.2 - ZONE.0) as usize;
const ZONE_H: usize = (ZONE.3 - ZONE.1) as usize;
/// Dentro de la zona: trozo de barra que está lleno con algo más del 15 % de vida.
const BAR: (usize, usize, usize, usize) = (32, 4, 78, 22);
/// Fracción mínima de píxeles verdes para dar la barra por visible.
const MIN_GREEN: f32 = 0.6;
/// Dentro de la zona: dónde puede estar el icono "+" (varía unos píxeles
/// según el ancho de la pantalla).
const ICON_SEARCH: (usize, usize, usize, usize) = (0, 0, 30, ZONE_H);
/// El icono "+": blanco, 10×11 px, con brazos de 4 y 3 px de grosor.
const PLUS: [&str; 11] = [
    "...####...",
    "...####...",
    "...####...",
    "...####...",
    "##########",
    "##########",
    "##########",
    "...####...",
    "...####...",
    "...####...",
    "...####...",
];

/// Lecturas seguidas con el HUD visible para dar por empezada una partida.
const START_SAMPLES: u32 = 3;
/// Sin HUD durante este tiempo la partida ha terminado (salió al lobby sin
/// pasar por la pantalla de puesto final). Es largo a propósito: el HUD
/// también se oculta en el autobús, al abrir el mapa, mientras estás
/// derribado y en la pantalla de eliminación antes de pasar a ver a un
/// compañero. El final normal llega antes, con la pantalla "QUEDASTE".
const GRACE_MS: i64 = 150_000;

/// Rectángulo de la zona del HUD en una pantalla de `height` píxeles de alto:
/// (x, y, ancho, alto).
pub fn zone_rect(height: u32) -> (u32, u32, u32, u32) {
    let scale = |v: u32| v * height / 768;
    let (x0, y0, x1, y1) = (scale(ZONE.0), scale(ZONE.1), scale(ZONE.2), scale(ZONE.3));
    (x0, y0, (x1 - x0).max(1), (y1 - y0).max(1))
}

/// ¿Se ve el HUD de partida en `zone` (un recorte de `zone_rect`)? Vale la
/// barra de vida o su icono "+": con muy poca vida la barra casi no se ve,
/// pero el icono sigue ahí mientras se está en partida.
pub fn hud_visible(zone: &Frame) -> bool {
    let Some(rgb) = canonical(zone) else {
        return false;
    };
    let pixel = |x: usize, y: usize| {
        let i = (y * ZONE_W + x) * 3;
        [rgb[i] as i16, rgb[i + 1] as i16, rgb[i + 2] as i16]
    };
    let mut green = 0;
    for y in BAR.1..BAR.3 {
        for x in BAR.0..BAR.2 {
            let [r, g, b] = pixel(x, y);
            // En i16: con u8 saturado, el blanco puro pasaría por verde.
            green += usize::from(g >= 150 && g - r >= 40 && g - b >= 70);
        }
    }
    let bar_area = (BAR.2 - BAR.0) * (BAR.3 - BAR.1);
    if green as f32 / bar_area as f32 >= MIN_GREEN {
        return true;
    }
    let white = |x: usize, y: usize| pixel(x, y).iter().all(|&c| c >= 190);
    let (width, height) = (PLUS[0].len(), PLUS.len());
    for top in ICON_SEARCH.1..=ICON_SEARCH.3 - height {
        for left in ICON_SEARCH.0..=ICON_SEARCH.2 - width {
            let (mut on, mut on_hit, mut off, mut off_hit) = (0, 0, 0, 0);
            for (y, row) in PLUS.iter().enumerate() {
                for (x, cell) in row.bytes().enumerate() {
                    let lit = white(left + x, top + y);
                    if cell == b'#' {
                        on += 1;
                        on_hit += usize::from(lit);
                    } else {
                        off += 1;
                        off_hit += usize::from(lit);
                    }
                }
            }
            // Casi todo el "+" encendido y casi nada de sus esquinas.
            if on_hit * 100 >= on * 85 && off_hit * 100 <= off * 20 {
                return true;
            }
        }
    }
    false
}

/// El recorte como RGB de `ZONE_W`×`ZONE_H` (reescalado si la pantalla no
/// mide 768 px de alto).
fn canonical(zone: &Frame) -> Option<Vec<u8>> {
    let (w, h) = (zone.width as usize, zone.height as usize);
    if zone.rgb.len() != w * h * 3 || w == 0 || h == 0 {
        return None;
    }
    if (w, h) == (ZONE_W, ZONE_H) {
        return Some(zone.rgb.clone());
    }
    let image = image::RgbImage::from_raw(zone.width, zone.height, zone.rgb.clone())?;
    Some(imageops::resize(&image, ZONE_W as u32, ZONE_H as u32, FilterType::Triangle).into_raw())
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
    /// Tras terminar la partida a mano: no empieza otra hasta que el HUD
    /// deje de verse al menos una vez (puede que aún se esté mirando a un
    /// compañero).
    wait_absent: bool,
    started_at: i64,
    visible_streak: u32,
    absent_since: Option<i64>,
}

impl MatchWatcher {
    /// ¿Hay una partida en curso (según el HUD)?
    pub fn in_match(&self) -> bool {
        self.in_match
    }

    pub fn step(&mut self, now: i64, bar_visible: bool) -> Option<MatchEvent> {
        if self.wait_absent {
            if bar_visible {
                return None;
            }
            self.wait_absent = false;
        }
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
        if now - since < GRACE_MS {
            return None;
        }
        self.in_match = false;
        self.absent_since = None;
        Some(MatchEvent::Ended { at: since })
    }

    /// La partida se terminó a mano: la siguiente que se vea empieza aparte.
    pub fn end_now(&mut self) {
        self.in_match = false;
        self.absent_since = None;
        self.visible_streak = 0;
        self.wait_absent = true;
    }

    /// Se vio la pantalla de puesto final ("QUEDASTE"): la partida terminó,
    /// en el momento en que dejó de verse el HUD.
    pub fn placement_seen(&mut self, now: i64) -> Option<MatchEvent> {
        if !self.in_match {
            return None;
        }
        let at = self.absent_since.unwrap_or(now);
        self.in_match = false;
        self.absent_since = None;
        self.visible_streak = 0;
        Some(MatchEvent::Ended { at })
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
        let events = run(&mut w, 0, &[(true, 600), (false, 200)]);
        assert_eq!(
            events,
            [
                (2, MatchEvent::Started),
                // Se decide a los 150 s, pero el final es cuando dejó de verse.
                (750, MatchEvent::Ended { at: 600 * SEC }),
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
                (190, MatchEvent::Ended { at: 40 * SEC })
            ]
        );
    }

    #[test]
    fn derribado_o_viendo_a_un_companero_no_termina_la_partida() {
        let mut w = MatchWatcher::default();
        // Derribado y pantalla de eliminación 2 min sin HUD; luego se ve el
        // HUD del compañero al que miras y, tras reaparecer, el tuyo.
        let events = run(&mut w, 0, &[(true, 600), (false, 120), (true, 300)]);
        assert_eq!(events, [(2, MatchEvent::Started)]);
    }

    #[test]
    fn terminar_a_mano_espera_a_que_el_hud_desaparezca_antes_de_otra() {
        let mut w = MatchWatcher::default();
        run(&mut w, 0, &[(true, 300)]);
        w.end_now();
        // Aún se ve un HUD (mirando a un compañero): no empieza otra.
        let events = run(&mut w, 300, &[(true, 60), (false, 30), (true, 10)]);
        assert_eq!(events, [(392, MatchEvent::Started)]);
    }

    #[test]
    fn la_pantalla_de_puesto_final_termina_la_partida_al_momento() {
        let mut w = MatchWatcher::default();
        run(&mut w, 0, &[(true, 600), (false, 8)]);
        assert_eq!(
            w.placement_seen(608 * SEC),
            Some(MatchEvent::Ended { at: 600 * SEC })
        );
        assert_eq!(w.placement_seen(609 * SEC), None, "solo una vez");
        // Volver enseguida a otra partida cuenta como una nueva.
        let events = run(&mut w, 640, &[(false, 20), (true, 10)]);
        assert_eq!(events, [(662, MatchEvent::Started)]);
    }

    #[test]
    fn dos_partidas_seguidas_se_cuentan_por_separado() {
        let mut w = MatchWatcher::default();
        let events = run(
            &mut w,
            0,
            &[(true, 500), (false, 200), (true, 500), (false, 160)],
        );
        let kinds: Vec<_> = events.iter().map(|(_, e)| *e).collect();
        assert_eq!(
            kinds,
            [
                MatchEvent::Started,
                MatchEvent::Ended { at: 500 * SEC },
                MatchEvent::Started,
                MatchEvent::Ended { at: 1_200 * SEC },
            ]
        );
    }

    /// Zona del HUD de un solo color.
    fn zone(color: [u8; 3]) -> Frame {
        Frame {
            width: ZONE_W as u32,
            height: ZONE_H as u32,
            rgb: color.repeat(ZONE_W * ZONE_H),
        }
    }

    fn paint(frame: &mut Frame, x0: usize, y0: usize, x1: usize, y1: usize, color: [u8; 3]) {
        for y in y0..y1 {
            for x in x0..x1 {
                let i = (y * frame.width as usize + x) * 3;
                frame.rgb[i..i + 3].copy_from_slice(&color);
            }
        }
    }

    /// HUD sobre un fondo: el icono "+" y la barra llena hasta `filled`.
    fn hud(background: [u8; 3], filled: usize) -> Frame {
        let mut frame = zone(background);
        paint(&mut frame, 28, 4, 78, 22, [40, 40, 50]); // pista vacía de la barra
        paint(&mut frame, 28, 4, 28 + filled, 22, [70, 205, 60]);
        for (y, row) in PLUS.iter().enumerate() {
            for (x, cell) in row.bytes().enumerate() {
                if cell == b'#' {
                    paint(&mut frame, 13 + x, 8 + y, 14 + x, 9 + y, [235, 235, 235]);
                }
            }
        }
        frame
    }

    #[test]
    fn con_vida_el_hud_se_ve() {
        assert!(hud_visible(&hud([60, 90, 140], 50)));
        assert!(
            hud_visible(&zone([160, 230, 40])),
            "barra verde amarillenta"
        );
    }

    #[test]
    fn con_muy_poca_vida_el_hud_se_sigue_viendo_por_el_icono() {
        // La barra está casi vacía: lo que queda es el "+".
        for filled in [0, 2, 5] {
            assert!(hud_visible(&hud([60, 90, 140], filled)), "vida {filled}");
        }
    }

    #[test]
    fn sin_hud_no_se_ve_nada() {
        for color in [
            [30, 30, 40],
            [255, 255, 255],
            [40, 160, 230],
            [200, 60, 50],
            [235, 235, 235],
        ] {
            assert!(!hud_visible(&zone(color)), "{color:?}");
        }
    }

    #[test]
    fn la_zona_escala_con_la_altura() {
        assert_eq!(zone_rect(768), (22, 698, 80, 26));
        assert_eq!(zone_rect(1080), (30, 981, 113, 37));
        // Una pantalla más grande se lee igual tras reescalar.
        let small = hud([60, 90, 140], 3);
        let image = image::RgbImage::from_raw(small.width, small.height, small.rgb).unwrap();
        let (_, _, w, h) = zone_rect(1440);
        let big = imageops::resize(&image, w, h, FilterType::CatmullRom);
        assert!(hud_visible(&Frame {
            width: w,
            height: h,
            rgb: big.into_raw(),
        }));
    }

    /// Herramienta: ¿se ve el HUD en cada imagen de una ráfaga (RAFAGA_DIR)?
    #[test]
    #[ignore]
    fn revisar_rafaga() {
        let dir = std::env::var("RAFAGA_DIR").expect("RAFAGA_DIR");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "jpg" || e == "png"))
            .collect();
        files.sort();
        for path in files {
            let image = image::open(&path).unwrap().into_rgb8();
            let (x, y, w, h) = zone_rect(image.height());
            let crop = image::imageops::crop_imm(&image, x, y, w, h).to_image();
            let frame = Frame {
                width: crop.width(),
                height: crop.height(),
                rgb: crop.into_raw(),
            };
            println!(
                "{} {}",
                path.file_name().unwrap().to_string_lossy(),
                hud_visible(&frame)
            );
        }
    }

    /// Recortes reales de la zona del HUD (locales): `si_*` en partida, `no_*`
    /// en el lobby, en la pantalla de eliminación y tras la partida.
    #[test]
    fn recortes_reales() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../Debug/reales/hud");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            eprintln!("sin recortes del HUD: prueba omitida");
            return;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let image = image::open(&path).unwrap().into_rgb8();
            let visible = hud_visible(&Frame {
                width: image.width(),
                height: image.height(),
                rgb: image.into_raw(),
            });
            assert_eq!(visible, name.starts_with("si_"), "{name}");
        }
    }

    /// Recortes de la barra a 1 fps de los vídeos del usuario (locales; ver
    /// tools/extraer_avisos.sh). "Cofres Prueba" va del lobby a la eliminación.
    #[test]
    fn videos_completos() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../Debug/hud-1fps");
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
                hud_visible(&Frame {
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
        for second in visible.len()..visible.len() + 200 {
            end = end.or(w.step(second as i64 * SEC, false));
        }
        assert!(
            matches!(end, Some(MatchEvent::Ended { at }) if (127..=129).contains(&(at / SEC))),
            "{end:?}"
        );
    }
}
