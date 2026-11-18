mod autostart;
mod backup;
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
mod panel;
mod paths;
mod placement;
mod scene;
mod settings;
mod store;
mod tracker;
mod updater;
mod vision;
mod watch;

use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use diag::{Diag, DiagState};
use settings::{Settings, tr};
use store::{ChestType, EndReason, Store};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};
use tracker::{DEFAULT_INACTIVITY_MS, Stats, Tracker};

struct AppState(Mutex<Tracker>);

/// Escribe en la base de datos el tamaño y la posición del panel, si cambiaron.
fn save_panel(app: &AppHandle) {
    if let Some(json) = panel::take(app)
        && let Ok(mut tracker) = app.state::<AppState>().0.lock()
    {
        let _ = tracker.set_meta(panel::META_KEY, &json);
    }
}

/// Último problema para ver el juego, para el aviso del panel.
struct VisionState(Mutex<vision::Problem>);

/// Guarda el problema para ver el juego y avisa al panel.
pub(crate) fn set_vision_problem(app: &AppHandle, problem: vision::Problem) {
    if let Some(state) = app.try_state::<VisionState>()
        && let Ok(mut current) = state.0.lock()
    {
        *current = problem;
    }
    let _ = app.emit("vision-problem", problem);
}

#[tauri::command]
fn get_vision_problem(state: State<VisionState>) -> vision::Problem {
    state.0.lock().map(|p| *p).unwrap_or_default()
}

/// Carpeta de las copias de seguridad.
struct BackupDir(std::path::PathBuf);

/// Atajo global que muestra u oculta el contador (el registrado ahora). Solo
/// existe si el sistema de atajos globales arrancó bien.
struct Hotkeys(Mutex<[Option<Shortcut>; HOTKEY_COUNT]>);
/// Posiciones en `Hotkeys`: mostrar u ocultar el contador, pausar el conteo
/// y terminar la partida.
const HOTKEY_OVERLAY: usize = 0;
const HOTKEY_PAUSE: usize = 1;
const HOTKEY_END_MATCH: usize = 2;
const HOTKEY_COUNT: usize = 3;
/// Versión nueva encontrada por `check_update`: la única que se puede instalar.
struct UpdateState(Mutex<Option<updater::Version>>);

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
    refresh_tray(app, &stats);
    app.emit("stats-changed", stats)
        .map_err(|e| e.to_string())?;
    Ok(stats)
}

#[tauri::command]
fn get_stats(app: AppHandle) -> Result<Stats, String> {
    update(&app, |_, _| Ok(()))
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
    // La interfaz ya lo valida; aquí, por si acaso, se rechaza lo absurdo.
    const MAX: i64 = 1_000_000;
    if [normal, rare, spirit, matches]
        .iter()
        .any(|n| !(0..=MAX).contains(n))
    {
        return Err(tr(
            "Los números deben estar entre 0 y 1.000.000.",
            "Numbers must be between 0 and 1,000,000.",
        )
        .into());
    }
    let backups = app.state::<BackupDir>().0.clone();
    update(&app, |t, now| {
        // Por si se reinició sin querer: lo de antes queda guardado aparte.
        let stats = t.stats()?;
        if stats.total_counts.total > 0 || stats.total_matches > 0 {
            let _ = backup::before_reset(t, &backups);
        }
        let chests = [
            (ChestType::Normal, normal),
            (ChestType::Rare, rare),
            (ChestType::Spirit, spirit),
        ];
        t.reset_with_totals(now, &chests, matches)
    })
}

