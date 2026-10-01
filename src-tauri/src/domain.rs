use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Destination {
    Mac,
    Windows,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorRoleConfig {
    pub id: String,
    pub hdmi_code: Option<u8>,
    pub dp_code: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorRoles {
    #[serde(rename = "1")]
    pub monitor1: Option<MonitorRoleConfig>,
    #[serde(rename = "2")]
    pub monitor2: Option<MonitorRoleConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hotkeys {
    pub to_mac: String,
    pub to_windows: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub monitors: MonitorRoles,
    pub hotkeys: Hotkeys,
    pub launch_at_login: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedCommand {
    pub role: u8,
    pub monitor_id: String,
    pub code: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MissingItem {
    Monitor1,
    Monitor1Hdmi,
    Monitor1Dp,
    Monitor2,
    Monitor2Hdmi,
    Monitor2Dp,
    Monitor1Disconnected,
    Monitor2Disconnected,
}

impl MissingItem {
    fn label(self) -> &'static str {
        match self {
            MissingItem::Monitor1 => "모니터 1",
            MissingItem::Monitor1Hdmi => "모니터 1 HDMI 번호",
            MissingItem::Monitor1Dp => "모니터 1 DP 번호",
            MissingItem::Monitor2 => "모니터 2",
            MissingItem::Monitor2Hdmi => "모니터 2 HDMI 번호",
            MissingItem::Monitor2Dp => "모니터 2 DP 번호",
            MissingItem::Monitor1Disconnected => "모니터 1 연결",
            MissingItem::Monitor2Disconnected => "모니터 2 연결",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanError {
    Incomplete { missing: Vec<MissingItem> },
    SameMonitor,
    Busy,
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::Incomplete { missing } => {
                let labels: Vec<&str> = missing.iter().cloned().map(MissingItem::label).collect();
                write!(f, "설정이 완료되지 않았습니다: {}", labels.join(", "))
            }
            PlanError::SameMonitor => write!(f, "모니터 1과 모니터 2는 서로 다른 모니터여야 합니다."),
            PlanError::Busy => write!(f, "이미 전환 중입니다."),
        }
    }
}

pub fn is_configured(settings: &Settings) -> bool {
    let Some(first) = &settings.monitors.monitor1 else {
        return false;
    };
    let Some(second) = &settings.monitors.monitor2 else {
        return false;
    };
    !first.id.is_empty()
        && !second.id.is_empty()
        && first.id != second.id
        && first.hdmi_code.is_some()
        && first.dp_code.is_some()
        && second.hdmi_code.is_some()
        && second.dp_code.is_some()
}

pub fn plan_switch(
    settings: &Settings,
    destination: Destination,
    connected_ids: &[String],
) -> Result<Vec<PlannedCommand>, PlanError> {
    let Some(first) = settings.monitors.monitor1.clone() else {
        return Err(PlanError::Incomplete { missing: vec![MissingItem::Monitor1] });
    };
    let Some(second) = settings.monitors.monitor2.clone() else {
        return Err(PlanError::Incomplete { missing: vec![MissingItem::Monitor2] });
    };
    if first.id == second.id {
        return Err(PlanError::SameMonitor);
    }

    let mut missing = Vec::new();
    let first_code = code_for(&first, destination, 1, &mut missing);
    let second_code = code_for(&second, destination, 2, &mut missing);
    if !connected_ids.iter().any(|id| id == &first.id) {
        missing.push(MissingItem::Monitor1Disconnected);
    }
    if !connected_ids.iter().any(|id| id == &second.id) {
        missing.push(MissingItem::Monitor2Disconnected);
    }
    if !missing.is_empty() {
        return Err(PlanError::Incomplete { missing });
    }

    Ok(vec![
        PlannedCommand { role: 1, monitor_id: first.id, code: first_code.unwrap() },
        PlannedCommand { role: 2, monitor_id: second.id, code: second_code.unwrap() },
    ])
}

fn code_for(
    role: &MonitorRoleConfig,
    destination: Destination,
    index: u8,
    missing: &mut Vec<MissingItem>,
) -> Option<u8> {
    let (hdmi_missing, dp_missing) = match index {
        1 => (MissingItem::Monitor1Hdmi, MissingItem::Monitor1Dp),
        _ => (MissingItem::Monitor2Hdmi, MissingItem::Monitor2Dp),
    };
    let code = match destination {
        Destination::Mac if index == 1 => role.hdmi_code,
        Destination::Mac => role.dp_code,
        Destination::Windows if index == 1 => role.dp_code,
        Destination::Windows => role.hdmi_code,
    };
    if code.is_none() {
        missing.push(if matches!(destination, Destination::Mac) && index == 1 || matches!(destination, Destination::Windows) && index == 2 {
            hdmi_missing
        } else {
            dp_missing
        });
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> Settings {
        Settings {
            monitors: MonitorRoles {
                monitor1: Some(MonitorRoleConfig {
                    id: "mon-1".into(),
                    hdmi_code: Some(17),
                    dp_code: Some(15),
                }),
                monitor2: Some(MonitorRoleConfig {
                    id: "mon-2".into(),
                    hdmi_code: Some(17),
                    dp_code: Some(15),
                }),
            },
            hotkeys: Hotkeys {
                to_mac: "Ctrl+Alt+M".into(),
                to_windows: "Ctrl+Alt+W".into(),
            },
            launch_at_login: true,
        }
    }

    #[test]
    fn mac_uses_hdmi_then_dp() {
        let plan = plan_switch(&ready(), Destination::Mac, &["mon-1".into(), "mon-2".into()]).unwrap();
        assert_eq!(plan[0], PlannedCommand { role: 1, monitor_id: "mon-1".into(), code: 17 });
        assert_eq!(plan[1], PlannedCommand { role: 2, monitor_id: "mon-2".into(), code: 15 });
    }

    #[test]
    fn windows_uses_dp_then_hdmi() {
        let plan = plan_switch(&ready(), Destination::Windows, &["mon-1".into(), "mon-2".into()]).unwrap();
        assert_eq!(plan[0].code, 15);
        assert_eq!(plan[1].code, 17);
    }

    #[test]
    fn missing_hdmi_sends_nothing() {
        let mut settings = ready();
        settings.monitors.monitor1.as_mut().unwrap().hdmi_code = None;
        let err = plan_switch(&settings, Destination::Mac, &["mon-1".into(), "mon-2".into()]).unwrap_err();
        assert!(err.to_string().contains("모니터 1 HDMI 번호"));
    }

    #[test]
    fn disconnected_monitor_is_named() {
        let err = plan_switch(&ready(), Destination::Mac, &["mon-1".into()]).unwrap_err();
        assert!(err.to_string().contains("모니터 2 연결"));
    }

    #[test]
    fn same_monitor_is_rejected() {
        let mut settings = ready();
        settings.monitors.monitor2.as_mut().unwrap().id = "mon-1".into();
        let err = plan_switch(&settings, Destination::Mac, &["mon-1".into()]).unwrap_err();
        assert!(matches!(err, PlanError::SameMonitor));
    }

    #[test]
    fn configured_requires_two_distinct_roles_and_four_codes() {
        assert!(is_configured(&ready()));
        let mut missing_dp = ready();
        missing_dp.monitors.monitor1.as_mut().unwrap().dp_code = None;
        assert!(!is_configured(&missing_dp));
        let mut same = ready();
        same.monitors.monitor2.as_mut().unwrap().id = "mon-1".into();
        assert!(!is_configured(&same));
    }
}
