//! Captura de pantalla. Por ahora solo Windows y solo el monitor principal.
//!
//! Se usa GDI (BitBlt) porque es lo más simple que funciona con el escritorio
//! compuesto de Windows 10/11. Si con GeForce Now saliera en negro, el paso
//! siguiente es DXGI Desktop Duplication (ARQUITECTURA.md §3).

/// Un frame en RGB de 8 bits, fila a fila desde arriba.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

#[cfg(not(windows))]
pub fn grab_screen() -> Result<Frame, String> {
    Err("La captura de pantalla solo está implementada en Windows.".into())
}

#[cfg(windows)]
pub fn grab_screen() -> Result<Frame, String> {
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    // SAFETY: llamadas GDI estándar; todos los handles creados aquí se liberan
    // antes de salir y el búfer tiene exactamente width*height*4 bytes.
    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        if width <= 0 || height <= 0 {
            return Err(format!("Tamaño de pantalla no válido: {width}x{height}"));
        }

        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err("No se pudo acceder a la pantalla (GetDC).".into());
        }
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let previous = SelectObject(memory, bitmap.into());
        let copied = BitBlt(memory, 0, 0, width, height, Some(screen), 0, 0, SRCCOPY);
        // GetDIBits exige que el bitmap ya no esté seleccionado en ningún DC.
        SelectObject(memory, previous);

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // negativo: filas de arriba abajo
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
                height as u32,
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

        copied.map_err(|e| format!("No se pudo copiar la pantalla (BitBlt): {e}"))?;
        if lines != height {
            return Err(format!(
                "Lectura incompleta de la pantalla: {lines} de {height} líneas."
            ));
        }

        Ok(Frame {
            width: width as u32,
            height: height as u32,
            rgb: bgra
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[2], p[1], p[0]])
                .collect(),
        })
    }
}
