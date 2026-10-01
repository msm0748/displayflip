mod control;
mod domain;
mod identity;
mod settings;
#[cfg(windows)]
mod windows_monitor;
#[cfg(target_os = "macos")]
mod macos_monitor;

use std::path::{Path, PathBuf};

use tauri::Manager;

const SETTINGS_DIR_UNAVAILABLE: &str = "설정 디렉터리를 만들지 못했습니다.";

struct AppState {
    settings_path: Option<PathBuf>,
    gate: std::sync::Mutex<domain::SwitchGate>,
    shortcut_error: std::sync::Mutex<Option<String>>,
}

fn require_settings_path(path: &Option<PathBuf>) -> Result<&Path, String> {
    path.as_deref()
        .ok_or_else(|| SETTINGS_DIR_UNAVAILABLE.to_string())
}

/// Maps a resolved app config directory to the settings file path. None when the directory is unknown.
pub(crate) fn map_config_dir_to_settings_path(config_dir: Option<&Path>) -> Option<PathBuf> {
    config_dir.map(|dir| dir.join("settings.json"))
}

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> Result<domain::Settings, String> {
    match &state.settings_path {
        Some(path) => settings::load_settings(path),
        None => Ok(settings::default_settings()),
    }
}

#[tauri::command]
fn save_settings(state: tauri::State<AppState>, settings: domain::Settings) -> Result<(), String> {
    let path = require_settings_path(&state.settings_path)?;
    settings::save_settings(path, &settings)
}

#[tauri::command]
fn list_monitors() -> Result<Vec<domain::DetectedMonitor>, String> {
    control::open_control().list_monitors()
}

#[tauri::command]
fn read_input(id: String) -> Result<u8, String> {
    control::open_control().get_input(&id)
}

#[tauri::command]
fn trial_set_input(state: tauri::State<AppState>, id: String, code: u8) -> Result<domain::MonitorOutcome, String> {
    let mut gate = state.gate.try_lock().map_err(|_| domain::PlanError::Busy.to_string())?;
    let mut control = control::open_control();
    gate.trial(&mut *control, &id, code).map_err(|err| err.to_string())
}

#[tauri::command]
fn switch_to(state: tauri::State<AppState>, destination: domain::Destination) -> Result<Vec<domain::MonitorOutcome>, String> {
    let mut gate = state.gate.try_lock().map_err(|_| domain::PlanError::Busy.to_string())?;
    let path = require_settings_path(&state.settings_path)?;
    let settings = settings::load_settings(path)?;
    let mut control = control::open_control();
    let connected = control.list_monitors()?.into_iter().map(|monitor| monitor.id).collect::<Vec<_>>();
    gate.run(&mut *control, &settings, destination, &connected).map_err(|err| err.to_string())
}

#[tauri::command]
fn shortcut_status(state: tauri::State<AppState>) -> Option<String> {
    state.shortcut_error.lock().ok().and_then(|error| error.clone())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let (settings_path, configured) = startup_settings(app);
            app.manage(AppState {
                settings_path,
                gate: std::sync::Mutex::new(domain::SwitchGate::default()),
                shortcut_error: std::sync::Mutex::new(None),
            });
            let autostart = std::env::args().any(|arg| arg == "--autostart");
            if domain::should_show_window(autostart, configured) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            list_monitors,
            read_input,
            trial_set_input,
            switch_to,
            shortcut_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn startup_settings(app: &tauri::App) -> (Option<PathBuf>, bool) {
    // Directory setup is best-effort. Failure still starts the process and shows the window.
    let Ok(dir) = app.path().app_config_dir() else {
        return (None, false);
    };
    let Some(settings_path) = map_config_dir_to_settings_path(Some(dir.as_path())) else {
        return (None, false);
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return (Some(settings_path), false);
    }
    let configured = settings::load_settings(&settings_path)
        .map(|settings| domain::is_configured(&settings))
        .unwrap_or(false);
    (Some(settings_path), configured)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_config_dir_yields_no_settings_path() {
        assert!(map_config_dir_to_settings_path(None).is_none());
    }

    #[test]
    fn config_dir_maps_to_settings_json() {
        let dir = PathBuf::from(r"C:\app\config");
        assert_eq!(
            map_config_dir_to_settings_path(Some(dir.as_path())),
            Some(dir.join("settings.json"))
        );
    }

    #[test]
    fn require_settings_path_rejects_missing_directory() {
        let err = require_settings_path(&None).unwrap_err();
        assert_eq!(err, SETTINGS_DIR_UNAVAILABLE);
    }
}
