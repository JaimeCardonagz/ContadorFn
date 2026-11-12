//! Lector del aviso de experiencia (ARQUITECTURA.md §5).
//!
//! Al abrir un contenedor aparece, en una zona fija del HUD, una cantidad
//! ("+600 PE") y debajo una o varias líneas; la última dice qué se abrió:
//! "COFRE", "COFRE RARO", "COFRE DE ESPÍRITUS", "CAJA DE MUNICIÓN". Este
//! módulo localiza esas líneas y compara la última con plantillas, así que
//! confirma la apertura y da el tipo a la vez.
//!
//! El texto es lila muy claro con contorno violeta. Se toma como texto todo
//! píxel claro que tenga contorno violeta a 2 px o menos; eso lo separa bien
//! del fondo, sea cual sea.
//!
//! La comparación es una correlación entre máscaras difuminadas, no una
//! coincidencia píxel a píxel: el aviso entra deslizándose y el vídeo de
//! GeForce Now va comprimido, así que el mismo texto no se dibuja idéntico en
//! dos frames. Medido con capturas reales (tools/eval_ncc.py): el texto
//! correcto da 0,92 o más y otro texto de la misma anchura, 0,81 como mucho.
//!
//! Todo se mide a 768 px de alto (el equipo de pruebas). Los recortes de otra
//! altura se reescalan.

use crate::capture::Frame;
use crate::notice_templates::TEMPLATES;
use crate::store::ChestType;
use image::imageops::{self, FilterType};
use std::sync::OnceLock;

/// Zona del aviso, en fracciones de la pantalla: x0, y0, x1, y1. Deja sitio
/// para avisos de hasta cinco líneas, que crecen hacia arriba y hacia abajo.
pub const ROI: (f32, f32, f32, f32) = (0.60, 0.235, 0.80, 0.41);
/// Tamaño de esa zona a 1366×768.
const CANON_W: usize = 273;
const CANON_H: usize = 134;

/// Una fila cuenta como texto con al menos estos píxeles.
const MIN_ROW: usize = 3;
/// Filas vacías que se toleran dentro de una misma línea de texto.
const MAX_GAP: usize = 2;
/// Altura mínima de la línea de la cantidad ("+600 PE").
const AMOUNT_MIN_HEIGHT: usize = 13;
/// Correlación mínima con una plantilla.
const MIN_SCORE: f32 = 0.86;
/// Desplazamientos que se prueban al comparar con una plantilla.
const MAX_DX: isize = 3;
const MAX_DY: isize = 1;
/// Margen alrededor de la etiqueta en el mapa difuminado.
const PAD: usize = 4;

/// Lo que se lee en la zona del aviso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// No hay aviso.
    Nothing,
    /// Hay un aviso de experiencia, pero su última línea no es ninguna de las
    /// etiquetas conocidas ("NUEVA FASE DE LA TORMENTA", "ELIMINACIÓN"…).
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
#[cfg(test)]
pub fn read(roi: &Frame) -> Reading {
    read_scored(roi).0
}

/// Como `read`, y además la plantilla que más se parece a la última línea y
/// su correlación, llegue o no al mínimo. Sirve para el registro.
pub fn read_scored(roi: &Frame) -> (Reading, Option<(ChestType, f32)>) {
    let nothing = (Reading::Nothing, None);
    let Some(rgb) = canonical(roi) else {
        return nothing;
    };
    let mask = Mask::from_text(&rgb);
    let lines = mask.lines();
    let Some((label, above)) = lines.split_last() else {
        return nothing;
    };
    // Encima de la etiqueta tiene que estar la cantidad, que es más alta.
    if !above.iter().any(|l| l.y1 - l.y0 >= AMOUNT_MIN_HEIGHT) {
        return nothing;
    }
    let label = Soft::from_mask(&mask.crop(label));
    let mut best: Option<(ChestType, f32)> = None;
    for (kind, template) in templates() {
        if label.w.abs_diff(template.w) > 4 || label.h.abs_diff(template.h) > 2 {
            continue;
        }
        let score = label.best_correlation(template);
        if best.is_none_or(|(_, top)| score > top) {
            best = Some((*kind, score));
        }
    }
    let reading = match best {
        Some((kind, score)) if score >= MIN_SCORE => Reading::Known(kind),
        _ => Reading::Unknown,
    };
    (reading, best)
}

fn templates() -> &'static [(ChestType, Soft)] {
    static SOFT: OnceLock<Vec<(ChestType, Soft)>> = OnceLock::new();
    SOFT.get_or_init(|| {
        TEMPLATES
            .iter()
            .map(|(kind, rows)| (*kind, Soft::from_mask(&Mask::from_ascii(rows))))
            .collect()
    })
}

