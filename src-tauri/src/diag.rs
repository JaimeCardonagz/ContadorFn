//! Diagnóstico: comprueba en el equipo real qué puede ver y oír la app
//! (captura de pantalla, tecla E, atajos F8/F9) y guarda capturas tal como las
//! recibe, para usarlas después como casos de prueba del detector.
//!
//! - F8: una captura sin pérdida (PNG).
//! - F9: empieza/detiene una ráfaga de frames (JPEG) con un CSV que anota en
//!   cada frame si la tecla E estaba pulsada.

use crate::capture::{self, Frame};
use crate::input::{self, Key};
use crate::now_ms;
use base64::Engine;
use image::ExtendedColorType;
use image::codecs::jpeg::JpegEncoder;
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const TICK: Duration = Duration::from_millis(33);
const BURST_INTERVAL: Duration = Duration::from_millis(66); // ~15 fps
const BURST_MAX: Duration = Duration::from_secs(10);
/// El mismo atajo puede llegar por sondeo y como atajo global a la vez, y una
/// tecla mantenida se repite: se ignoran peticiones tan seguidas.
const DEBOUNCE_MS: i64 = 600;
/// Por debajo de este brillo medio (0-255) la captura se considera negra.
const BLACK_LUMA: f32 = 3.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagState {
    /// La captura y el teclado solo están implementados en Windows.
    pub supported: bool,
    pub dir: String,
    pub e_down: bool,
    pub e_presses: u32,
    /// F8/F9 vistas consultando el teclado.
    pub poll_hits: u32,
    /// F8/F9 recibidas como atajo global del sistema.
    pub hotkey_hits: u32,
    pub hotkey_error: Option<String>,
    /// Segundos que faltan para una captura o ráfaga pedida con retardo.
    pub pending_in: Option<u32>,
    pub recording: bool,
    pub frames: u32,
    pub last_result: Option<Outcome>,
    /// Contenedores contados por la detección desde que se abrió la app.
    pub detected: u32,
    /// Tipo del último contado ("normal", "rare", "spirit", "ammo").
    pub last_detected: Option<String>,
    /// Por qué la detección no está funcionando, si es el caso.
    pub detect_error: Option<String>,
}

pub struct Diag {
    dir: PathBuf,
    state: Mutex<DiagState>,
    // Instante (epoch ms) en que toca hacer cada cosa; 0 = nada pendiente.
    shot_at: AtomicI64,
    burst_at: AtomicI64,
    last_shot_request: AtomicI64,
    last_burst_request: AtomicI64,
}

