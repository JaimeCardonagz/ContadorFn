//! Cañón dorado: suelta botín como un cofre, pero no muestra el aviso de
//! experiencia. Se reconoce por su recuadro "ENCENDER MECHA": si mientras se
//! ve avanza el anillo de progreso (aparecen píxeles blancos alrededor del
//! icono de la tecla) y después el recuadro desaparece, se encendió. Cuenta
//! como cofre de tesoro (decisión del usuario, 2026-11-12).
//!
//! El recuadro no está en un sitio fijo: sigue al cañón en pantalla. Por eso
//! se busca en una zona grande, con una tabla de sumas para descartar rápido
//! casi todas las posiciones.

use crate::capture::Frame;
use crate::notice::{Mask, Sums};
use crate::notice_templates::FUSE_PROMPT;
use image::imageops::{self, FilterType};

/// Zona donde puede aparecer el recuadro, en fracciones de la pantalla.
pub const ROI: (f32, f32, f32, f32) = (0.25, 0.20, 0.85, 0.80);
/// Alto de esa zona a 768 px de alto de pantalla.
const CANON_H: u32 = 461;
/// Coincidencia mínima (intersección / unión) del texto con la plantilla.
const MIN_IOU: f32 = 0.6;
/// Píxeles blancos que añade el anillo, como mínimo, para darlo por avanzado
/// (completo añade unos 320).
pub const RING_DELTA: u32 = 120;
/// Lecturas seguidas sin el recuadro para darlo por desaparecido.
const GONE_FRAMES: u32 = 2;

pub fn roi_rect(width: u32, height: u32) -> (u32, u32, u32, u32) {
    let x0 = (ROI.0 * width as f32) as u32;
    let y0 = (ROI.1 * height as f32) as u32;
    let x1 = (ROI.2 * width as f32) as u32;
    let y1 = (ROI.3 * height as f32) as u32;
    (x0, y0, x1 - x0, y1 - y0)
}

/// Si el recuadro "ENCENDER MECHA" está en `roi`, devuelve cuántos píxeles
/// blancos hay alrededor del icono de la tecla (crece al avanzar el anillo).
pub fn read(roi: &Frame) -> Option<u32> {
    let mask = white_text(roi)?;
    let sums = Sums::new(&mask);
    let template = Mask::from_ascii(FUSE_PROMPT);
    let (tw, th) = (template.w as isize, template.h as isize);
    let wanted = template.bits.iter().filter(|&&b| b).count() as u32;
    for y0 in 0..=mask.h as isize - th {
        for x0 in 0..=mask.w as isize - tw {
            // Descarte rápido: cantidad de texto parecida a la de la plantilla
            // y sin más texto pegado justo encima ni debajo.
            let inside = sums.count(x0, y0, x0 + tw, y0 + th);
            if inside * 10 < wanted * 7 || inside * 10 > wanted * 14 {
                continue;
            }
            if sums.count(x0, y0 - 3, x0 + tw, y0 - 1) * 10 > wanted
                || sums.count(x0, y0 + th + 1, x0 + tw, y0 + th + 3) * 10 > wanted
            {
                continue;
            }
            if iou(&mask, &template, x0, y0) >= MIN_IOU {
                return Some(sums.count(x0 - 75, y0 - 16, x0 - 5, y0 + th + 16));
            }
        }
    }
    None
}

