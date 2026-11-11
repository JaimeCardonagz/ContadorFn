// El detector aún no está conectado a ninguna fuente de señales (captura,
// audio, entrada), así que por ahora solo lo usan sus pruebas.
#[allow(dead_code)]
mod detect;
mod store;
mod tracker;

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use store::{ChestType, EndReason, Store};
use tauri::{AppHandle, Emitter, Manager};
use tracker::{DEFAULT_INACTIVITY_MS, Stats, Tracker};

struct AppState(Mutex<Tracker>);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Aplica un cambio al estado, avisa a la interfaz y devuelve los contadores.
fn update(
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
fn adjust_total(app: AppHandle, chest: Option<ChestType>, delta: i64) -> Result<Stats, String> {
    let mut accepted = false;
    let stats = update(&app, |t, now| {
        accepted = t.adjust_total(now, chest, delta)?;
        Ok(())
    })?;
    if accepted {
        Ok(stats)
    } else {
        Err("El ajuste dejaría ese contador por debajo de cero.".into())
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
        Err("El ajuste dejaría las partidas por debajo de cero.".into())
    }
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
        "boss" => chest(ChestType::Boss),
        "match_start" => update(&app, |t, now| t.match_started(now)),
        "eliminated" => update(&app, |t, now| t.match_ended(now, EndReason::Eliminated)),
        "victory" => update(&app, |t, now| t.match_ended(now, EndReason::Victory)),
        "lobby" => update(&app, |t, now| t.match_ended(now, EndReason::Lobby)),
        other => Err(format!("evento desconocido: {other}")),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let store = Store::open(&dir.join("chests.db"))?;
            let tracker = Tracker::new(store, now_ms(), DEFAULT_INACTIVITY_MS)?;
            app.manage(AppState(Mutex::new(tracker)));

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
        .invoke_handler(tauri::generate_handler![
            get_stats,
            start_session,
            stop_session,
            new_match,
            adjust_total,
            adjust_matches,
            is_debug,
            debug_simulate,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
