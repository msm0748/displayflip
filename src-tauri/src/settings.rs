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

    match replace_settings_file(&temp_path, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = std::fs::remove_file(&temp_path);
            Err(err)
        }
    }
}

#[cfg(windows)]
fn replace_settings_file(temp_path: &Path, dest_path: &Path) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(
            lpExistingFileName: *const u16,
            lpNewFileName: *const u16,
            dwFlags: u32,
        ) -> i32;
    }

    fn to_wide(path: &Path) -> Vec<u16> {
        OsStr::new(path).encode_wide().chain(Some(0)).collect()
    }

    let from = to_wide(temp_path);
    let to = to_wide(dest_path);
    let ok = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_settings_file(temp_path: &Path, dest_path: &Path) -> Result<(), String> {
    std::fs::rename(temp_path, dest_path).map_err(|err| err.to_string())
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
    fn save_succeeds_with_preexisting_bak_file() {
        let path = std::env::temp_dir().join(format!(
            "displayflip-bak-{}-settings.json",
            std::process::id()
        ));
        let bak_path = path.with_extension("json.bak");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&bak_path);
        std::fs::write(&bak_path, b"junk leftover backup").unwrap();

        let mut settings = default_settings();
        settings.hotkeys.to_mac = "Ctrl+Alt+Bak1".into();
        save_settings(&path, &settings).unwrap();
        assert_eq!(load_settings(&path).unwrap(), settings);

        settings.hotkeys.to_mac = "Ctrl+Alt+Bak2".into();
        save_settings(&path, &settings).unwrap();
        assert_eq!(load_settings(&path).unwrap(), settings);
        assert!(!path.with_extension("json.tmp").exists());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&bak_path);
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
