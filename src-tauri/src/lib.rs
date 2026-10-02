mod control;
mod domain;
mod identity;
mod settings;
#[cfg(windows)]
mod windows_monitor;
#[cfg(target_os = "macos")]
mod macos_monitor;
#[cfg(target_os = "macos")]
mod ddc;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

const SETTINGS_DIR_UNAVAILABLE: &str = "설정 디렉터리를 만들지 못했습니다.";

struct AppState {
    settings_path: Option<PathBuf>,
    gate: std::sync::Mutex<domain::SwitchGate>,
    notice: std::sync::Mutex<WindowNotice>,
    registered_shortcuts: std::sync::Mutex<Vec<Shortcut>>,
    tray_ready: AtomicBool,
    shortcut_capture: AtomicBool,
    saved_hotkeys: std::sync::Mutex<domain::Hotkeys>,
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
fn save_settings(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    settings: domain::Settings,
) -> Result<(), String> {
    let path = require_settings_path(&state.settings_path)?;
    settings::save_settings(path, &settings)?;
    record_shortcut_registration(&state, replace_registered_shortcuts(&app, &settings.hotkeys));
    let registered = if settings.launch_at_login {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    };
    if let Err(error) = registered {
        return Err(persist_autolaunch_failure(path, &settings, &error.to_string()));
    }
    Ok(())
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
    stored_notice(state.inner())
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SwitchFinished {
    outcomes: Option<Vec<domain::MonitorOutcome>>,
    error: Option<String>,
}

fn perform_switch(app: &tauri::AppHandle, destination: domain::Destination) {
    let state = app.state::<AppState>();
    let result = (|| {
        let mut gate = state
            .gate
            .try_lock()
            .map_err(|_| domain::PlanError::Busy.to_string())?;
        let path = require_settings_path(&state.settings_path)?;
        let settings = settings::load_settings(path)?;
        let mut control = control::open_control();
        let connected = control
            .list_monitors()?
            .into_iter()
            .map(|monitor| monitor.id)
            .collect::<Vec<_>>();
        gate.run(&mut *control, &settings, destination, &connected)
            .map_err(|err| err.to_string())
    })();
    let window = app.get_webview_window("main");
    let visible = window
        .as_ref()
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false);
    if visible {
        let payload = match result {
            Ok(outcomes) => SwitchFinished {
                outcomes: Some(outcomes),
                error: None,
            },
            Err(message) => SwitchFinished {
                outcomes: None,
                error: Some(message),
            },
        };
        let _ = app.emit("switch-finished", payload);
        return;
    }
    let body = match &result {
        Ok(outcomes) => outcomes.iter().map(outcome_line).collect::<Vec<_>>().join("\n"),
        Err(message) => message.clone(),
    };
    let _ = app
        .notification()
        .builder()
        .title("DisplayFlip")
        .body(body)
        .show();
}

fn outcome_line(outcome: &domain::MonitorOutcome) -> String {
    let delivery = match &outcome.delivery {
        domain::Delivery::Delivered => "전달됨".to_string(),
        domain::Delivery::Unconfirmed => "전달됐으나 확인 불가".to_string(),
        domain::Delivery::Failed { reason } => format!("실패: {reason}"),
    };
    format!(
        "모니터 {}: {delivery}",
        if outcome.role == 0 {
            outcome.monitor_id.clone()
        } else {
            outcome.role.to_string()
        }
    )
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn parse_shortcut(value: &str) -> Result<Shortcut, String> {
    value.parse::<Shortcut>()
        .map_err(|_| format!("단축키를 해석할 수 없습니다: {value}"))
}

#[tauri::command]
fn set_shortcut_capture(app: tauri::AppHandle, capturing: bool) -> Result<(), String> {
    update_shortcut_capture(&app, capturing)
}

fn update_shortcut_capture(app: &tauri::AppHandle, capturing: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.shortcut_capture.swap(capturing, Ordering::SeqCst) == capturing {
        return Ok(());
    }
    let hotkeys = state.saved_hotkeys.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    let result = replace_registered_shortcuts(app, &hotkeys);
    record_shortcut_registration(&state, result.clone());
    result
}

fn remember_shortcuts(state: &AppState, shortcuts: Vec<Shortcut>) {
    if let Ok(mut slot) = state.registered_shortcuts.lock() {
        *slot = shortcuts;
    }
}

fn replace_registered_shortcuts(app: &tauri::AppHandle, hotkeys: &domain::Hotkeys) -> Result<(), String> {
    let state = app.state::<AppState>();
    *state.saved_hotkeys.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = hotkeys.clone();
    let previous = state
        .registered_shortcuts
        .lock()
        .map(|shortcuts| shortcuts.clone())
        .unwrap_or_default();
    let mut kept = previous.clone();
    for shortcut in previous {
        app.global_shortcut()
            .unregister(shortcut)
            .map_err(|err| err.to_string())?;
        kept.retain(|registered| registered != &shortcut);
        remember_shortcuts(&state, kept.clone());
    }

    if state.shortcut_capture.load(Ordering::SeqCst) {
        return Ok(());
    }

    let mut registered = Vec::new();
    for (value, destination) in [
        (hotkeys.to_mac.as_str(), domain::Destination::Mac),
        (hotkeys.to_windows.as_str(), domain::Destination::Windows),
    ] {
        let shortcut = parse_shortcut(value)?;
        app.global_shortcut()
            .on_shortcut(shortcut, move |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed
                    && !app.state::<AppState>().shortcut_capture.load(Ordering::SeqCst) {
                    perform_switch(app, destination);
                }
            })
            .map_err(|err| err.to_string())?;
        registered.push(shortcut);
        remember_shortcuts(&state, registered.clone());
    }
    Ok(())
}

enum NoticeKind {
    Retained(String),
    Shortcut(String),
}

struct WindowNotice {
    parts: Vec<NoticeKind>,
}

impl WindowNotice {
    fn text(&self) -> Option<String> {
        if self.parts.is_empty() {
            None
        } else {
            Some(
                self.parts
                    .iter()
                    .map(|part| match part {
                        NoticeKind::Retained(message) | NoticeKind::Shortcut(message) => message.as_str(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        }
    }
}

fn stored_notice(state: &AppState) -> Option<String> {
    state.notice.lock().ok().and_then(|notice| notice.text())
}

fn record_shortcut_registration(state: &AppState, result: Result<(), String>) {
    let Ok(mut notice) = state.notice.lock() else {
        return;
    };
    notice
        .parts
        .retain(|part| !matches!(part, NoticeKind::Shortcut(_)));
    if let Err(error) = result {
        notice.parts.push(NoticeKind::Shortcut(format!(
            "단축키를 등록하지 못했습니다: {error}"
        )));
    }
}

fn record_app_notice(state: &AppState, message: String) {
    if let Ok(mut notice) = state.notice.lock() {
        notice.parts.push(NoticeKind::Retained(message));
    }
}

struct AutolaunchFailure {
    launch_at_login: bool,
    message: String,
}

fn autolaunch_failure(requested: bool, error: &str) -> AutolaunchFailure {
    if requested {
        AutolaunchFailure {
            launch_at_login: false,
            message: format!("로그인 자동 실행을 등록하지 못했습니다: {error}"),
        }
    } else {
        AutolaunchFailure {
            launch_at_login: true,
            message: format!("로그인 자동 실행을 해제하지 못했습니다: {error}"),
        }
    }
}

fn persist_autolaunch_failure(path: &Path, settings: &domain::Settings, error: &str) -> String {
    let failure = autolaunch_failure(settings.launch_at_login, error);
    let mut rewritten = settings.clone();
    rewritten.launch_at_login = failure.launch_at_login;
    let _ = settings::save_settings(path, &rewritten);
    failure.message
}

fn sync_startup_autolaunch(
    app: &tauri::AppHandle,
    path: &Path,
    settings: &domain::Settings,
) -> Option<String> {
    let registered = if settings.launch_at_login {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    };
    if let Err(error) = registered {
        return Some(persist_autolaunch_failure(
            path,
            settings,
            &error.to_string(),
        ));
    }
    None
}

fn tray_failure_message(error: &str) -> String {
    format!("트레이를 만들지 못했습니다: {error}")
}

fn hide_on_close(tray_exists: bool) -> bool {
    tray_exists
}

fn tray_is_ready<R: tauri::Runtime>(window: &tauri::Window<R>) -> bool {
    window
        .try_state::<AppState>()
        .is_some_and(|state| state.tray_ready.load(Ordering::Acquire))
}

fn install_tray(app: &tauri::App) -> tauri::Result<()> {
    let to_mac = tauri::menu::MenuItem::with_id(app, "to-mac", "맥으로", true, None::<&str>)?;
    let to_windows = tauri::menu::MenuItem::with_id(app, "to-windows", "Windows로", true, None::<&str>)?;
    let open = tauri::menu::MenuItem::with_id(app, "open", "열기", true, None::<&str>)?;
    let quit = tauri::menu::MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let menu = tauri::menu::Menu::with_items(app, &[&to_mac, &to_windows, &open, &quit])?;
    let icon = app.default_window_icon().cloned();
    let mut tray = tauri::tray::TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "to-mac" => perform_switch(app, domain::Destination::Mac),
            "to-windows" => perform_switch(app, domain::Destination::Windows),
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = icon {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    app.state::<AppState>().tray_ready.store(true, Ordering::Release);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Focused(false) | tauri::WindowEvent::CloseRequested { .. }) {
                if let Some(state) = window.try_state::<AppState>() {
                    if state.shortcut_capture.load(Ordering::SeqCst) {
                        let _ = update_shortcut_capture(window.app_handle(), false);
                    }
                }
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if hide_on_close(tray_is_ready(window)) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let (settings_path, configured) = startup_settings(app);
            let autolaunch_notice = if let Some(path) = &settings_path {
                settings::load_settings(path)
                    .ok()
                    .and_then(|settings| sync_startup_autolaunch(app.handle(), path, &settings))
            } else {
                None
            };
            app.manage(AppState {
                settings_path: settings_path.clone(),
                gate: std::sync::Mutex::new(domain::SwitchGate::default()),
                notice: std::sync::Mutex::new(WindowNotice { parts: Vec::new() }),
                registered_shortcuts: std::sync::Mutex::new(Vec::new()),
                tray_ready: AtomicBool::new(false),
                shortcut_capture: AtomicBool::new(false),
                saved_hotkeys: std::sync::Mutex::new(settings::default_settings().hotkeys),
            });
            if let Some(message) = autolaunch_notice {
                record_app_notice(app.state::<AppState>().inner(), message);
            }
            if let Some(path) = &settings_path {
                if let Ok(settings) = settings::load_settings(path) {
                    record_shortcut_registration(
                        app.state::<AppState>().inner(),
                        replace_registered_shortcuts(app.handle(), &settings.hotkeys),
                    );
                }
            }
            if let Err(error) = install_tray(app) {
                record_app_notice(
                    app.state::<AppState>().inner(),
                    tray_failure_message(&error.to_string()),
                );
            }
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
            shortcut_status,
            set_shortcut_capture
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

    #[test]
    fn default_shortcuts_parse() {
        assert!(parse_shortcut("Ctrl+Alt+M").is_ok());
        assert!(parse_shortcut("Ctrl+Alt+W").is_ok());
        assert!(parse_shortcut("Nope").unwrap_err().contains("단축키를 해석할 수 없습니다"));
    }

    #[test]
    fn captured_function_keys_and_multiple_modifiers_parse() {
        assert!(parse_shortcut("Ctrl+Alt+Shift+Super+M").is_ok());
        assert!(parse_shortcut("Shift+F12").is_ok());
        assert!(parse_shortcut("Ctrl+ArrowUp").is_ok());
        assert!(parse_shortcut("Ctrl+Shift+Equal").is_ok());
        assert!(parse_shortcut("Ctrl+M+W").is_err());
    }

    #[test]
    fn autolaunch_failure_matches_os_registration() {
        let enabled = autolaunch_failure(true, "denied");
        assert!(!enabled.launch_at_login);
        assert_eq!(enabled.message, "로그인 자동 실행을 등록하지 못했습니다: denied");

        let disabled = autolaunch_failure(false, "busy");
        assert!(disabled.launch_at_login);
        assert_eq!(disabled.message, "로그인 자동 실행을 해제하지 못했습니다: busy");
    }

    #[test]
    fn startup_disable_failure_rewrites_launch_at_login_to_true() {
        let path = std::env::temp_dir().join(format!(
            "displayflip-autolaunch-disable-{}-settings.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let mut settings = settings::default_settings();
        settings.launch_at_login = false;
        settings::save_settings(&path, &settings).unwrap();

        let message = persist_autolaunch_failure(&path, &settings, "busy");
        assert_eq!(message, "로그인 자동 실행을 해제하지 못했습니다: busy");
        assert!(settings::load_settings(&path).unwrap().launch_at_login);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn close_hides_only_when_a_tray_exists() {
        assert!(hide_on_close(true));
        assert!(!hide_on_close(false));
    }

    #[test]
    fn tray_build_failure_is_recorded_for_the_window() {
        assert_eq!(tray_failure_message("missing icon"), "트레이를 만들지 못했습니다: missing icon");
    }

    #[test]
    fn successful_shortcut_registration_keeps_autolaunch_notice() {
        let state = AppState {
            settings_path: None,
            gate: std::sync::Mutex::new(domain::SwitchGate::default()),
            notice: std::sync::Mutex::new(WindowNotice { parts: Vec::new() }),
            registered_shortcuts: std::sync::Mutex::new(Vec::new()),
            tray_ready: AtomicBool::new(false),
            shortcut_capture: AtomicBool::new(false),
            saved_hotkeys: std::sync::Mutex::new(settings::default_settings().hotkeys),
        };
        let message = autolaunch_failure(true, "denied").message;
        record_app_notice(&state, message.clone());
        record_shortcut_registration(&state, Err("taken".to_string()));
        let during = stored_notice(&state).expect("autolaunch and shortcut notices");
        assert!(during.contains(&message));
        assert!(during.contains("단축키를 등록하지 못했습니다: taken"));

        record_shortcut_registration(&state, Ok(()));
        assert_eq!(stored_notice(&state).as_deref(), Some(message.as_str()));
    }
}
