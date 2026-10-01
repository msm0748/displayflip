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
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
    }
    let text = serde_json::to_string_pretty(settings).map_err(|err| err.to_string())?;
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, text).map_err(|err| err.to_string())?;

    let bak_path = path.with_extension("json.bak");
    let replace_result = if !path.exists() {
        std::fs::rename(&temp_path, path).map_err(|err| err.to_string())
    } else {
        std::fs::rename(path, &bak_path).map_err(|err| err.to_string())?;
        match std::fs::rename(&temp_path, path) {
            Ok(()) => {
                let _ = std::fs::remove_file(&bak_path);
                Ok(())
            }
            Err(err) => {
                std::fs::rename(&bak_path, path).map_err(|restore| {
                    format!("{}; failed to restore backup: {}", err, restore)
                })?;
                Err(err.to_string())
            }
        }
    };

    if replace_result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    replace_result
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
        settings.hotkeys.to_windows = "Ctrl+Alt+2".into();
        save_settings(&path, &settings).unwrap();
        assert_eq!(load_settings(&path).unwrap(), settings);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_rejects_the_same_monitor_on_both_roles() {
        let path = std::env::temp_dir().join(format!("displayflip-same-{}-settings.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut settings = default_settings();
        settings.monitors.monitor1 = Some(MonitorRoleConfig { id: "AAA".into(), hdmi_code: None, dp_code: None });
        settings.monitors.monitor2 = Some(MonitorRoleConfig { id: "AAA".into(), hdmi_code: None, dp_code: None });
        let err = save_settings(&path, &settings).unwrap_err();
        assert!(err.contains("서로 다른 모니터"));
        assert!(!path.exists());
    }
}
