mod capture;
// La máquina de estados completa aún no se usa: falta leer de la pantalla el
// recuadro "BUSCAR" y el anillo. De momento cuenta `watch` con solo el aviso.
#[allow(dead_code)]
mod detect;
mod diag;
mod fuse;
mod input;
mod notice;
mod notice_templates;
mod scene;
mod settings;
mod store;
mod tracker;
mod watch;

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use diag::{Diag, DiagState};
use settings::{Settings, tr};
use store::{ChestType, EndReason, Store};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};
use tracker::{DEFAULT_INACTIVITY_MS, Stats, Tracker};

struct AppState(Mutex<Tracker>);

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Aplica un cambio al estado, avisa a la interfaz y devuelve los contadores.
pub(crate) fn update(
    app: &AppHandle,
    change: impl FnOnce(&mut Tracker, i64) -> rusqlite::Result<()>,
) -> Result<Stats, String> {
    let state = app.state::<AppState>();
    let mut tracker = state.0.lock().map_err(|e| e.to_string())?;
    change(&mut tracker, now_ms()).map_err(|e| e.to_string())?;
    let stats = tracker.stats().map_err(|e| e.to_string())?;
    #[cfg(debug_assertions)]
    eprintln!("[stats] {stats:?}");
    app.emit("stats-changed", stats)
        .map_err(|e| e.to_string())?;
    Ok(stats)
}

#[tauri::command]
fn get_stats(app: AppHandle) -> Result<Stats, String> {
    update(&app, |_, _| Ok(()))
}

#[tauri::command]
fn start_session(app: AppHandle) -> Result<Stats, String> {
    update(&app, |t, now| t.start_session(now, false).map(|_| ()))
}

#[tauri::command]
fn stop_session(app: AppHandle) -> Result<Stats, String> {
    update(&app, |t, now| t.stop_session(now))
}

#[tauri::command]
fn new_match(app: AppHandle) -> Result<Stats, String> {
    update(&app, |t, now| t.force_new_match(now))
}

/// Suma (o resta, con `delta` negativo) cofres al histórico a mano.
#[tauri::command]
fn adjust_total(app: AppHandle, chest: ChestType, delta: i64) -> Result<Stats, String> {
    let mut accepted = false;
    let stats = update(&app, |t, now| {
        accepted = t.adjust_total(now, chest, delta)?;
        Ok(())
    })?;
    if accepted {
        Ok(stats)
    } else {
        Err(tr(
            "El ajuste dejaría ese contador por debajo de cero.",
            "That adjustment would take the counter below zero.",
        )
        .into())
    }
}

/// Suma (o resta) partidas al total histórico a mano.
#[tauri::command]
fn adjust_matches(app: AppHandle, delta: i64) -> Result<Stats, String> {
    let mut accepted = false;
    let stats = update(&app, |t, now| {
        accepted = t.adjust_matches(now, delta)?;
        Ok(())
    })?;
    if accepted {
        Ok(stats)
    } else {
        Err(tr(
            "El ajuste dejaría las partidas por debajo de cero.",
            "That adjustment would take the match count below zero.",
        )
        .into())
    }
}

/// Pantalla inicial: lo que el jugador llevaba antes de usar el programa.
#[tauri::command]
fn initial_setup(
    app: AppHandle,
    normal: i64,
    rare: i64,
    spirit: i64,
    matches: i64,
) -> Result<Stats, String> {
    update(&app, |t, now| {
        let chests = [
            (ChestType::Normal, normal),
            (ChestType::Rare, rare),
            (ChestType::Spirit, spirit),
        ];
        t.reset_with_totals(now, &chests, matches)
    })
}

#[tauri::command]
fn is_debug() -> bool {
    cfg!(debug_assertions)
}

