//! Captura de pantalla. Por ahora solo Windows y solo el monitor principal.
//!
//! Se usa GDI (BitBlt) porque es lo más simple que funciona con el escritorio
//! compuesto de Windows 10/11, y permite copiar solo un rectángulo pequeño.
//! Si con GeForce Now saliera en negro, el paso siguiente es DXGI Desktop
//! Duplication (ARQUITECTURA.md §3).

use crate::settings::tr;

/// Una imagen en RGB de 8 bits, fila a fila desde arriba.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// ¿Es del juego la ventana en primer plano? Se mira qué programa es su
/// dueño (GeForce NOW o Fortnite); el título solo vale si nombra GeForce NOW.
/// No basta con que el título contenga "Fortnite": también lo contienen una
/// carpeta, un vídeo o una pestaña del navegador.
#[cfg(windows)]
fn foreground_game_window() -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };
    use windows::core::PWSTR;

    // SAFETY: los búferes son válidos y cada llamada recibe su tamaño; el
    // handle del proceso se cierra antes de salir.
    unsafe {
        let window = GetForegroundWindow();
        let mut title = [0u16; 256];
        let length = GetWindowTextW(window, &mut title).max(0) as usize;
        let title = String::from_utf16_lossy(&title[..length]).to_lowercase();

        let mut program = String::new();
        let mut process_id = 0u32;
        GetWindowThreadProcessId(window, Some(&mut process_id));
        if let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) {
            let mut path = [0u16; 512];
            let mut size = path.len() as u32;
            if QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &mut size,
            )
            .is_ok()
            {
                program = String::from_utf16_lossy(&path[..size as usize]).to_lowercase();
            }
            let _ = CloseHandle(process);
        }
        is_game_window(&program, &title).then_some(window)
    }
}

#[cfg(windows)]
pub fn game_in_foreground() -> bool {
    foreground_game_window().is_some()
}

#[cfg(not(windows))]
pub fn game_in_foreground() -> bool {
    false
}

/// `program`: ruta del ejecutable; `title`: título de la ventana. En minúsculas.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn is_game_window(program: &str, title: &str) -> bool {
    let executable = program.rsplit(['\\', '/']).next().unwrap_or(program);
    if executable.contains("chest") {
        return false; // esta misma app
    }
    executable.contains("geforcenow")
        || executable.contains("fortniteclient")
        || title.contains("geforce now")
}

/// Zona de la pantalla donde se ve el juego, en píxeles reales del escritorio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// `true` si es la ventana del juego; `false` si es el monitor principal
    /// (el juego no está delante o no se pudo localizar su ventana).
    pub window: bool,
}

impl Area {
    /// Pasa un rectángulo relativo a la zona (x, y, ancho, alto) a
    /// coordenadas del escritorio.
    pub fn absolute(&self, (x, y, w, h): (u32, u32, u32, u32)) -> (i32, i32, u32, u32) {
        (self.x + x as i32, self.y + y as i32, w, h)
    }

    /// La parte con imagen: si la ventana es más alta que 16:9, el juego se
    /// ve con franjas negras arriba y abajo y hay que descontarlas.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn content(self) -> Self {
        let (w, h) = (self.width as u64, self.height as u64);
        if w * 9 >= h * 16 {
            return self;
        }
        let visible = (w * 9 / 16) as u32;
        Self {
            y: self.y + ((self.height - visible) / 2) as i32,
            height: visible,
            ..self
        }
    }
}

/// Dónde está el juego ahora: el área de su ventana si está delante y no
/// minimizada (vale en cualquier resolución, en ventana o en otro monitor);
/// si no, el monitor principal.
pub fn game_area() -> Result<Area, String> {
    #[cfg(windows)]
    if let Some(area) = foreground_game_area() {
        return Ok(area.content());
    }
    let (width, height) = screen_size()?;
    Ok(Area {
        x: 0,
        y: 0,
        width,
        height,
        window: false,
    })
}

/// Tamaño mínimo de la ventana del juego para poder leer el aviso.
#[cfg_attr(not(windows), allow(dead_code))]
const MIN_WIDTH: u32 = 320;
#[cfg_attr(not(windows), allow(dead_code))]
const MIN_HEIGHT: u32 = 180;

/// ¿Está delante la ventana del juego pero es demasiado pequeña para leerla?
/// (Minimizada no cuenta: entonces no se está jugando.)
pub fn game_window_too_small() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, IsIconic};

        let Some(window) = foreground_game_window() else {
            return false;
        };
        // SAFETY: `window` es una ventana válida y la estructura es local.
        unsafe {
            let mut rect = RECT::default();
            if IsIconic(window).as_bool() || GetClientRect(window, &mut rect).is_err() {
                return false;
            }
            let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
            width > 0 && height > 0 && (width < MIN_WIDTH as i32 || height < MIN_HEIGHT as i32)
        }
    }
    #[cfg(not(windows))]
    false
}

#[cfg(windows)]
fn foreground_game_area() -> Option<Area> {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, IsIconic};

    let window = foreground_game_window()?;
    // SAFETY: `window` es una ventana válida y las estructuras son locales.
    unsafe {
        if IsIconic(window).as_bool() {
            return None;
        }
        let mut rect = RECT::default();
        GetClientRect(window, &mut rect).ok()?;
        let mut origin = POINT::default();
        if !ClientToScreen(window, &mut origin).as_bool() {
            return None;
        }
        let (width, height) = (
            (rect.right - rect.left) as u32,
            (rect.bottom - rect.top) as u32,
        );
        // Ventanas diminutas (o a medio crear): no sirven para leer nada.
        (width >= MIN_WIDTH && height >= MIN_HEIGHT).then_some(Area {
            x: origin.x,
            y: origin.y,
            width,
            height,
            window: true,
        })
    }
}

