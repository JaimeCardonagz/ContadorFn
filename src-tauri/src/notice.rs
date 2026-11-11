//! Lector del aviso de experiencia (ARQUITECTURA.md §5).
//!
//! Al abrir un contenedor aparece, en una zona fija del HUD, una cantidad
//! ("+600 PE") y debajo una o dos líneas; la última dice qué se abrió:
//! "COFRE", "COFRE RARO", "CAJA DE MUNICIÓN". Este módulo localiza esas líneas
//! y compara la última con plantillas, así que confirma la apertura y da el
//! tipo a la vez.
//!
//! El texto es lila muy claro con contorno violeta. Se toma como texto todo
//! píxel claro que tenga contorno violeta a 2 px o menos; eso lo separa bien
//! del fondo, sea cual sea.
//!
//! Todo se mide en la resolución de referencia 1366×768 (la del equipo de
//! pruebas). Los recortes de otro tamaño se reescalan a ella.

use crate::capture::Frame;
use crate::store::ChestType;
use image::imageops::{self, FilterType};

/// Zona del aviso, en fracciones de la pantalla: x0, y0, x1, y1.
pub const ROI: (f32, f32, f32, f32) = (0.62, 0.27, 0.80, 0.41);
/// Tamaño de esa zona a 1366×768.
const CANON_W: usize = 246;
const CANON_H: usize = 107;

/// Una fila cuenta como texto con al menos estos píxeles.
const MIN_ROW: usize = 3;
/// Filas vacías que se toleran dentro de una misma línea de texto.
const MAX_GAP: usize = 2;
/// Altura mínima de la línea de la cantidad ("+600 PE").
const AMOUNT_MIN_HEIGHT: usize = 13;
/// Coincidencia mínima (intersección / unión) con una plantilla.
const MIN_IOU: f32 = 0.6;

// Plantillas a 1366×768, español de Latinoamérica.
const COFRE: &[&str] = &[
    "..#####...#####...#####.#####...#####",
    "..##.###.###.##..###.#..##..##.###...",
    ".##...##.##..###.##.....#...##.##....",
    ".##.....##....##.##.....#...##.##....",
    ".#......##...##..####..##..##..####..",
    "##......##...##..####..#####...##....",
    "#....##.##...##..##....##.##...##....",
    ".##.##..##..##...#.....##..#...##....",
    ".#####...#####...#.....#...##..####..",
];
const COFRE_RARO: &[&str] = &[
    "....##......##........#...###...............##.............##........#..",
    "..#####...######..#####..#####...####......#####....###...######...####.",
    ".##..##...##..##..##.....##..##.###........#...##..####...##..##..##..##",
    ".##..##..##...##..##....##...##.##........##...##..####...#...##.##...##",
    "##.......##...##.###....##..###.##........##..##...#..#..##...#####...##",
    "##.......##...##..####..######..####......######..##..#..######..##...##",
    "##......##....##.#####..#####...##........#####...##.##..#####...##...##",
    "##...##.###..##..##.....##.##..##.........##.##..######..##.##...##..##.",
    "###.##...##..##..##....##..##..###........#..##..##..##..##..##..##..##.",
    ".####....#####...#......#...#..#####......#...##.#....#..#...##..#####..",
];
const CAJA_DE_MUNICION: &[&str] = &[
    ".....................#...##.................###............###.......#.......#........###.........##.........#",
    ".#####....#.........##...##........#####...#####.......#...###..##..##..##..##..##...#####..##..######..##...#",
    ".##.###...#.##......##..###........#..##...##..........#..####..##..##..##..##..##..##..##..##..##..##..#.#..#",
    "##...##..#####......##..##........##..###.##...........##.####..#...##..##..##..##.##...##..##.##...##..###..#",
    "##.......##.##......##..#.........##..##..###..........##.####.##...##...##.##..##.##.......##.##...##..###.##",
    "#........#..##......#.............#...##..#####.....##.###.###.##...##.######...##.##.......##.##...##.#####..",
    "#....#..##..##.##..##.............#...##..##........##.###.##..##...##.##.###...#..##...#...#.###...##.##.##..",
    "#...##..######.##..##.######.....##...##..##........##.###.##..##..##..#..###..##.###..##..##.###..###.##.###.",
    "##.###.###..##.######.##..##.....##..##...###.......##.....##..######.##..###..##..##..##..##..##..##..##..##.",
    "...##..##...##.#####..#...##.....##..#....#####.....#......##...####...#...##..##..##..#...##..#####...#...##.",
];
const TEMPLATES: &[(ChestType, &[&str])] = &[
    (ChestType::Normal, COFRE),
    (ChestType::Rare, COFRE_RARO),
    (ChestType::Ammo, CAJA_DE_MUNICION),
];