fn iou(mask: &Mask, template: &Mask, x0: isize, y0: isize) -> f32 {
    let (mut both, mut either) = (0u32, 0u32);
    for y in 0..template.h as isize {
        for x in 0..template.w as isize {
            let (a, b) = (mask.get(x0 + x, y0 + y), template.get(x, y));
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

/// Texto blanco sobre recuadro oscuro: píxeles blancos con varios oscuros cerca.
fn white_text(roi: &Frame) -> Option<Mask> {
    let (mut w, mut h) = (roi.width as usize, roi.height as usize);
    if roi.rgb.len() != w * h * 3 || h == 0 {
        return None;
    }
    let mut rgb = std::borrow::Cow::Borrowed(&roi.rgb[..]);
    if roi.height != CANON_H {
        // Otra resolución: se lleva al tamaño que tendría a 768 px de alto.
        let width = (roi.width * CANON_H / roi.height).max(1);
        let image = image::RgbImage::from_raw(roi.width, roi.height, roi.rgb.clone())?;
        rgb = imageops::resize(&image, width, CANON_H, FilterType::Triangle)
            .into_raw()
            .into();
        (w, h) = (width as usize, CANON_H as usize);
    }
    let pixel = |i: usize| {
        [
            rgb[i * 3] as i16,
            rgb[i * 3 + 1] as i16,
            rgb[i * 3 + 2] as i16,
        ]
    };
    let dark: Vec<bool> = (0..w * h)
        .map(|i| {
            let [r, g, b] = pixel(i);
            (299 * r as i32 + 587 * g as i32 + 114 * b as i32) / 1000 < 120
        })
        .collect();
    let mut bits = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let [r, g, b] = pixel(y * w + x);
            let (low, high) = (r.min(g).min(b), r.max(g).max(b));
            if low < 200 || high - low > 40 {
                continue;
            }
            let mut near = 0;
            for yy in y.saturating_sub(3)..(y + 4).min(h) {
                for xx in x.saturating_sub(3)..(x + 4).min(w) {
                    near += u32::from(dark[yy * w + xx]);
                }
            }
            bits[y * w + x] = near >= 8;
        }
    }
    Some(Mask { w, h, bits })
}

/// Sigue el recuadro del cañón a lo largo de lecturas sucesivas.
#[derive(Default)]
pub struct FuseCounter {
    visible: bool,
    min: u32,
    max: u32,
    gone: u32,
}

impl FuseCounter {
    /// ¿Está el recuadro a la vista ahora mismo?
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Cuando el recuadro desaparece devuelve cuánto creció el anillo mientras
    /// estuvo a la vista; a partir de `RING_DELTA` el cañón se encendió.
    pub fn step(&mut self, reading: Option<u32>) -> Option<u32> {
        match reading {
            Some(ring) => {
                if !self.visible {
                    (self.visible, self.min, self.max) = (true, ring, ring);
                }
                self.min = self.min.min(ring);
                self.max = self.max.max(ring);
                self.gone = 0;
                None
            }
            None if !self.visible => None,
            None => {
                self.gone += 1;
                if self.gone < GONE_FRAMES {
                    return None;
                }
                self.visible = false;
                Some(self.max - self.min)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(readings: &[(Option<u32>, usize)]) -> usize {
        let mut counter = FuseCounter::default();
        readings
            .iter()
            .flat_map(|&(r, n)| std::iter::repeat_n(r, n))
            .filter(|&r| counter.step(r).is_some_and(|grown| grown >= RING_DELTA))
            .count()
    }

    #[test]
    fn anillo_que_avanza_y_recuadro_que_desaparece_cuenta_uno() {
        assert_eq!(
            count(&[(Some(100), 10), (Some(250), 3), (Some(420), 3), (None, 20)]),
            1
        );
    }

    #[test]
    fn mirar_el_canon_sin_encenderlo_no_cuenta() {
        assert_eq!(
            count(&[(Some(100), 30), (None, 20), (Some(104), 30), (None, 20)]),
            0
        );
    }

    #[test]
    fn un_parpadeo_del_recuadro_no_parte_la_lectura() {
        let readings = [
            (Some(100), 5),
            (None, 1),
            (Some(300), 3),
            (None, 1),
            (Some(420), 2),
            (None, 9),
        ];
        assert_eq!(count(&readings), 1);
    }

    /// Ráfagas reales (locales): la segunda prueba enciende un cañón dorado;
    /// la primera abre un cofre normal y no debe contar ningún cañón.
    #[test]
    fn rafagas_reales() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../Debug/reales");
        for (dir, expected) in [("canon", 1), ("canon-no", 0)] {
            let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
                eprintln!("sin ráfaga {dir}: prueba omitida");
                continue;
            };
            let mut files: Vec<_> = entries.map(|e| e.unwrap().path()).collect();
            files.sort();
            let mut counter = FuseCounter::default();
            let mut counted = 0;
            for path in &files {
                let image = image::open(path).unwrap().into_rgb8();
                let roi = Frame {
                    width: image.width(),
                    height: image.height(),
                    rgb: image.into_raw(),
                };
                counted += usize::from(
                    counter
                        .step(read(&roi))
                        .is_some_and(|grown| grown >= RING_DELTA),
                );
            }
            assert_eq!(counted, expected, "{dir}");
        }
    }
}
