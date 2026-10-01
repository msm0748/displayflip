use std::path::Path;

use crate::domain::{Hotkeys, MonitorRoles, PlanError, Settings};

pub fn default_settings() -> Settings {
    Settings {
        monitors: MonitorRoles { monitor1: None, monitor2: None },
        hotkeys: Hotkeys {
            to_mac: "Ctrl+Alt+M".into(),
            to_windows: "Ctrl+Alt+W".into(),
        },
        launch_at_login: true,
    }
}

pub fn load_settings(path: &Path) -> Result<Settings, String> {
    if !path.exists() {
        return Ok(default_settings());
    }
    let text = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    serde_json::from_str(&text).map_err(|err| err.to_string())
}

pub fn save_settings(path: &Path, settings: &Settings) -> Result<(), String> {
    if let (Some(first), Some(second)) = (&settings.monitors.monitor1, &settings.monitors.monitor2) {
        if first.id == second.id {
            return Err(PlanError::SameMonitor.to_string());
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|err| err.to_string())?;
    std::fs::write(path, text).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{MonitorRoleConfig, MonitorRoles};

    #[test]
    fn missing_file_uses_defaults() {
        let path = std::env::temp_dir().join(format!("displayflip-missing-{}-settings.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let settings = load_settings(&path).unwrap();
        assert_eq!(settings.hotkeys.to_mac, "Ctrl+Alt+M");
        assert_eq!(settings.hotkeys.to_windows, "Ctrl+Alt+W");
        assert!(settings.launch_at_login);
        assert!(settings.monitors.monitor1.is_none());
    }

    #[test]
    fn round_trip_keeps_identity_codes_and_hotkeys() {
        let path = std::env::temp_dir().join(format!("displayflip-round-{}-settings.json", std::process::id()));
        let mut settings = default_settings();
        settings.monitors = MonitorRoles {
            monitor1: Some(MonitorRoleConfig { id: "AAA".into(), hdmi_code: Some(17), dp_code: Some(15) }),
            monitor2: Some(MonitorRoleConfig { id: "BBB".into(), hdmi_code: Some(18), dp_code: Some(16) }),
        };
        settings.hotkeys.to_mac = "Ctrl+Alt+1".into();
        settings.launch_at_login = false;
        save_settings(&path, &settings).unwrap();
        assert_eq!(load_settings(&path).unwrap(), settings);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_rejects_the_same_monitor_on_both_roles() {
        let path = std::env::temp_dir().join(format!("displayflip-same-{}-settings.json", std::process::id()));
        let mut settings = default_settings();
        settings.monitors.monitor1 = Some(MonitorRoleConfig { id: "AAA".into(), hdmi_code: None, dp_code: None });
        settings.monitors.monitor2 = Some(MonitorRoleConfig { id: "AAA".into(), hdmi_code: None, dp_code: None });
        let err = save_settings(&path, &settings).unwrap_err();
        assert!(err.contains("서로 다른 모니터"));
    }
}