/// Lo que se lee en la zona del aviso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// No hay aviso.
    Nothing,
    /// Hay un aviso de experiencia, pero su última línea no es ninguna de las
    /// etiquetas conocidas ("NUEVA FASE DE LA TORMENTA", un tipo sin plantilla…).
    Unknown,
    Known(ChestType),
}

impl Reading {
    pub fn kind(self) -> Option<ChestType> {
        match self {
            Reading::Known(kind) => Some(kind),
            _ => None,
        }
    }
}

/// Rectángulo de la zona del aviso en una pantalla de `width`×`height`:
/// (x, y, ancho, alto) en píxeles.
pub fn roi_rect(width: u32, height: u32) -> (u32, u32, u32, u32) {
    let x0 = (ROI.0 * width as f32) as u32;
    let y0 = (ROI.1 * height as f32) as u32;
    let x1 = (ROI.2 * width as f32) as u32;
    let y1 = (ROI.3 * height as f32) as u32;
    (x0, y0, x1 - x0, y1 - y0)
}

/// Lee el aviso en una captura de la pantalla completa.
#[cfg(test)]
pub fn read_screen(screen: &Frame) -> Reading {
    let (x, y, w, h) = roi_rect(screen.width, screen.height);
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for row in y..y + h {
        let start = ((row * screen.width + x) * 3) as usize;
        match screen.rgb.get(start..start + (w * 3) as usize) {
            Some(pixels) => rgb.extend_from_slice(pixels),
            None => return Reading::Nothing,
        }
    }
    read(&Frame {
        width: w,
        height: h,
        rgb,
    })
}

/// Lee el aviso en un recorte que cubre exactamente la zona `ROI`.
pub fn read(roi: &Frame) -> Reading {
    let Some(rgb) = canonical(roi) else {
        return Reading::Nothing;
    };
    let mask = Mask::from_text(&rgb);
    let lines = mask.lines();
    let Some((label, above)) = lines.split_last() else {
        return Reading::Nothing;
    };
    // Encima de la etiqueta tiene que estar la cantidad, que es más alta.
    if !above.iter().any(|l| l.y1 - l.y0 >= AMOUNT_MIN_HEIGHT) {
        return Reading::Nothing;
    }
    let (width, height) = (label.x1 - label.x0, label.y1 - label.y0);
    let mut best = (None, MIN_IOU);
    for (kind, rows) in TEMPLATES {
        let template = Mask::from_ascii(rows);
        if width.abs_diff(template.w) > 4 || height.abs_diff(template.h) > 2 {
            continue;
        }
        for dy in -1..=1 {
            for dx in -2..=2 {
                let score = mask.iou(label, &template, dx, dy);
                if score >= best.1 {
                    best = (Some(*kind), score);
                }
            }
        }
    }
    best.0.map_or(Reading::Unknown, Reading::Known)
}

/// El recorte como RGB de `CANON_W`×`CANON_H`.
fn canonical(roi: &Frame) -> Option<Vec<u8>> {
    if roi.width as usize == CANON_W && roi.height as usize == CANON_H {
        return Some(roi.rgb.clone());
    }
    let image = image::RgbImage::from_raw(roi.width, roi.height, roi.rgb.clone())?;
    let resized = imageops::resize(&image, CANON_W as u32, CANON_H as u32, FilterType::Triangle);
    Some(resized.into_raw())
}

fn is_fill([r, g, b]: [u8; 3]) -> bool {
    r >= 205 && b >= 220 && g >= 165 && r >= g.saturating_add(6) && b >= g.saturating_add(6)
}

fn is_outline([r, g, b]: [u8; 3]) -> bool {
    b >= 95
        && b >= r.saturating_add(22)
        && r >= g.saturating_add(8)
        && (r as u32 + g as u32 + b as u32) < 480
}

/// Línea de texto: filas [y0, y1) y columnas [x0, x1).
#[derive(Debug, Clone, Copy)]
struct Line {
    y0: usize,
    y1: usize,
    x0: usize,
    x1: usize,
}

struct Mask {
    w: usize,
    h: usize,
    bits: Vec<bool>,
}

impl Mask {
    fn get(&self, x: isize, y: isize) -> bool {
        x >= 0
            && y >= 0
            && (x as usize) < self.w
            && (y as usize) < self.h
            && self.bits[y as usize * self.w + x as usize]
    }