impl Diag {
    pub fn new(dir: PathBuf) -> Arc<Self> {
        let state = DiagState {
            supported: cfg!(windows),
            dir: dir.display().to_string(),
            ..Default::default()
        };
        Arc::new(Self {
            dir,
            state: Mutex::new(state),
            shot_at: AtomicI64::new(0),
            burst_at: AtomicI64::new(0),
            last_shot_request: AtomicI64::new(0),
            last_burst_request: AtomicI64::new(0),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn snapshot(&self) -> DiagState {
        self.lock().clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, DiagState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn request_shot(&self, delay_ms: i64) {
        request(&self.last_shot_request, &self.shot_at, delay_ms);
    }

    /// Empieza una ráfaga tras `delay_ms`, o detiene ya la que esté en curso.
    pub fn request_burst(&self, delay_ms: i64) {
        let delay_ms = if self.lock().recording { 0 } else { delay_ms };
        request(&self.last_burst_request, &self.burst_at, delay_ms);
    }

    /// Llegó F8 (`burst = false`) o F9 (`burst = true`) como atajo global.
    pub fn hotkey(&self, burst: bool) {
        self.lock().hotkey_hits += 1;
        if burst {
            self.request_burst(0);
        } else {
            self.request_shot(0);
        }
    }

    pub fn set_hotkey_error(&self, error: String) {
        self.log(&format!("atajo global no disponible: {error}"));
        self.lock().hotkey_error = Some(error);
    }

    /// Añade una línea a `diagnostico.log`, junto a las capturas.
    pub fn log(&self, message: &str) {
        let secs = now_ms() / 1000;
        let (h, m, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("diagnostico.log"))
        {
            // Una sola escritura por línea: la usan varios hilos a la vez.
            let line = format!("{h:02}:{m:02}:{s:02} UTC  {message}\n");
            let _ = file.write_all(line.as_bytes());
        }
    }

    /// La detección contó un contenedor de tipo `kind`.
    pub fn detected(&self, kind: &str) {
        self.log(&format!("detectado: {kind}"));
        let mut state = self.lock();
        state.detected += 1;
        state.last_detected = Some(kind.to_string());
    }

    /// Estado de la detección: `Some(motivo)` si no puede capturar.
    pub fn detection_error(&self, error: Option<String>) {
        if let Some(error) = &error {
            self.log(&format!("detección detenida: {error}"));
        }
        self.lock().detect_error = error;
    }

    fn finish(&self, outcome: Outcome) {
        self.log(&outcome.message);
        self.lock().last_result = Some(outcome);
    }
}

fn request(last: &AtomicI64, at: &AtomicI64, delay_ms: i64) {
    let now = now_ms();
    if now - last.swap(now, SeqCst) >= DEBOUNCE_MS {
        at.store(now + delay_ms, SeqCst);
    }
}

fn take_due(at: &AtomicI64, now: i64) -> bool {
    let due = at.load(SeqCst);
    due != 0 && due <= now && at.compare_exchange(due, 0, SeqCst, SeqCst).is_ok()
}

/// Hilo de diagnóstico: consulta el teclado y atiende capturas y ráfagas.
pub fn spawn(app: AppHandle, diag: Arc<Diag>) {
    std::thread::spawn(move || {
        diag.log(&format!(
            "inicio v{} (captura disponible: {})",
            env!("CARGO_PKG_VERSION"),
            cfg!(windows)
        ));
        let mut previous = [false; 3];
        let mut burst: Option<Burst> = None;
        let mut emitted = DiagState::default();

        loop {
            let tick = Instant::now();
            let keys = [Key::E, Key::F8, Key::F9].map(input::key_down);
            {
                let mut state = diag.lock();
                state.e_down = keys[0];
                state.e_presses += u32::from(keys[0] && !previous[0]);
                state.poll_hits += u32::from(keys[1] && !previous[1]);
                state.poll_hits += u32::from(keys[2] && !previous[2]);
            }
            if keys[1] && !previous[1] {
                diag.request_shot(0);
            }
            if keys[2] && !previous[2] {
                diag.request_burst(0);
            }
            previous = keys;

            let now = now_ms();
            if take_due(&diag.shot_at, now) {
                let (outcome, thumbnail) = shoot(diag.dir());
                diag.finish(outcome);
                if let Some(thumbnail) = thumbnail {
                    let _ = app.emit("diag-thumbnail", thumbnail);
                }
            }
            if take_due(&diag.burst_at, now) {
                match burst.take() {
                    Some(running) => diag.finish(running.summary()),
                    None => match Burst::start(diag.dir()) {
                        Ok(started) => burst = Some(started),
                        Err(e) => diag.finish(Outcome {
                            ok: false,
                            message: format!("No se pudo crear la carpeta de la ráfaga: {e}"),
                        }),
                    },
                }
            }
            if let Some(running) = &mut burst {
                let ms = running.started.elapsed().as_millis();
                let step =
                    capture::grab_screen().and_then(|frame| running.add(&frame, ms, keys[0]));
                let failed = step.err();
                if (failed.is_some() || running.started.elapsed() >= BURST_MAX)
                    && let Some(finished) = burst.take()
                {
                    let mut outcome = finished.summary();
                    if let Some(error) = failed {
                        outcome = Outcome {
                            ok: false,
                            message: format!("Ráfaga interrumpida: {error} ({})", outcome.message),
                        };
                    }
                    diag.finish(outcome);
                }
            }

            let snapshot = {
                let mut state = diag.lock();
                state.recording = burst.is_some();
                if let Some(running) = &burst {
                    state.frames = running.frames;
                }
                let next = [diag.shot_at.load(SeqCst), diag.burst_at.load(SeqCst)]
                    .into_iter()
                    .filter(|&at| at > now)
                    .min();
                state.pending_in = next.map(|at| ((at - now) as u32).div_ceil(1000));
                state.clone()
            };
            if snapshot != emitted {
                let _ = app.emit("diag-state", &snapshot);
                emitted = snapshot;
            }

            let interval = if burst.is_some() {
                BURST_INTERVAL
            } else {
                TICK
            };
            std::thread::sleep(interval.saturating_sub(tick.elapsed()));
        }
    });
}

/// Captura la pantalla, la guarda en PNG y devuelve el resultado con una
/// miniatura (data URL) para enseñarla en la ventana.
fn shoot(dir: &Path) -> (Outcome, Option<String>) {
    let started = Instant::now();
    let frame = match capture::grab_screen() {
        Ok(frame) => frame,
        Err(message) => return (Outcome { ok: false, message }, None),
    };
    let ms = started.elapsed().as_millis();
    let luma = mean_luma(&frame);
    let name = format!("captura-{}.png", now_ms());
    let saved = save_png(&frame, &dir.join(&name));
    let thumbnail = thumbnail_data_url(&frame);

    let size = format!("{}x{} en {ms} ms", frame.width, frame.height);
    let outcome = match saved {
        Err(e) => Outcome {
            ok: false,
            message: format!("Captura {size}, pero no se pudo guardar: {e}"),
        },
        Ok(()) if luma < BLACK_LUMA => Outcome {
            ok: false,
            message: format!("Captura NEGRA ({size}, brillo {luma:.1}): {name}"),
        },
        Ok(()) => Outcome {
            ok: true,
            message: format!("Captura correcta ({size}, brillo {luma:.0}): {name}"),
        },
    };
    (outcome, thumbnail)
}

/// Ráfaga en curso: una carpeta con los frames numerados y `frames.csv`.
struct Burst {
    dir: PathBuf,
    started: Instant,
    csv: BufWriter<File>,
    frames: u32,
}

impl Burst {
    fn start(root: &Path) -> std::io::Result<Self> {
        let dir = root.join(format!("rafaga-{}", now_ms()));
        fs::create_dir_all(&dir)?;
        let mut csv = BufWriter::new(File::create(dir.join("frames.csv"))?);
        writeln!(csv, "frame,ms,e_pulsada")?;
        Ok(Self {
            dir,
            started: Instant::now(),
            csv,
            frames: 0,
        })
    }

    /// Guarda un frame capturado `ms` milisegundos después de empezar.
    fn add(&mut self, frame: &Frame, ms: u128, e_down: bool) -> Result<(), String> {
        let number = self.frames + 1;
        save_jpeg(frame, &self.dir.join(format!("{number:04}.jpg")), 92)
            .map_err(|e| e.to_string())?;
        writeln!(self.csv, "{number},{ms},{}", u8::from(e_down)).map_err(|e| e.to_string())?;
        self.frames = number;
        Ok(())
    }

    fn summary(mut self) -> Outcome {
        let _ = self.csv.flush();
        let secs = self.started.elapsed().as_secs_f32();
        let folder = self.dir.file_name().unwrap_or_default().to_string_lossy();
        Outcome {
            ok: self.frames > 0,
            message: format!(
                "Ráfaga de {} frames en {secs:.1} s ({:.0} fps): {folder}",
                self.frames,
                self.frames as f32 / secs.max(0.001),
            ),
        }
    }
}

fn save_png(frame: &Frame, path: &Path) -> image::ImageResult<()> {
    image::save_buffer(
        path,
        &frame.rgb,
        frame.width,
        frame.height,
        ExtendedColorType::Rgb8,
    )
}

fn save_jpeg(frame: &Frame, path: &Path, quality: u8) -> image::ImageResult<()> {
    let file = BufWriter::new(File::create(path)?);
    JpegEncoder::new_with_quality(file, quality).encode(
        &frame.rgb,
        frame.width,
        frame.height,
        ExtendedColorType::Rgb8,
    )
}

/// Brillo medio (0-255), muestreando uno de cada 16 píxeles.
fn mean_luma(frame: &Frame) -> f32 {
    let mut sum = 0u64;
    let mut count = 0u64;
    for p in frame.rgb.as_chunks::<3>().0.iter().step_by(16) {
        sum += (299 * p[0] as u64 + 587 * p[1] as u64 + 114 * p[2] as u64) / 1000;
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        sum as f32 / count as f32
    }
}

fn thumbnail_data_url(frame: &Frame) -> Option<String> {
    let image = image::RgbImage::from_raw(frame.width, frame.height, frame.rgb.clone())?;
    let width = 320.min(frame.width);
    let height = (frame.height * width / frame.width).max(1);
    let small = image::imageops::thumbnail(&image, width, height);
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 75)
        .encode(small.as_raw(), width, height, ExtendedColorType::Rgb8)
        .ok()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(jpeg);
    Some(format!("data:image/jpeg;base64,{encoded}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: u32, height: u32, color: impl Fn(u32, u32) -> [u8; 3]) -> Frame {
        let mut rgb = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height {
            for x in 0..width {
                rgb.extend(color(x, y));
            }
        }
        Frame { width, height, rgb }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fcc-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn distingue_una_captura_negra_de_una_con_imagen() {
        assert!(mean_luma(&frame(1366, 768, |_, _| [0, 0, 0])) < BLACK_LUMA);
        assert!(mean_luma(&frame(1366, 768, |x, _| [(x % 256) as u8, 90, 40])) > BLACK_LUMA);
    }

    #[test]
    fn el_png_guardado_conserva_los_pixeles_exactos() {
        let dir = temp_dir("png");
        let original = frame(1366, 768, |x, y| [(x % 256) as u8, (y % 256) as u8, 7]);
        save_png(&original, &dir.join("a.png")).unwrap();

        let read = image::open(dir.join("a.png")).unwrap().into_rgb8();
        assert_eq!((read.width(), read.height()), (1366, 768));
        assert_eq!(read.as_raw(), &original.rgb);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn la_miniatura_es_un_jpeg_pequeno_con_la_misma_proporcion() {
        let url = thumbnail_data_url(&frame(1366, 768, |x, _| [(x % 256) as u8, 0, 0])).unwrap();
        let data = url.strip_prefix("data:image/jpeg;base64,").unwrap();
        let jpeg = base64::engine::general_purpose::STANDARD
            .decode(data)
            .unwrap();
        let small = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((small.width(), small.height()), (320, 179));
    }

    #[test]
    fn la_rafaga_guarda_frames_numerados_y_el_csv_con_la_tecla() {
        let root = temp_dir("rafaga");
        let mut burst = Burst::start(&root).unwrap();
        let image = frame(320, 180, |x, y| [(x % 256) as u8, (y % 256) as u8, 0]);
        burst.add(&image, 0, false).unwrap();
        burst.add(&image, 66, true).unwrap();
        burst.add(&image, 133, true).unwrap();
        let dir = burst.dir.clone();
        let outcome = burst.summary();

        assert!(outcome.ok);
        assert!(outcome.message.contains("3 frames"), "{}", outcome.message);
        assert_eq!(
            fs::read_to_string(dir.join("frames.csv")).unwrap(),
            "frame,ms,e_pulsada\n1,0,0\n2,66,1\n3,133,1\n"
        );
        for name in ["0001.jpg", "0002.jpg", "0003.jpg"] {
            let read = image::open(dir.join(name)).unwrap();
            assert_eq!((read.width(), read.height()), (320, 180));
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn peticiones_muy_seguidas_cuentan_como_una() {
        let (last, at) = (AtomicI64::new(0), AtomicI64::new(0));
        request(&last, &at, 0);
        assert!(take_due(&at, now_ms()), "la primera petición se atiende");
        request(&last, &at, 0); // misma pulsación llegando por la otra vía
        assert!(!take_due(&at, now_ms()));
    }

    #[test]
    fn una_peticion_con_retardo_no_se_atiende_antes_de_tiempo() {
        let (last, at) = (AtomicI64::new(0), AtomicI64::new(0));
        request(&last, &at, 5_000);
        let now = now_ms();
        assert!(!take_due(&at, now));
        assert!(take_due(&at, now + 5_001));
        assert!(!take_due(&at, now + 5_002), "solo se atiende una vez");
    }
}
