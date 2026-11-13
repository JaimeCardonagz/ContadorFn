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
pub fn game_in_foreground() -> bool {
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
        is_game_window(&program, &title)
    }
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
pub fn grab_rect(_x: u32, _y: u32, _width: u32, _height: u32) -> Result<Frame, String> {
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
pub fn grab_rect(x: u32, y: u32, width: u32, height: u32) -> Result<Frame, String> {
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
        let copied = BitBlt(
            memory,
            0,
            0,
            w,
            h,
            Some(screen),
            x as i32,
            y as i32,
            SRCCOPY,
        );
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
}