    fn from_ascii(rows: &[&str]) -> Self {
        Self {
            w: rows[0].len(),
            h: rows.len(),
            bits: rows
                .iter()
                .flat_map(|row| row.bytes().map(|c| c == b'#'))
                .collect(),
        }
    }

    /// Píxeles de texto: relleno claro con contorno violeta a 2 px o menos.
    fn from_text(rgb: &[u8]) -> Self {
        let pixel = |i: usize| [rgb[i * 3], rgb[i * 3 + 1], rgb[i * 3 + 2]];
        let outline = Self {
            w: CANON_W,
            h: CANON_H,
            bits: (0..CANON_W * CANON_H)
                .map(|i| is_outline(pixel(i)))
                .collect(),
        };
        let mut bits = vec![false; CANON_W * CANON_H];
        for y in 0..CANON_H as isize {
            for x in 0..CANON_W as isize {
                let i = y as usize * CANON_W + x as usize;
                bits[i] = is_fill(pixel(i))
                    && (-2..=2).any(|dy| (-2..=2).any(|dx| outline.get(x + dx, y + dy)));
            }
        }
        Self {
            w: CANON_W,
            h: CANON_H,
            bits,
        }
    }

    fn lines(&self) -> Vec<Line> {
        let row_count = |y: usize| {
            self.bits[y * self.w..(y + 1) * self.w]
                .iter()
                .filter(|&&b| b)
                .count()
        };
        let mut lines = Vec::new();
        let mut start = None;
        let mut blank = 0;
        for y in 0..=self.h {
            if y < self.h && row_count(y) >= MIN_ROW {
                start.get_or_insert(y);
                blank = 0;
            } else if let Some(y0) = start {
                blank += 1;
                if blank > MAX_GAP || y == self.h {
                    let y1 = y + 1 - blank;
                    let used = |x: usize| (y0..y1).any(|y| self.bits[y * self.w + x]);
                    if let (Some(x0), Some(last)) = (
                        (0..self.w).find(|&x| used(x)),
                        (0..self.w).rfind(|&x| used(x)),
                    ) {
                        lines.push(Line {
                            y0,
                            y1,
                            x0,
                            x1: last + 1,
                        });
                    }
                    start = None;
                    blank = 0;
                }
            }
        }
        lines
    }

