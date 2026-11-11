//! Escucha pasiva del teclado: solo se consulta si una tecla está pulsada.
//! No se envía ninguna entrada ni se instala ningún gancho.

#[derive(Debug, Clone, Copy)]
pub enum Key {
    E,
    F8,
    F9,
}

#[cfg(not(windows))]
pub fn key_down(_key: Key) -> bool {
    false
}

#[cfg(windows)]
pub fn key_down(key: Key) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    let virtual_key = match key {
        Key::E => 0x45,
        Key::F8 => 0x77,
        Key::F9 => 0x78,
    };
    // SAFETY: GetAsyncKeyState no tiene precondiciones.
    // El bit más alto indica que la tecla está pulsada ahora mismo.
    unsafe { GetAsyncKeyState(virtual_key) < 0 }
}
