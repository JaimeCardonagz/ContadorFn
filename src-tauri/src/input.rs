//! Escucha pasiva de teclado, ratón y mando: solo se consulta si un botón está
//! pulsado. No se envía ninguna entrada ni se instala ningún gancho.

use crate::settings::Binding;

pub const VK_F8: u16 = 0x77;
pub const VK_F9: u16 = 0x78;

/// ¿Está pulsado ahora mismo el botón configurado?
pub fn binding_down(binding: &Binding) -> bool {
    match binding {
        Binding::Key { code, .. } => vk_down(*code),
        Binding::Mouse { button } => mouse_vk(*button).is_some_and(vk_down),
        Binding::Pad { button } => pad_button(*button).is_some_and(pad_down),
    }
}

/// Tecla virtual de Windows de un botón del ratón (numeración web).
fn mouse_vk(button: u8) -> Option<u16> {
    Some(match button {
        0 => 0x01, // izquierdo
        1 => 0x04, // central
        2 => 0x02, // derecho
        3 => 0x05, // lateral 1
        4 => 0x06, // lateral 2
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PadButton {
    /// Bit de `XINPUT_GAMEPAD.wButtons`.
    Mask(u16),
    LeftTrigger,
    RightTrigger,
}

/// Botón de XInput de un botón del mando (numeración del estándar web).
fn pad_button(button: u8) -> Option<PadButton> {
    Some(match button {
        0 => PadButton::Mask(0x1000), // A
        1 => PadButton::Mask(0x2000), // B
        2 => PadButton::Mask(0x4000), // X
        3 => PadButton::Mask(0x8000), // Y
        4 => PadButton::Mask(0x0100), // LB
        5 => PadButton::Mask(0x0200), // RB
        6 => PadButton::LeftTrigger,
        7 => PadButton::RightTrigger,
        8 => PadButton::Mask(0x0020),  // Atrás
        9 => PadButton::Mask(0x0010),  // Inicio
        10 => PadButton::Mask(0x0040), // L3
        11 => PadButton::Mask(0x0080), // R3
        12 => PadButton::Mask(0x0001), // cruceta arriba
        13 => PadButton::Mask(0x0002), // cruceta abajo
        14 => PadButton::Mask(0x0004), // cruceta izquierda
        15 => PadButton::Mask(0x0008), // cruceta derecha
        _ => return None,
    })
}

#[cfg(not(windows))]
pub fn vk_down(_virtual_key: u16) -> bool {
    false
}

#[cfg(not(windows))]
fn pad_down(_button: PadButton) -> bool {
    false
}

#[cfg(windows)]
pub fn vk_down(virtual_key: u16) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    // SAFETY: GetAsyncKeyState no tiene precondiciones.
    // El bit más alto indica que la tecla está pulsada ahora mismo.
    unsafe { GetAsyncKeyState(virtual_key as i32) < 0 }
}

/// ¿Está pulsado ese botón en alguno de los mandos conectados (XInput)?
#[cfg(windows)]
fn pad_down(button: PadButton) -> bool {
    use std::sync::Mutex;
    use windows::Win32::UI::Input::XboxController::{XINPUT_STATE, XInputGetState};

    /// Los disparadores son analógicos (0-255): pulsado a partir de aquí.
    const TRIGGER_THRESHOLD: u8 = 64;
    /// Consultar un puerto sin mando es lento: se reintenta cada 2 s.
    const RETRY_MS: i64 = 2_000;
    static RETRY_AT: Mutex<[i64; 4]> = Mutex::new([0; 4]);

    let now = crate::now_ms();
    let mut retry_at = RETRY_AT.lock().unwrap_or_else(|e| e.into_inner());
    let mut down = false;
    for (port, retry) in retry_at.iter_mut().enumerate() {
        if now < *retry {
            continue;
        }
        let mut state = XINPUT_STATE::default();
        // SAFETY: `state` es una estructura válida y el puerto está en 0..4.
        if unsafe { XInputGetState(port as u32, &mut state) } != 0 {
            *retry = now + RETRY_MS;
            continue;
        }
        let pad = state.Gamepad;
        down |= match button {
            PadButton::Mask(mask) => pad.wButtons.0 & mask != 0,
            PadButton::LeftTrigger => pad.bLeftTrigger >= TRIGGER_THRESHOLD,
            PadButton::RightTrigger => pad.bRightTrigger >= TRIGGER_THRESHOLD,
        };
    }
    down
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_botones_del_raton_se_traducen_a_teclas_virtuales() {
        let vks: Vec<_> = (0..6).map(mouse_vk).collect();
        assert_eq!(
            vks,
            [
                Some(0x01),
                Some(0x04),
                Some(0x02),
                Some(0x05),
                Some(0x06),
                None
            ]
        );
    }

    #[test]
    fn los_botones_del_mando_cubren_el_estandar_y_no_se_repiten() {
        let buttons: Vec<_> = (0..16).map(|b| pad_button(b).unwrap()).collect();
        for (i, a) in buttons.iter().enumerate() {
            assert!(!buttons[i + 1..].contains(a), "botón {i} repetido");
        }
        assert_eq!(
            pad_button(2),
            Some(PadButton::Mask(0x4000)),
            "X/□ abre por defecto"
        );
        assert_eq!(pad_button(16), None);
    }
}