/// Captura el monitor principal completo.
pub fn grab_screen() -> Result<Frame, String> {
    let (width, height) = screen_size()?;
    grab_rect(0, 0, width, height)
}

#[cfg(not(windows))]
pub fn screen_size() -> Result<(u32, u32), String> {
    Err(unsupported())
}

#[cfg(not(windows))]
pub fn grab_rect(_x: i32, _y: i32, _width: u32, _height: u32) -> Result<Frame, String> {
    Err(unsupported())
}

#[cfg(not(windows))]
fn unsupported() -> String {
    tr(
        "La captura de pantalla solo está implementada en Windows.",
        "Screen capture is only implemented on Windows.",
    )
    .into()
}

/// Tamaño en píxeles del monitor principal.
#[cfg(windows)]
pub fn screen_size() -> Result<(u32, u32), String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    // SAFETY: GetSystemMetrics no tiene precondiciones.
    let (width, height) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    if width <= 0 || height <= 0 {
        return Err(format!(
            "{}: {width}x{height}",
            tr("Tamaño de pantalla no válido", "Invalid screen size")
        ));
    }
    Ok((width as u32, height as u32))
}

/// Captura un rectángulo del monitor principal.
#[cfg(windows)]
pub fn grab_rect(x: i32, y: i32, width: u32, height: u32) -> Result<Frame, String> {
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
    };

    if width == 0 || height == 0 {
        return Err(tr("Rectángulo de captura vacío.", "Empty capture rectangle.").into());
    }
    let (w, h) = (width as i32, height as i32);

    // SAFETY: llamadas GDI estándar; todos los handles creados aquí se liberan
    // antes de salir y el búfer tiene exactamente width*height*4 bytes.
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err(tr(
                "No se pudo acceder a la pantalla (GetDC).",
                "Could not access the screen (GetDC).",
            )
            .into());
        }
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        let previous = SelectObject(memory, bitmap.into());
        let copied = BitBlt(memory, 0, 0, w, h, Some(screen), x, y, SRCCOPY);
        // GetDIBits exige que el bitmap ya no esté seleccionado en ningún DC.
        SelectObject(memory, previous);

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // negativo: filas de arriba abajo
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bgra = vec![0u8; width as usize * height as usize * 4];
        let lines = if copied.is_ok() {
            GetDIBits(
                screen,
                bitmap,
                0,
                height,
                Some(bgra.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };

        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);

        copied.map_err(|e| {
            format!(
                "{} (BitBlt): {e}",
                tr("No se pudo copiar la pantalla", "Could not copy the screen")
            )
        })?;
        if lines != h {
            return Err(tr(
                format!("Lectura incompleta de la pantalla: {lines} de {h} líneas."),
                format!("Incomplete screen read: {lines} of {h} lines."),
            ));
        }

        Ok(Frame {
            width,
            height,
            rgb: bgra
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[2], p[1], p[0]])
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::is_game_window;

    #[test]
    fn el_juego_se_reconoce_por_su_programa_y_no_por_un_titulo_parecido() {
        let gfn = r"c:\users\yo\appdata\local\nvidia corporation\geforcenow\cef\geforcenow.exe";
        assert!(is_game_window(gfn, "fortnite® on geforce now"));
        assert!(is_game_window(gfn, ""));
        assert!(is_game_window(
            r"c:\juegos\fortniteclient-win64-shipping.exe",
            "fortnite"
        ));
        assert!(is_game_window(
            r"c:\otro\reproductor.exe",
            "fortnite on geforce now"
        ));

        // Otros programas, aunque su título mencione Fortnite.
        assert!(!is_game_window(
            r"c:\windows\explorer.exe",
            "fortnitechestcounter"
        ));
        assert!(!is_game_window(
            r"c:\program files\chrome.exe",
            "fortnite - youtube"
        ));
        assert!(!is_game_window(
            r"c:\apps\fortnitechestcounter.exe",
            "fortnite chest counter"
        ));
        assert!(!is_game_window("", ""));
    }

    use super::Area;

    #[test]
    fn una_ventana_mas_alta_que_16_9_descuenta_las_franjas_negras() {
        let tall = Area {
            x: 100,
            y: 50,
            width: 1280,
            height: 1024,
            window: true,
        };
        assert_eq!(
            tall.content(),
            Area {
                x: 100,
                y: 50 + 152,
                width: 1280,
                height: 720,
                window: true
            }
        );
        let wide = Area {
            x: 0,
            y: 0,
            width: 2560,
            height: 1080,
            window: true,
        };
        assert_eq!(wide.content(), wide, "16:9 o más ancha: sin cambios");
        let exact = Area {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
            window: true,
        };
        assert_eq!(exact.content(), exact);
    }

    #[test]
    fn las_zonas_se_desplazan_al_sitio_de_la_ventana() {
        let area = Area {
            x: -1920,
            y: 40,
            width: 1920,
            height: 1080,
            window: true,
        };
        assert_eq!(area.absolute((10, 20, 30, 40)), (-1910, 60, 30, 40));
    }
}