/// Pausa o reanuda el conteo.
#[tauri::command]
fn set_paused(app: AppHandle, paused: bool) -> Result<Stats, String> {
    update(&app, |t, _| {
        t.set_paused(paused);
        Ok(())
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
    // Antes que nada: si un atajo no se puede registrar, no se guarda nada.
    set_global_hotkeys(
        &app,
        [
            &settings.overlay_hotkey,
            &settings.pause_hotkey,
            &settings.end_match_hotkey,
        ],
    )?;
    let mut tracker = state.0.lock().map_err(|e| e.to_string())?;
    tracker
        .save_settings(&settings)
        .map_err(|e| e.to_string())?;
    settings::set_language(&settings.language);
    if let Ok(stats) = tracker.stats() {
        refresh_tray(&app, &stats);
    }
    diag.apply(&settings);
    set_hotkeys(&app, &diag, settings.diagnostics);
    hide_from_capture(&app, settings.hide_from_capture);
    apply_overlay(&app, &settings);
    let _ = app.emit("settings-changed", &settings);
    autostart::set(settings.start_with_windows)?;
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
    if MOVING_OVERLAY.load(Relaxed) {
        return;
    }
    if !settings.show_overlay {
        let _ = window.hide();
    }
    if let (Ok(Some(monitor)), Ok(size)) = (window.primary_monitor(), window.outer_size()) {
        let (screen, origin) = (monitor.size(), monitor.position());
        let area = (origin.x, origin.y, screen.width, screen.height);
        let (x, y) = overlay_position(
            &settings.corner,
            area,
            (size.width, size.height),
            (settings.overlay_x, settings.overlay_y),
        );
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

/// Esquina superior izquierda del contador, pegado a la esquina elegida de la
/// zona `area` (x, y, ancho, alto). Abajo a la izquierda no se ofrece: ahí
/// está la barra de vida.
fn overlay_position(
    corner: &str,
    area: (i32, i32, u32, u32),
    size: (u32, u32),
    custom: (u16, u16),
) -> (i32, i32) {
    let (x, y, width, height) = area;
    let right = x + width as i32 - size.0 as i32;
    let bottom = y + height as i32 - size.1 as i32;
    match corner {
        "top-right" => (right, y),
        "bottom-right" => (right, bottom),
        "custom" => {
            let free_x = (width as i32 - size.0 as i32).max(0);
            let free_y = (height as i32 - size.1 as i32).max(0);
            let at = (
                x + free_x * i32::from(custom.0.min(1000)) / 1000,
                y + free_y * i32::from(custom.1.min(1000)) / 1000,
            );
            avoid_health_bar(at, area, size)
        }
        _ => (x, y),
    }
}

/// Zona de la barra de vida y el escudo, abajo a la izquierda, a 768 px de
/// alto (x1, alto desde abajo). Escala con la altura, como el HUD del juego.
const HEALTH_BAR: (u32, u32) = (360, 128);

/// Si el contador taparía la barra de vida, se sube justo por encima.
fn avoid_health_bar(at: (i32, i32), area: (i32, i32, u32, u32), size: (u32, u32)) -> (i32, i32) {
    let (x, y, _, height) = area;
    let scale = |v: u32| (v * height / 768) as i32;
    let (zone_right, zone_top) = (
        x + scale(HEALTH_BAR.0),
        y + height as i32 - scale(HEALTH_BAR.1),
    );
    let overlaps = at.0 < zone_right && at.1 + size.1 as i32 > zone_top;
    if overlaps {
        (at.0, (zone_top - size.1 as i32).max(y))
    } else {
        at
    }
}

/// Posición personalizada (en milésimas) que corresponde a dejar el contador
/// en `at` dentro de `area`.
fn overlay_fraction(at: (i32, i32), area: (i32, i32, u32, u32), size: (u32, u32)) -> (u16, u16) {
    let (x, y, width, height) = area;
    let part = |offset: i32, free: i32| {
        if free <= 0 {
            0
        } else {
            (i64::from(offset.clamp(0, free)) * 1000 / i64::from(free)) as u16
        }
    };
    (
        part(at.0 - x, width as i32 - size.0 as i32),
        part(at.1 - y, height as i32 - size.1 as i32),
    )
}

/// ¿Está el análisis esperando a que el juego esté delante?
static DETECTION_WAITING: AtomicBool = AtomicBool::new(true);

/// El bucle de detección avisa de si está analizando o esperando al juego.
pub(crate) fn set_detection_waiting(app: &AppHandle, waiting: bool) {
    DETECTION_WAITING.store(waiting, Relaxed);
    let _ = app.emit("detection-waiting", waiting);
}

#[tauri::command]
fn get_detection_waiting() -> bool {
    DETECTION_WAITING.load(Relaxed)
}

/// Última zona donde se colocó el contador (para recolocarlo si cambia de tamaño).
static LAST_PLACED_AREA: Mutex<Option<capture::Area>> = Mutex::new(None);

/// El contador ajusta su ventana a lo que muestra (ancho y alto en píxeles
/// lógicos) y se recoloca, porque en las esquinas de la derecha o de abajo su
/// posición depende de su tamaño.
#[tauri::command]
fn fit_overlay(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let window = app.get_webview_window("overlay").ok_or("sin contador")?;
    let size = tauri::LogicalSize::new(width.clamp(40.0, 900.0), height.clamp(16.0, 500.0));
    window.set_size(size).map_err(|e| e.to_string())?;
    if let Some(area) = LAST_PLACED_AREA.lock().ok().and_then(|a| *a) {
        place_overlay(&app, &area);
    }
    Ok(())
}

/// Mientras se coloca el contador arrastrándolo, nadie más lo mueve ni lo oculta.
static MOVING_OVERLAY: AtomicBool = AtomicBool::new(false);
/// Última zona donde se vio la ventana del juego: ahí se coloca el contador.
static LAST_GAME_AREA: Mutex<Option<capture::Area>> = Mutex::new(None);

/// El bucle de detección avisa de dónde está la ventana del juego.
pub(crate) fn remember_game_area(area: &capture::Area) {
    if area.window
        && let Ok(mut last) = LAST_GAME_AREA.lock()
    {
        *last = Some(*area);
    }
}

/// Zona donde colocar el contador al moverlo: la última ventana del juego o,
/// si no se ha visto, la pantalla principal.
fn move_area(window: &tauri::WebviewWindow) -> Option<(i32, i32, u32, u32)> {
    if let Some(area) = LAST_GAME_AREA.lock().ok().and_then(|last| *last) {
        return Some((area.x, area.y, area.width, area.height));
    }
    let monitor = window.primary_monitor().ok().flatten()?;
    let (origin, size) = (monitor.position(), monitor.size());
    Some((origin.x, origin.y, size.width, size.height))
}

/// Empieza a colocar el contador: se ve y se puede arrastrar.
#[tauri::command]
fn start_overlay_move(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let window = app.get_webview_window("overlay").ok_or("sin contador")?;
    let settings = state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .settings()
        .map_err(|e| e.to_string())?;
    MOVING_OVERLAY.store(true, Relaxed);
    if let (Some(area), Ok(size)) = (move_area(&window), window.outer_size()) {
        let (x, y) = overlay_position(
            &settings.corner,
            area,
            (size.width, size.height),
            (settings.overlay_x, settings.overlay_y),
        );
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    }
    let _ = window.set_focusable(true);
    let _ = window.show();
    let _ = window.set_ignore_cursor_events(false);
    let _ = window.set_focus();
    let _ = app.emit("overlay-moving", true);
    Ok(())
}

/// Termina de colocar el contador; con `save`, guarda su posición como
/// "Posición personalizada" (fuera de la barra de vida).
#[tauri::command]
fn finish_overlay_move(
    app: AppHandle,
    state: State<AppState>,
    save: bool,
) -> Result<Settings, String> {
    let window = app.get_webview_window("overlay").ok_or("sin contador")?;
    let mut tracker = state.0.lock().map_err(|e| e.to_string())?;
    let mut settings = tracker.settings().map_err(|e| e.to_string())?;
    if save
        && let (Some(area), Ok(at), Ok(size)) = (
            move_area(&window),
            window.outer_position(),
            window.outer_size(),
        )
    {
        let size = (size.width, size.height);
        let at = avoid_health_bar((at.x, at.y), area, size);
        (settings.overlay_x, settings.overlay_y) = overlay_fraction(at, area, size);
        settings.corner = "custom".into();
        tracker
            .save_settings(&settings)
            .map_err(|e| e.to_string())?;
    }
    drop(tracker);
    MOVING_OVERLAY.store(false, Relaxed);
    let _ = window.set_ignore_cursor_events(true);
    let _ = window.set_focusable(false);
    let _ = window.hide();
    let _ = app.emit("overlay-moving", false);
    let _ = app.emit("settings-changed", &settings);
    Ok(settings)
}

/// Avisa al contador de que una partida acaba de terminar, con su resumen
/// (duración y cofres). El contador decide si lo muestra según los ajustes.
pub(crate) fn announce_match_summary(app: &AppHandle) {
    let summary = app
        .state::<AppState>()
        .0
        .lock()
        .ok()
        .and_then(|tracker| tracker.history(now_ms()).ok())
        .and_then(|history| history.into_iter().next());
    if let Some(summary) = summary {
        let _ = app.emit("match-summary", summary);
    }
}

/// Coloca el contador en la esquina elegida de la zona donde se ve el juego
/// (su ventana, en el monitor que esté).
pub(crate) fn place_overlay(app: &AppHandle, area: &capture::Area) {
    let Some(window) = app.get_webview_window("overlay") else {
        return;
    };
    if let Ok(mut last) = LAST_PLACED_AREA.lock() {
        *last = Some(*area);
    }
    // Alto de la zona del juego en píxeles lógicos: con él, el tamaño
    // "Automático" del contador escala con la resolución.
    let scale = window.scale_factor().unwrap_or(1.0).max(0.5);
    let _ = app.emit("overlay-area", f64::from(area.height) / scale);
    if MOVING_OVERLAY.load(Relaxed) {
        return;
    }
    let Some(settings) = app
        .state::<AppState>()
        .0
        .lock()
        .ok()
        .and_then(|tracker| tracker.settings().ok())
    else {
        return;
    };
    if let Ok(size) = window.outer_size() {
        let zone = (area.x, area.y, area.width, area.height);
        let (x, y) = overlay_position(
            &settings.corner,
            zone,
            (size.width, size.height),
            (settings.overlay_x, settings.overlay_y),
        );
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

/// Muestra u oculta el contador sobre el juego. Al mostrarlo, el panel se
/// retira a la bandeja del sistema para no estorbar.
pub(crate) fn set_overlay_visible(app: &AppHandle, visible: bool) {
    let Some(overlay) = app.get_webview_window("overlay") else {
        return;
    };
    if MOVING_OVERLAY.load(Relaxed) {
        return;
    }
    if !visible {
        let _ = overlay.hide();
        return;
    }
    // Primero mostrarla: en Linux no se puede hacer atravesable una ventana
    // que todavía no existe en pantalla.
    let _ = overlay.show();
    let _ = overlay.set_ignore_cursor_events(true);
    // Mostrarla puede quitarle el foco al juego: se le devuelve.
    capture::give_focus_back_to_game();
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
const TRAY_ID: &str = "main";

/// Texto al pasar el ratón por el icono de la bandeja: cofres de la partida y
/// del día. Corto: Windows no enseña más de 127 caracteres.
fn tray_tooltip(stats: &Stats) -> String {
    let chests = |n: u32| {
        format!(
            "{n} {}",
            if n == 1 {
                tr("cofre", "chest")
            } else {
                tr("cofres", "chests")
            }
        )
    };
    let current = if stats.in_match {
        format!(
            "{}: {}",
            tr("Partida", "Match"),
            chests(stats.match_counts.total)
        )
    } else {
        tr("Fuera de partida", "Not in a match").to_string()
    };
    let matches = stats.session_matches;
    let day = format!(
        "{}: {} · {matches} {}",
        tr("Hoy", "Today"),
        chests(stats.session_counts.total),
        if matches == 1 {
            tr("partida", "match")
        } else {
            tr("partidas", "matches")
        }
    );
    let paused = if stats.paused {
        tr(
            "\nEn pausa: no se cuenta nada",
            "\nPaused: nothing is counted",
        )
    } else {
        ""
    };
    format!("Fortnite Chest Counter\n{current}\n{day}{paused}")
}

/// Opciones del menú de la bandeja, para poner al día sus textos.
struct TrayItems(Vec<(&'static str, tauri::menu::MenuItem<tauri::Wry>)>);

/// Texto de cada opción del menú de la bandeja.
fn tray_label(id: &str, stats: &Stats) -> &'static str {
    match id {
        "open" => tr("Abrir panel", "Open panel"),
        "pause" if stats.paused => tr("Reanudar conteo", "Resume counting"),
        "pause" => tr("Pausar conteo", "Pause counting"),
        "overlay" => tr("Mostrar u ocultar el contador", "Show or hide the counter"),
        _ => tr("Salir", "Quit"),
    }
}

/// Pone al día el texto del icono de la bandeja.
fn refresh_tray(app: &AppHandle, stats: &Stats) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tray_tooltip(stats)));
    }
    if let Some(items) = app.try_state::<TrayItems>() {
        for (id, item) in &items.0 {
            let _ = item.set_text(tray_label(id, stats));
        }
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    use tauri::menu::PredefinedMenuItem;

    let stats = Stats::default();
    let item = |id: &'static str| {
        MenuItem::with_id(app, id, tray_label(id, &stats), true, None::<&str>)
            .map(|item| (id, item))
    };
    let items = ["open", "pause", "overlay", "quit"]
        .into_iter()
        .map(item)
        .collect::<tauri::Result<Vec<_>>>()?;
    let separator = PredefinedMenuItem::separator(app)?;
    let find = |id: &str| &items.iter().find(|(i, _)| *i == id).expect("opción").1;
    let menu = Menu::with_items(
        app,
        &[
            find("open"),
            &separator,
            find("pause"),
            find("overlay"),
            &PredefinedMenuItem::separator(app)?,
            find("quit"),
        ],
    )?;
    app.manage(TrayItems(items));
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Fortnite Chest Counter")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_panel(app),
            "pause" => toggle_pause(app),
            "overlay" => toggle_overlay(app),
            "quit" => {
                save_panel(app);
                app.exit(0)
            }
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

/// Página de descargas del proyecto.
const RELEASES_URL: &str = "https://github.com/JaimeCardonagz/ContadorFn/releases/latest";

/// Abre una carpeta o una dirección web con el programa del sistema.
fn open_external(target: &std::ffi::OsStr) -> Result<(), String> {
    let opener = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Racha del objetivo diario. `offset_minutes`: lo que da la interfaz con
/// `getTimezoneOffset()` (minutos que hay que sumar a la hora local para la UTC).
#[tauri::command]
fn get_streak(state: State<AppState>, offset_minutes: i32) -> Result<tracker::Streak, String> {
    let offset_ms = -i64::from(offset_minutes.clamp(-24 * 60, 24 * 60)) * 60_000;
    let tracker = state.0.lock().map_err(|e| e.to_string())?;
    tracker
        .streak(now_ms(), offset_ms)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_backups(dir: State<BackupDir>) -> Vec<backup::BackupFile> {
    backup::list(&dir.0)
}

/// Recupera una copia de seguridad (por su nombre, de las de `list_backups`).
#[tauri::command]
fn restore_backup(app: AppHandle, dir: State<BackupDir>, name: String) -> Result<Stats, String> {
    let state = app.state::<AppState>();
    let stats = {
        let mut tracker = state.0.lock().map_err(|e| e.to_string())?;
        backup::restore(&mut tracker, &dir.0, &name, now_ms())?;
        tracker.stats().map_err(|e| e.to_string())?
    };
    refresh_tray(&app, &stats);
    let _ = app.emit("stats-changed", stats);
    Ok(stats)
}

/// Todas las partidas terminadas, para exportarlas.
#[tauri::command]
fn get_all_matches(state: State<AppState>) -> Result<Vec<store::MatchSummary>, String> {
    let tracker = state.0.lock().map_err(|e| e.to_string())?;
    tracker.all_matches().map_err(|e| e.to_string())
}

#[tauri::command]
fn backup_info(dir: State<BackupDir>) -> backup::BackupInfo {
    backup::info(&dir.0)
}

#[tauri::command]
fn open_backups_dir(dir: State<BackupDir>) -> Result<(), String> {
    std::fs::create_dir_all(&dir.0).map_err(|e| e.to_string())?;
    open_external(dir.0.as_os_str())
}

#[tauri::command]
fn open_captures_dir(diag: State<Arc<Diag>>) -> Result<(), String> {
    open_external(diag.dir().as_os_str())
}

/// Abre la página de descargas (siempre la misma dirección, fija aquí).
#[tauri::command]
fn open_releases() -> Result<(), String> {
    open_external(RELEASES_URL.as_ref())
}

/// Busca una versión nueva (siempre: no se puede desactivar).
#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<String>, String> {
    let found = tauri::async_runtime::spawn_blocking(updater::newer_version)
        .await
        .map_err(|e| e.to_string())??;
    *app.state::<UpdateState>()
        .0
        .lock()
        .map_err(|e| e.to_string())? = found;
    Ok(found.map(|v| v.to_string()))
}

/// Descarga, verifica e instala la versión que encontró `check_update`, y
/// reinicia el programa. La interfaz no puede pedir otra versión ni otro archivo.
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    let version = app
        .state::<UpdateState>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            tr(
                "No hay ninguna versión nueva comprobada.",
                "No new version has been checked.",
            )
            .to_string()
        })?;
    if !cfg!(windows) {
        return Err(tr(
            "La actualización automática solo está disponible en Windows.",
            "Automatic updates are only available on Windows.",
        )
        .into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = exe.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = updater::download_verified(version)?;
        updater::install_at(&target, &bytes)
    })
    .await
    .map_err(|e| e.to_string())??;
    // La versión nueva espera a que esta termine (instancia única).
    std::process::Command::new(&exe)
        .args([updater::WAIT_ARG, &std::process::id().to_string()])
        .spawn()
        .map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn get_history(state: State<AppState>) -> Result<Vec<store::MatchSummary>, String> {
    let tracker = state.0.lock().map_err(|e| e.to_string())?;
    tracker.history(now_ms()).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_records(state: State<AppState>) -> Result<tracker::Records, String> {
    let tracker = state.0.lock().map_err(|e| e.to_string())?;
    tracker.records(now_ms()).map_err(|e| e.to_string())
}

/// Convierte el texto de un atajo ("Ctrl+Alt+KeyK"); vacío = sin atajo.
fn parse_hotkey(accel: &str) -> Result<Option<Shortcut>, String> {
    if accel.is_empty() {
        return Ok(None);
    }
    let shortcut: Shortcut = accel
        .parse()
        .map_err(|_| tr("Ese atajo no es válido.", "That shortcut is not valid.").to_string())?;
    if shortcut.mods.is_empty() && matches!(shortcut.key, Code::F8 | Code::F9) {
        return Err(tr(
            "F8 y F9 están reservadas para el diagnóstico. Elige otra tecla.",
            "F8 and F9 are reserved for diagnostics. Choose another key.",
        )
        .into());
    }
    Ok(Some(shortcut))
}

/// Pone en `slot` el atajo `wanted`, registrándolo en el sistema y quitando
/// el anterior.
fn set_hotkey_slot(app: &AppHandle, slot: usize, wanted: Option<Shortcut>) -> Result<(), String> {
    let Some(state) = app.try_state::<Hotkeys>() else {
        return match wanted {
            None => Ok(()),
            Some(_) => Err(tr(
                "Los atajos globales no están disponibles en este equipo.",
                "Global shortcuts are not available on this computer.",
            )
            .into()),
        };
    };
    let mut current = state.0.lock().map_err(|e| e.to_string())?;
    if current[slot] == wanted {
        return Ok(());
    }
    let shortcuts = app.global_shortcut();
    if let Some(shortcut) = wanted {
        shortcuts.register(shortcut).map_err(|_| {
            tr(
                "Ese atajo ya lo usa otro programa. Elige otro.",
                "Another program already uses that shortcut. Choose another one.",
            )
            .to_string()
        })?;
    }
    if let Some(old) = current[slot].take() {
        let _ = shortcuts.unregister(old);
    }
    current[slot] = wanted;
    Ok(())
}

/// Registra los atajos globales (contador, pausa y terminar partida), en el
/// orden de `HOTKEY_*`. Si alguno falla, todo queda como estaba.
fn set_global_hotkeys(app: &AppHandle, accels: [&str; HOTKEY_COUNT]) -> Result<(), String> {
    let mut wanted = [None; HOTKEY_COUNT];
    for (slot, accel) in accels.iter().enumerate() {
        wanted[slot] = parse_hotkey(accel)?;
    }
    for (i, a) in wanted.iter().enumerate() {
        if a.is_some() && wanted[i + 1..].contains(a) {
            return Err(tr(
                "Dos atajos no pueden ser iguales.",
                "Two shortcuts can't be the same.",
            )
            .into());
        }
    }
    let previous = app
        .try_state::<Hotkeys>()
        .and_then(|state| state.0.lock().ok().map(|c| *c))
        .unwrap_or_default();
    // Primero se sueltan los que cambian, por si se intercambian entre sí.
    for slot in 0..HOTKEY_COUNT {
        if previous[slot] != wanted[slot] {
            set_hotkey_slot(app, slot, None)?;
        }
    }
    let result = (0..HOTKEY_COUNT).try_for_each(|slot| set_hotkey_slot(app, slot, wanted[slot]));
    if result.is_err() {
        for slot in 0..HOTKEY_COUNT {
            let _ = set_hotkey_slot(app, slot, None);
        }
        for (slot, shortcut) in previous.into_iter().enumerate() {
            let _ = set_hotkey_slot(app, slot, shortcut);
        }
    }
    result
}

/// Termina la partida en curso ahora mismo (atajo global): para cuando se
/// vuelve al lobby sin pasar por la pantalla de puesto final.
fn end_match_now(app: &AppHandle) {
    let mut closed = false;
    let result = update(app, |t, now| {
        closed = t.stats()?.in_match;
        t.match_ended(now, store::EndReason::Manual)
    });
    if closed && result.is_ok() {
        announce_match_summary(app);
    }
    // El vigilante de partidas deja de dar la partida por abierta.
    END_MATCH_REQUESTED.store(true, Relaxed);
}

/// Pedido desde el atajo "Terminar partida"; lo recoge el bucle de detección.
static END_MATCH_REQUESTED: AtomicBool = AtomicBool::new(false);

/// ¿Se pidió terminar la partida a mano? (y lo da por atendido)
pub(crate) fn take_end_match_request() -> bool {
    END_MATCH_REQUESTED.swap(false, Relaxed)
}

/// Pausa o reanuda el conteo (menú de la bandeja y atajo global).
fn toggle_pause(app: &AppHandle) {
    let paused = app
        .state::<AppState>()
        .0
        .lock()
        .ok()
        .and_then(|t| t.stats().ok())
        .is_some_and(|s| s.paused);
    let _ = update(app, |t, _| {
        t.set_paused(!paused);
        Ok(())
    });
}

/// Muestra u oculta el contador (el atajo global): cambia el ajuste, lo
/// guarda y avisa al panel para que la casilla lo refleje.
fn toggle_overlay(app: &AppHandle) {
    let settings = {
        let state = app.state::<AppState>();
        let Ok(mut tracker) = state.0.lock() else {
            return;
        };
        let Ok(mut settings) = tracker.settings() else {
            return;
        };
        settings.show_overlay = !settings.show_overlay;
        if tracker.save_settings(&settings).is_err() {
            return;
        }
        settings
    };
    app.state::<Arc<Diag>>().apply(&settings);
    apply_overlay(app, &settings);
    let _ = app.emit("settings-changed", &settings);
}

/// Activa o desactiva F8 y F9 como atajos globales del sistema. Solo con el
/// modo diagnóstico: así no se le quitan esas teclas a otros programas.
fn set_hotkeys(app: &AppHandle, diag: &Diag, on: bool) {
    // Sin el sistema de atajos (no arrancó) no hay nada que activar.
    if app.try_state::<Hotkeys>().is_none() {
        return;
    }
    let shortcuts = app.global_shortcut();
    for key in [Code::F8, Code::F9] {
        let shortcut = Shortcut::new(None, key);
        let result = if on {
            if shortcuts.is_registered(shortcut) {
                Ok(())
            } else {
                shortcuts.register(shortcut)
            }
        } else if shortcuts.is_registered(shortcut) {
            shortcuts.unregister(shortcut)
        } else {
            Ok(())
        };
        if let Err(error) = result {
            diag.set_hotkey_error(error.to_string());
        }
    }
}

/// Crea el panel y el contador. Se hace aquí y no en tauri.conf.json para
/// poder elegir la carpeta de caché de la ventana (si no, lleva el
/// identificador interno de la app).
fn create_windows(
    app: &AppHandle,
    webview_dir: &std::path::Path,
    show_panel: bool,
    panel_geometry: Option<&str>,
) -> tauri::Result<()> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    // Oculto hasta ponerlo donde estaba la última vez.
    let main = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("/".into()))
        .title("Fortnite Chest Counter")
        .inner_size(440.0, 620.0)
        .min_inner_size(320.0, 360.0)
        .visible(false)
        .data_directory(webview_dir.to_path_buf())
        .build()?;
    panel::restore(&main, panel_geometry);
    panel::seed(app, panel_geometry);
    if show_panel {
        main.show()?;
    }
    WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("/overlay".into()))
        .title("Fortnite Chest Counter - contador")
        .inner_size(250.0, 104.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(false)
        .focusable(false)
        .visible(false)
        .data_directory(webview_dir.to_path_buf())
        .build()?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Tras actualizar: esperar a que la versión anterior se cierre.
    updater::wait_for_previous(&std::env::args().collect::<Vec<_>>());
    tauri::Builder::default()
        // Una sola copia del programa: si se abre otra, se muestra el panel
        // de la que ya está abierta y la nueva se cierra (si no, contarían
        // los mismos cofres dos veces). Debe ser el primer plugin.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_panel(app);
        }))
        .setup(|app| {
            // Restos de una actualización anterior.
            if let Ok(exe) = std::env::current_exe() {
                updater::cleanup(&exe);
            }
            app.manage(UpdateState(Mutex::new(None)));
            // Carpetas con el nombre de la app; la base de datos de versiones
            // anteriores (carpeta con el identificador interno) se trae aquí.
            let dir = app.path().data_dir()?.join(paths::APP_DIR);
            let webview_dir = app.path().local_data_dir()?.join(paths::APP_DIR);
            if let Ok(old) = app.path().app_data_dir()
                && let Ok(true) = paths::migrate_database(&old, &dir)
                && let Ok(old_local) = app.path().app_local_data_dir()
                && old_local != webview_dir
            {
                // Caché de la ventana de la versión anterior: ya no se usa.
                let _ = std::fs::remove_dir_all(old_local);
            }
            std::fs::create_dir_all(&dir)?;
            let store = Store::open(&dir.join("chests.db"))?;
            let tracker = Tracker::new(store, now_ms(), DEFAULT_INACTIVITY_MS)?;
            let settings = tracker.settings()?;
            // Abierto al iniciar sesión: directo a la bandeja (salvo la primera
            // vez, que hay que contestar la pantalla inicial).
            let to_tray = autostart::started_by_system(&std::env::args().collect::<Vec<_>>())
                && settings.setup_done;
            app.manage(panel::PanelState::default());
            let panel_geometry = tracker.meta(panel::META_KEY).ok().flatten();
            create_windows(
                app.handle(),
                &webview_dir,
                !to_tray,
                panel_geometry.as_deref(),
            )?;
            if settings.start_with_windows {
                // Mantiene la ruta al día por si se movió el programa.
                let _ = autostart::set(true);
            }
            settings::set_language(&settings.language);
            hide_from_capture(app.handle(), settings.hide_from_capture);
            apply_overlay(app.handle(), &settings);
            app.manage(AppState(Mutex::new(tracker)));
            app.manage(BackupDir(dir.join(backup::DIR)));
            app.manage(VisionState(Mutex::new(vision::Problem::None)));

            // Modo diagnóstico (registro, capturas, recortes y atajos F8/F9):
            // apagado salvo que el usuario lo active en Configuración.
            let captures = app
                .path()
                .picture_dir()
                .unwrap_or(dir)
                .join("FortniteChestCounter");
            if settings.diagnostics {
                std::fs::create_dir_all(&captures)?;
            }
            let diag = Diag::new(captures, &settings);
            app.manage(diag.clone());
            let handler = diag.clone();
            // Si los atajos globales no están disponibles, el programa sigue sin ellos.
            match app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |app, shortcut, event| {
                        if event.state() != ShortcutState::Pressed {
                            return;
                        }
                        let slot = app.try_state::<Hotkeys>().and_then(|state| {
                            state
                                .0
                                .lock()
                                .ok()
                                .and_then(|c| c.iter().position(|s| *s == Some(*shortcut)))
                        });
                        if slot == Some(HOTKEY_OVERLAY) {
                            toggle_overlay(app);
                        } else if slot == Some(HOTKEY_PAUSE) {
                            toggle_pause(app);
                        } else if slot == Some(HOTKEY_END_MATCH) {
                            end_match_now(app);
                        } else if shortcut.mods.is_empty()
                            && matches!(shortcut.key, Code::F8 | Code::F9)
                        {
                            handler.hotkey(shortcut.key == Code::F9);
                        }
                    })
                    .build(),
            ) {
                Ok(()) => {
                    app.manage(Hotkeys(Mutex::new([None; HOTKEY_COUNT])));
                    set_hotkeys(app.handle(), &diag, settings.diagnostics);
                    if let Err(error) = set_global_hotkeys(
                        app.handle(),
                        [
                            &settings.overlay_hotkey,
                            &settings.pause_hotkey,
                            &settings.end_match_hotkey,
                        ],
                    ) {
                        diag.log(&format!("atajos globales: {error}"));
                    }
                }
                Err(error) => diag.set_hotkey_error(error.to_string()),
            }
            if let Err(error) = build_tray(app.handle()) {
                diag.log(&format!("sin icono en la bandeja: {error}"));
            } else if let Ok(Ok(stats)) = app.state::<AppState>().0.lock().map(|t| t.stats()) {
                refresh_tray(app.handle(), &stats);
            }
            diag::spawn(app.handle().clone(), diag.clone());
            watch::spawn(app.handle().clone(), diag);

            // Cierre de sesión por inactividad y copia de seguridad diaria.
            let handle = app.handle().clone();
            let backups = handle.state::<BackupDir>().0.clone();
            std::thread::spawn(move || {
                let mut backed_up = String::new();
                loop {
                    let state = handle.state::<AppState>();
                    let Ok(tracker) = state.0.lock() else {
                        return;
                    };
                    let day = backup::today(now_ms());
                    if day != backed_up && tracker.settings().is_ok_and(|s| s.setup_done) {
                        match backup::daily(&tracker, &backups, &day) {
                            Ok(_) => backed_up = day,
                            Err(error) => eprintln!("copia de seguridad: {error}"),
                        }
                    }
                    drop(tracker);
                    std::thread::sleep(Duration::from_secs(30));
                    save_panel(&handle);
                    let Ok(mut tracker) = state.0.lock() else {
                        return;
                    };
                    if let Ok(true) = tracker.tick(now_ms())
                        && let Ok(stats) = tracker.stats()
                    {
                        refresh_tray(&handle, &stats);
                        let _ = handle.emit("stats-changed", stats);
                    }
                }
            });
            Ok(())
        })
        // Cerrar el panel cierra la app: el contador no puede quedarse solo.
        // Cerrar el panel: a la bandeja (por defecto) o salir del programa.
        // El contador no puede quedarse solo, así que "salir" cierra todo.
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if matches!(
                event,
                tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)
            ) && let Some(panel) = window.app_handle().get_webview_window("main")
            {
                panel::remember(&panel);
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                save_panel(app);
                let to_tray = app
                    .state::<AppState>()
                    .0
                    .lock()
                    .ok()
                    .and_then(|tracker| tracker.settings().ok())
                    .is_none_or(|settings| settings.close_to_tray);
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    app.exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_stats,
            adjust_total,
            adjust_matches,
            set_paused,
            initial_setup,
            is_debug,
            debug_simulate,
            get_settings,
            set_settings,
            get_diag,
            diag_shot,
            diag_burst,
            open_captures_dir,
            open_releases,
            check_update,
            install_update,
            get_history,
            get_records,
            backup_info,
            get_vision_problem,
            get_detection_waiting,
            fit_overlay,
            list_backups,
            get_streak,
            restore_backup,
            get_all_matches,
            start_overlay_move,
            finish_overlay_move,
            open_backups_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{overlay_fraction, overlay_position, tray_tooltip};

    #[test]
    fn la_posicion_personalizada_se_adapta_a_la_zona_y_evita_la_barra_de_vida() {
        let full_hd = (0, 0, 1920, 1080);
        let size = (250, 104);
        // A la derecha, a media altura.
        assert_eq!(
            overlay_position("custom", full_hd, size, (1000, 500)),
            (1670, 488)
        );
        // En otra resolución, el mismo sitio relativo.
        assert_eq!(
            overlay_position("custom", (0, 0, 1366, 768), size, (1000, 500)),
            (1116, 332)
        );
        // Abajo a la izquierda taparía la barra de vida: se sube justo encima.
        let (x, y) = overlay_position("custom", full_hd, size, (0, 1000));
        assert_eq!((x, y), (0, 1080 - 180 - 104));
        // Abajo, pero a la derecha de la barra: se queda.
        assert_eq!(
            overlay_position("custom", full_hd, size, (500, 1000)),
            (835, 976)
        );
        // Ida y vuelta: guardar la posición y volver a calcularla.
        let at = (700, 300);
        let fraction = overlay_fraction(at, full_hd, size);
        let back = overlay_position("custom", full_hd, size, fraction);
        assert!((back.0 - at.0).abs() <= 2 && (back.1 - at.1).abs() <= 2);
        // Fuera de la zona: se queda en el borde.
        assert_eq!(overlay_fraction((-50, 5000), full_hd, size), (0, 1000));
    }
    use crate::store::Counts;
    use crate::tracker::Stats;

    #[test]
    fn el_icono_de_la_bandeja_ensena_la_partida_y_el_dia() {
        let counts = |total| Counts {
            total,
            ..Counts::default()
        };
        let mut stats = Stats {
            in_match: true,
            match_counts: counts(1),
            session_counts: counts(12),
            session_matches: 4,
            ..Stats::default()
        };
        assert_eq!(
            tray_tooltip(&stats),
            "Fortnite Chest Counter\nPartida: 1 cofre\nHoy: 12 cofres · 4 partidas"
        );
        stats.in_match = false;
        stats.session_matches = 1;
        let text = tray_tooltip(&stats);
        assert_eq!(
            text,
            "Fortnite Chest Counter\nFuera de partida\nHoy: 12 cofres · 1 partida"
        );
        assert!(text.chars().count() < 128, "Windows corta a 127 caracteres");
    }

    #[test]
    fn el_contador_se_pega_a_la_esquina_de_la_ventana_del_juego() {
        // Ventana de 1280×720 en un segundo monitor a la izquierda.
        let area = (-1600, 100, 1280, 720);
        assert_eq!(
            overlay_position("top-left", area, (250, 104), (0, 0)),
            (-1600, 100)
        );
        assert_eq!(
            overlay_position("top-right", area, (250, 104), (0, 0)),
            (-1600 + 1030, 100)
        );
        assert_eq!(
            overlay_position("bottom-right", area, (250, 104), (0, 0)),
            (-1600 + 1030, 100 + 616)
        );
        assert_eq!(
            overlay_position("bottom-left", area, (250, 104), (0, 0)),
            (-1600, 100),
            "no se ofrece"
        );
    }
}