/// Simula eventos del detector para probar el resto de la app sin el juego.
/// Solo existe en compilaciones de depuración.
#[tauri::command]
fn debug_simulate(app: AppHandle, event: String) -> Result<Stats, String> {
    if !cfg!(debug_assertions) {
        return Err("solo disponible en compilaciones de depuración".into());
    }
    let chest = |kind: ChestType| update(&app, move |t, now| t.chest_opened(now, kind, 1.0, 0));
    match event.as_str() {
        "normal" => chest(ChestType::Normal),
        "rare" => chest(ChestType::Rare),
        "spirit" => chest(ChestType::Spirit),
        "match_start" => update(&app, |t, now| t.match_started(now)),
        "eliminated" => update(&app, |t, now| t.match_ended(now, EndReason::Eliminated)),
        "victory" => update(&app, |t, now| t.match_ended(now, EndReason::Victory)),
        "lobby" => update(&app, |t, now| t.match_ended(now, EndReason::Lobby)),
        other => Err(format!("evento desconocido: {other}")),
    }
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Result<Settings, String> {
    let tracker = state.0.lock().map_err(|e| e.to_string())?;
    tracker.settings().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_settings(
    app: AppHandle,
    state: State<AppState>,
    diag: State<Arc<Diag>>,
    settings: Settings,
) -> Result<(), String> {
    let mut tracker = state.0.lock().map_err(|e| e.to_string())?;
    tracker
        .save_settings(&settings)
        .map_err(|e| e.to_string())?;
    settings::set_language(&settings.language);
    diag.apply(&settings);
    hide_from_capture(&app, settings.hide_from_capture);
    apply_overlay(&app, &settings);
    let _ = app.emit("settings-changed", &settings);
    Ok(())
}

/// Excluye (o no) las ventanas de la app de capturas de pantalla y
/// grabaciones, incluidas las de la propia app. En Windows 10 2004 o posterior.
fn hide_from_capture(app: &AppHandle, hide: bool) {
    for label in ["main", "overlay"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.set_content_protected(hide);
        }
    }
}

/// Coloca el contador pegado a la esquina elegida. No lo muestra: eso lo
/// decide el bucle de detección, que solo lo enseña con el juego delante.
fn apply_overlay(app: &AppHandle, settings: &Settings) {
    let Some(window) = app.get_webview_window("overlay") else {
        return;
    };
    if !settings.show_overlay {
        let _ = window.hide();
    }
    if let (Ok(Some(monitor)), Ok(size)) = (window.primary_monitor(), window.outer_size()) {
        let (screen, origin) = (monitor.size(), monitor.position());
        let right = origin.x + screen.width as i32 - size.width as i32;
        let bottom = origin.y + screen.height as i32 - size.height as i32;
        // Abajo a la izquierda no se ofrece: ahí está la barra de vida.
        let (x, y) = match settings.corner.as_str() {
            "top-right" => (right, origin.y),
            "bottom-right" => (right, bottom),
            _ => (origin.x, origin.y),
        };
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

/// Muestra u oculta el contador sobre el juego. Al mostrarlo, el panel se
/// retira a la bandeja del sistema para no estorbar.
pub(crate) fn set_overlay_visible(app: &AppHandle, visible: bool) {
    let Some(overlay) = app.get_webview_window("overlay") else {
        return;
    };
    if !visible {
        let _ = overlay.hide();
        return;
    }
    // Primero mostrarla: en Linux no se puede hacer atravesable una ventana
    // que todavía no existe en pantalla.
    let _ = overlay.show();
    let _ = overlay.set_ignore_cursor_events(true);
    if let Some(panel) = app.get_webview_window("main") {
        let _ = panel.hide();
    }
}

fn show_panel(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window("main") {
        let _ = panel.show();
        let _ = panel.unminimize();
        let _ = panel.set_focus();
    }
}

/// Icono en la bandeja del sistema: clic para abrir el panel, menú para salir.
fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let open = MenuItem::with_id(
        app,
        "open",
        tr("Abrir panel", "Open panel"),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", tr("Salir", "Quit"), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::new()
        .tooltip("Fortnite Chest Counter")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_panel(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_panel(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Retardo de las acciones pedidas con un botón, para dar tiempo a volver al juego.
const BUTTON_DELAY_MS: i64 = 5_000;

#[tauri::command]
fn get_diag(diag: State<Arc<Diag>>) -> DiagState {
    diag.snapshot()
}

#[tauri::command]
fn diag_shot(diag: State<Arc<Diag>>) {
    diag.request_shot(BUTTON_DELAY_MS);
}

#[tauri::command]
fn diag_burst(diag: State<Arc<Diag>>) {
    diag.request_burst(BUTTON_DELAY_MS);
}

#[tauri::command]
fn open_captures_dir(diag: State<Arc<Diag>>) -> Result<(), String> {
    let opener = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(diag.dir())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Registra F8 y F9 como atajos globales del sistema. Es una vía distinta de
/// la consulta del teclado: el diagnóstico muestra cuál de las dos funciona.
fn register_hotkeys(app: &AppHandle, diag: &Arc<Diag>) -> Result<(), String> {
    let handler = diag.clone();
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |_app, shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    handler.hotkey(shortcut.key == Code::F9);
                }
            })
            .build(),
    )
    .map_err(|e| e.to_string())?;
    for key in [Code::F8, Code::F9] {
        app.global_shortcut()
            .register(Shortcut::new(None, key))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let store = Store::open(&dir.join("chests.db"))?;
            let tracker = Tracker::new(store, now_ms(), DEFAULT_INACTIVITY_MS)?;
            let settings = tracker.settings()?;
            settings::set_language(&settings.language);
            hide_from_capture(app.handle(), settings.hide_from_capture);
            apply_overlay(app.handle(), &settings);
            app.manage(AppState(Mutex::new(tracker)));

            // Diagnóstico: capturas en Imágenes/FortniteChestCounter.
            let captures = app
                .path()
                .picture_dir()
                .unwrap_or(dir)
                .join("FortniteChestCounter");
            std::fs::create_dir_all(&captures)?;
            let diag = Diag::new(captures, &settings);
            app.manage(diag.clone());
            if let Err(error) = register_hotkeys(app.handle(), &diag) {
                diag.set_hotkey_error(error);
            }
            if let Err(error) = build_tray(app.handle()) {
                diag.log(&format!("sin icono en la bandeja: {error}"));
            }
            diag::spawn(app.handle().clone(), diag.clone());
            watch::spawn(app.handle().clone(), diag);

            // Cierre de sesión por inactividad.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_secs(30));
                    let state = handle.state::<AppState>();
                    let Ok(mut tracker) = state.0.lock() else {
                        return;
                    };
                    if let Ok(true) = tracker.tick(now_ms())
                        && let Ok(stats) = tracker.stats()
                    {
                        let _ = handle.emit("stats-changed", stats);
                    }
                }
            });
            Ok(())
        })
        // Cerrar el panel cierra la app: el contador no puede quedarse solo.
        .on_window_event(|window, event| {
            if window.label() == "main"
                && matches!(event, tauri::WindowEvent::CloseRequested { .. })
            {
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_stats,
            start_session,
            stop_session,
            new_match,
            adjust_total,
            adjust_matches,
            initial_setup,
            is_debug,
            debug_simulate,
            get_settings,
            set_settings,
            get_diag,
            diag_shot,
            diag_burst,
            open_captures_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