/// El recorte como RGB de `CANON_W`×`CANON_H`.
fn canonical(roi: &Frame) -> Option<Vec<u8>> {
    let (w, h) = (roi.width as usize, roi.height as usize);
    if roi.rgb.len() != w * h * 3 {
        return None;
    }
    // Misma altura (p. ej. 1360×768 en vez de 1366×768): se usan los píxeles
    // tal cual, sin reescalar, recortando o rellenando alguna columna.
    if h == CANON_H && w.abs_diff(CANON_W) <= 2 {
        let mut rgb = vec![0; CANON_W * CANON_H * 3];
        let columns = w.min(CANON_W) * 3;
        for y in 0..h {
            rgb[y * CANON_W * 3..][..columns].copy_from_slice(&roi.rgb[y * w * 3..][..columns]);
        }
        return Some(rgb);
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

    /// La parte de la máscara que ocupa una línea.
    fn crop(&self, line: &Line) -> Mask {
        let (w, h) = (line.x1 - line.x0, line.y1 - line.y0);
        Mask {
            w,
            h,
            bits: (0..h)
                .flat_map(|y| {
                    let start = (line.y0 + y) * self.w + line.x0;
                    self.bits[start..start + w].iter().copied()
                })
                .collect(),
        }
    }
}

/// Máscara difuminada (media 3×3) con un margen de `PAD` alrededor. `w` y `h`
/// son los de la máscara original.
struct Soft {
    w: usize,
    h: usize,
    values: Vec<f32>,
}

impl Soft {
    fn from_mask(mask: &Mask) -> Self {
        let (width, height) = (mask.w + 2 * PAD, mask.h + 2 * PAD);
        let mut values = vec![0.0; width * height];
        for y in 0..mask.h {
            for x in 0..mask.w {
                if mask.bits[y * mask.w + x] {
                    for yy in y + PAD - 1..=y + PAD + 1 {
                        for xx in x + PAD - 1..=x + PAD + 1 {
                            values[yy * width + xx] += 1.0 / 9.0;
                        }
                    }
                }
            }
        }
        Self {
            w: mask.w,
            h: mask.h,
            values,
        }
    }

    fn get(&self, x: isize, y: isize) -> f32 {
        let (width, height) = ((self.w + 2 * PAD) as isize, (self.h + 2 * PAD) as isize);
        if x < 0 || y < 0 || x >= width || y >= height {
            0.0
        } else {
            self.values[(y * width + x) as usize]
        }
    }

    /// Mejor correlación con `other` probando pequeños desplazamientos.
    fn best_correlation(&self, other: &Soft) -> f32 {
        let mut best = f32::MIN;
        for dy in -MAX_DY..=MAX_DY {
            for dx in -MAX_DX..=MAX_DX {
                best = best.max(self.correlation(other, dx, dy));
            }
        }
        best
    }

    /// Correlación de Pearson entre los dos mapas, con `other` desplazado.
    fn correlation(&self, other: &Soft, dx: isize, dy: isize) -> f32 {
        let width = (self.w.max(other.w) + 2 * PAD) as isize;
        let height = (self.h.max(other.h) + 2 * PAD) as isize;
        let n = (width * height) as f32;
        let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for y in 0..height {
            for x in 0..width {
                let a = self.get(x, y);
                let b = other.get(x - dx, y - dy);
                sa += a;
                sb += b;
                saa += a * a;
                sbb += b * b;
                sab += a * b;
            }
        }
        let variance = (saa - sa * sa / n).max(1e-9) * (sbb - sb * sb / n).max(1e-9);
        (sab - sa * sb / n) / variance.sqrt()
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

    /// Una plantilla del tipo pedido.
    fn template(kind: ChestType) -> &'static [&'static str] {
        TEMPLATES.iter().find(|(k, _)| *k == kind).unwrap().1
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
        draw(&mut roi, template(ChestType::Normal), 109, 40);
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
        draw(&mut roi, template(ChestType::Normal), 54, 51);
        assert_eq!(read(&roi), Reading::Known(ChestType::Normal));
    }

    #[test]
    fn lee_igual_un_recorte_a_otra_resolucion() {
        let roi = notice([20, 30, 60], template(ChestType::Rare), 74);
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

    /// Comprueba todas las imágenes de una carpeta. El nombre dice qué debe
    /// leerse: `normal_`, `rare_`, `spirit_`, `ammo_`, `none_`; o
    /// `trans-<tipo>_` para un aviso a medio aparecer o desaparecer, donde
    /// vale no leer nada pero no leer otro tipo. Devuelve cuántas comprobó.
    fn check_images(dir: &Path, read: fn(&Frame) -> Reading) -> usize {
        let Ok(entries) = std::fs::read_dir(dir) else {
            eprintln!("sin imágenes en {}: prueba omitida", dir.display());
            return usize::MAX;
        };
        let mut checked = 0;
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                continue;
            }
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let image = image::open(&path).unwrap().into_rgb8();
            let frame = Frame {
                width: image.width(),
                height: image.height(),
                rgb: image.into_raw(),
            };
            let got = read(&frame).kind();
            let prefix = name.split('_').next().unwrap();
            let (transition, kind) = match prefix.strip_prefix("trans-") {
                Some(kind) => (true, kind),
                None => (false, prefix),
            };
            let expected = match kind {
                "normal" => Some(ChestType::Normal),
                "rare" => Some(ChestType::Rare),
                "spirit" => Some(ChestType::Spirit),
                "ammo" => Some(ChestType::Ammo),
                "none" => None,
                other => panic!("prefijo desconocido: {other}"),
            };
            if transition {
                assert!(got.is_none() || got == expected, "{name}: leyó {got:?}");
            } else {
                assert_eq!(got, expected, "{name}");
            }
            checked += 1;
        }
        checked
    }

    /// Fotogramas completos a 1366×768 sacados de los vídeos de OBS (locales,
    /// no están en el repositorio).
    #[test]
    fn fotogramas_de_los_videos() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Debug/frames");
        assert!(check_images(&dir, read_screen) >= 40);
    }

    /// Recortes guardados por la app en el equipo de pruebas (GeForce Now,
    /// 1360×768), incluido el cofre que la primera versión dejó sin contar.
    #[test]
    fn recortes_reales_del_equipo_de_pruebas() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Debug/reales");
        assert!(check_images(&dir, read) >= 23);
    }
}
