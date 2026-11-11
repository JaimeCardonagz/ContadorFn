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