    /// Intersección / unión entre la línea `line` y la plantilla desplazada.
    fn iou(&self, line: &Line, template: &Mask, dx: isize, dy: isize) -> f32 {
        let (width, height) = ((line.x1 - line.x0) as isize, (line.y1 - line.y0) as isize);
        let (mut both, mut either) = (0u32, 0u32);
        for y in -1..height.max(template.h as isize + dy) + 1 {
            for x in -2..width.max(template.w as isize + dx) + 2 {
                let in_line = (0..width).contains(&x) && (0..height).contains(&y);
                let a = in_line && self.get(line.x0 as isize + x, line.y0 as isize + y);
                let b = template.get(x - dx, y - dy);
                both += u32::from(a && b);
                either += u32::from(a || b);
            }
        }
        if either == 0 {
            0.0
        } else {
            both as f32 / either as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const FILL: [u8; 3] = [255, 236, 255];
    const OUTLINE: [u8; 3] = [88, 62, 145];

    /// Zona del aviso de un solo color.
    fn blank(background: [u8; 3]) -> Frame {
        Frame {
            width: CANON_W as u32,
            height: CANON_H as u32,
            rgb: background.repeat(CANON_W * CANON_H),
        }
    }

    /// Dibuja texto con el estilo del juego: relleno claro y contorno violeta.
    fn draw(roi: &mut Frame, rows: &[&str], left: usize, top: usize) {
        let mask = Mask::from_ascii(rows);
        let mut set = |x: isize, y: isize, color: [u8; 3]| {
            if (0..CANON_W as isize).contains(&x) && (0..CANON_H as isize).contains(&y) {
                let i = (y as usize * CANON_W + x as usize) * 3;
                roi.rgb[i..i + 3].copy_from_slice(&color);
            }
        };
        let (left, top) = (left as isize, top as isize);
        for y in -1..=mask.h as isize {
            for x in -1..=mask.w as isize {
                if mask.get(x, y) {
                    set(left + x, top + y, FILL);
                } else if (-1..=1).any(|dy| (-1..=1).any(|dx| mask.get(x + dx, y + dy))) {
                    set(left + x, top + y, OUTLINE);
                }
            }
        }
    }

    /// Algo con la altura de la cantidad ("+600 PE").
    const AMOUNT: &[&str] = &["##..####..####..####..###..####"; 16];

    fn notice(background: [u8; 3], label: &[&str], left: usize) -> Frame {
        let mut roi = blank(background);
        draw(&mut roi, AMOUNT, 45, 17);
        draw(&mut roi, label, left, 40);
        roi
    }

    #[test]
    fn reconoce_cada_etiqueta_sobre_fondos_distintos() {
        let backgrounds = [[20, 30, 60], [130, 35, 55], [215, 140, 60], [250, 250, 250]];
        for background in backgrounds {
            for (kind, rows) in TEMPLATES {
                for left in [40, 75, 109] {
                    assert_eq!(
                        read(&notice(background, rows, left)),
                        Reading::Known(*kind),
                        "{kind:?} en x={left} sobre {background:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn sin_aviso_no_lee_nada() {
        for background in [[0, 0, 0], [255, 255, 255], [255, 236, 255], [88, 62, 145]] {
            assert_eq!(read(&blank(background)), Reading::Nothing, "{background:?}");
        }
    }

    #[test]
    fn una_etiqueta_sin_la_cantidad_encima_no_vale() {
        let mut roi = blank([20, 30, 60]);
        draw(&mut roi, COFRE, 109, 40);
        assert_eq!(read(&roi), Reading::Nothing);
    }

    #[test]
    fn un_aviso_de_otra_cosa_se_lee_como_desconocido() {
        // Mismo estilo y altura, pero otro texto: otra anchura u otra forma.
        let other_width: &[&str] = &["###.###.###.###.###.###.###.###.###.###.###.###.###.###"; 9];
        let same_width: &[&str] = &["#####################################"; 9];
        for label in [other_width, same_width] {
            assert_eq!(read(&notice([20, 30, 60], label, 60)), Reading::Unknown);
        }
    }

    #[test]
    fn con_una_linea_intermedia_lee_la_ultima() {
        // "+750 PE / NIVEL DE ESPÍRITU / COFRE"
        let mut roi = blank([20, 30, 60]);
        draw(&mut roi, AMOUNT, 39, 6);
        draw(&mut roi, &["####.####.##.####.####.####.###"; 10], 54, 29);
        draw(&mut roi, COFRE, 54, 51);
        assert_eq!(read(&roi), Reading::Known(ChestType::Normal));
    }

    #[test]
    fn lee_igual_un_recorte_a_otra_resolucion() {
        let roi = notice([20, 30, 60], COFRE_RARO, 74);
        let image = image::RgbImage::from_raw(roi.width, roi.height, roi.rgb).unwrap();
        // El mismo recorte en una pantalla de 1920×1080.
        let (_, _, w, h) = roi_rect(1920, 1080);
        let big = imageops::resize(&image, w, h, FilterType::CatmullRom);
        let roi = Frame {
            width: w,
            height: h,
            rgb: big.into_raw(),
        };
        assert_eq!(read(&roi), Reading::Known(ChestType::Rare));
    }

    /// Fotogramas reales del juego a 1366×768 (locales, no están en el
    /// repositorio). El nombre dice qué debe leerse: `normal_`, `rare_`,
    /// `ammo_`, `none_`, o `trans_` para un aviso a medio aparecer o
    /// desaparecer, donde vale no leer nada pero no leer otro tipo.
    #[test]
    fn fotogramas_reales() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Debug/frames");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            eprintln!("sin fotogramas reales en {}: prueba omitida", dir.display());
            return;
        };
        let mut checked = 0;
        for entry in entries {
            let path = entry.unwrap().path();
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let image = image::open(&path).unwrap().into_rgb8();
            let screen = Frame {
                width: image.width(),
                height: image.height(),
                rgb: image.into_raw(),
            };
            let read = read_screen(&screen).kind();
            let expected = match name.split('_').next().unwrap() {
                "normal" => Some(ChestType::Normal),
                "rare" => Some(ChestType::Rare),
                "ammo" => Some(ChestType::Ammo),
                "none" => None,
                "trans" => {
                    assert!(
                        matches!(read, None | Some(ChestType::Normal)),
                        "{name}: leyó {read:?}"
                    );
                    continue;
                }
                other => panic!("prefijo desconocido: {other}"),
            };
            assert_eq!(read, expected, "{name}");
            checked += 1;
        }
        assert!(checked >= 30, "solo se comprobaron {checked} fotogramas");
    }
}
