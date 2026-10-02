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
    InvalidMonitor,
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
            PlanError::InvalidMonitor => write!(f, "모니터 번호는 1 또는 2여야 합니다."),
            PlanError::Busy => write!(f, "이미 전환 중입니다."),
        }
    }
}

pub fn should_show_window(autostart: bool, configured: bool) -> bool {
    !(autostart && configured)
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
    let mut missing = Vec::new();
    if first.id.is_empty() {
        missing.push(MissingItem::Monitor1);
    }
    if second.id.is_empty() {
        missing.push(MissingItem::Monitor2);
    }
    let first_code = code_for(&first, destination, 1, &mut missing);
    let second_code = code_for(&second, destination, 2, &mut missing);
    if !first.id.is_empty() && !connected_ids.iter().any(|id| id == &first.id) {
        missing.push(MissingItem::Monitor1Disconnected);
    }
    if !second.id.is_empty() && !connected_ids.iter().any(|id| id == &second.id) {
        missing.push(MissingItem::Monitor2Disconnected);
    }
    if !missing.is_empty() {
        return Err(PlanError::Incomplete { missing });
    }
    if first.id == second.id {
        return Err(PlanError::SameMonitor);
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedMonitor {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum Delivery {
    Delivered,
    Unconfirmed,
    Failed { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorOutcome {
    pub role: u8,
    pub monitor_id: String,
    pub delivery: Delivery,
}

pub trait MonitorControl {
    fn list_monitors(&mut self) -> Result<Vec<DetectedMonitor>, String>;
    fn set_input(&mut self, id: &str, code: u8) -> Result<(), String>;
    fn get_input(&mut self, id: &str) -> Result<u8, String>;
}

#[derive(Default)]
pub struct SwitchGate {
    pub busy: bool,
}

impl SwitchGate {
    pub fn run_monitor<C: MonitorControl + ?Sized>(
        &mut self,
        control: &mut C,
        settings: &Settings,
        destination: Destination,
        index: u8,
        connected_ids: &[String],
    ) -> Result<MonitorOutcome, PlanError> {
        if self.busy {
            return Err(PlanError::Busy);
        }
        let (selected, monitor_missing, disconnected) = match index {
            1 => (&settings.monitors.monitor1, MissingItem::Monitor1, MissingItem::Monitor1Disconnected),
            2 => (&settings.monitors.monitor2, MissingItem::Monitor2, MissingItem::Monitor2Disconnected),
            _ => return Err(PlanError::InvalidMonitor),
        };
        let selected = selected.as_ref().filter(|monitor| !monitor.id.is_empty())
            .ok_or_else(|| PlanError::Incomplete { missing: vec![monitor_missing] })?;
        let mut missing = Vec::new();
        let code = code_for(selected, destination, index, &mut missing);
        if !connected_ids.contains(&selected.id) {
            missing.push(disconnected);
        }
        if !missing.is_empty() {
            return Err(PlanError::Incomplete { missing });
        }
        self.busy = true;
        let guard = BusyGuard(&mut self.busy);
        let outcome = execute(control, index, &selected.id, code.unwrap());
        drop(guard);
        Ok(outcome)
    }

    pub fn run<C: MonitorControl + ?Sized>(
        &mut self,
        control: &mut C,
        settings: &Settings,
        destination: Destination,
        connected_ids: &[String],
    ) -> Result<Vec<MonitorOutcome>, PlanError> {
        if self.busy {
            return Err(PlanError::Busy);
        }
        let planned = plan_switch(settings, destination, connected_ids)?;
        self.busy = true;
        let gate = BusyGuard(&mut self.busy);
        let mut outcomes = Vec::new();
        for command in planned {
            outcomes.push(execute(control, command.role, &command.monitor_id, command.code));
        }
        drop(gate);
        Ok(outcomes)
    }

    pub fn trial<C: MonitorControl + ?Sized>(
        &mut self,
        control: &mut C,
        id: &str,
        code: u8,
    ) -> Result<MonitorOutcome, PlanError> {
        if self.busy {
            return Err(PlanError::Busy);
        }
        self.busy = true;
        let gate = BusyGuard(&mut self.busy);
        let outcome = execute(control, 0, id, code);
        drop(gate);
        Ok(outcome)
    }
}

struct BusyGuard<'a>(&'a mut bool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        *self.0 = false;
    }
}

fn execute<C: MonitorControl + ?Sized>(control: &mut C, role: u8, id: &str, code: u8) -> MonitorOutcome {
    let delivery = match control.set_input(id, code) {
        Err(reason) => Delivery::Failed { reason },
        Ok(()) => match control.get_input(id) {
            Err(_) => Delivery::Unconfirmed,
            Ok(actual) if actual == code => Delivery::Delivered,
            Ok(actual) => Delivery::Failed {
                reason: format!("보낸 번호 {code}와 현재 번호 {actual}이 다릅니다."),
            },
        },
    };
    MonitorOutcome { role, monitor_id: id.into(), delivery }
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
    fn empty_role_ids_are_incomplete_not_same_monitor() {
        let mut settings = ready();
        settings.monitors.monitor1.as_mut().unwrap().id.clear();
        settings.monitors.monitor2.as_mut().unwrap().id.clear();
        let err =
            plan_switch(&settings, Destination::Mac, &["mon-1".into(), "mon-2".into()]).unwrap_err();
        assert!(matches!(err, PlanError::Incomplete { .. }));
        assert!(!matches!(err, PlanError::SameMonitor));
        let PlanError::Incomplete { missing } = err else {
            unreachable!()
        };
        assert_eq!(
            missing,
            vec![MissingItem::Monitor1, MissingItem::Monitor2]
        );
        let message = PlanError::Incomplete { missing: missing.clone() }.to_string();
        assert!(message.contains("모니터 1"));
        assert!(message.contains("모니터 2"));
        assert!(!message.contains("연결"));
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

    struct Fake {
        fail_set: bool,
        read: ReadMode,
        sets: Vec<(String, u8)>,
    }

    enum ReadMode {
        Match,
        Fail,
        Mismatch,
    }

    impl MonitorControl for Fake {
        fn list_monitors(&mut self) -> Result<Vec<DetectedMonitor>, String> {
            Ok(vec![
                DetectedMonitor { id: "mon-1".into(), name: "하나".into() },
                DetectedMonitor { id: "mon-2".into(), name: "둘".into() },
            ])
        }
        fn set_input(&mut self, id: &str, code: u8) -> Result<(), String> {
            self.sets.push((id.into(), code));
            if self.fail_set && id == "mon-2" {
                Err("거부".into())
            } else {
                Ok(())
            }
        }
        fn get_input(&mut self, _id: &str) -> Result<u8, String> {
            match self.read {
                ReadMode::Match => Ok(17),
                ReadMode::Fail => Err("읽기 실패".into()),
                ReadMode::Mismatch => Ok(99),
            }
        }
    }

    #[test]
    fn delivered_when_read_matches_sent_code() {
        let mut gate = SwitchGate::default();
        let mut fake = Fake { fail_set: false, read: ReadMode::Match, sets: Vec::new() };
        let mut settings = ready();
        settings.monitors.monitor1.as_mut().unwrap().hdmi_code = Some(17);
        settings.monitors.monitor2.as_mut().unwrap().dp_code = Some(17);
        let outcomes = gate.run(&mut fake, &settings, Destination::Mac, &["mon-1".into(), "mon-2".into()]).unwrap();
        assert!(matches!(outcomes[0].delivery, Delivery::Delivered));
        assert!(matches!(outcomes[1].delivery, Delivery::Delivered));
    }

    #[test]
    fn unconfirmed_when_read_fails() {
        let mut gate = SwitchGate::default();
        let mut fake = Fake { fail_set: false, read: ReadMode::Fail, sets: Vec::new() };
        let outcomes = gate.run(&mut fake, &ready(), Destination::Mac, &["mon-1".into(), "mon-2".into()]).unwrap();
        assert!(matches!(outcomes[0].delivery, Delivery::Unconfirmed));
    }

    #[test]
    fn failed_when_read_differs() {
        let mut gate = SwitchGate::default();
        let mut fake = Fake { fail_set: false, read: ReadMode::Mismatch, sets: Vec::new() };
        let outcomes = gate.run(&mut fake, &ready(), Destination::Mac, &["mon-1".into(), "mon-2".into()]).unwrap();
        match &outcomes[0].delivery {
            Delivery::Failed { reason } => assert!(reason.contains("보낸 번호")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn set_failure_does_not_read_or_roll_back() {
        let mut gate = SwitchGate::default();
        let mut fake = Fake { fail_set: true, read: ReadMode::Match, sets: Vec::new() };
        let outcomes = gate.run(&mut fake, &ready(), Destination::Mac, &["mon-1".into(), "mon-2".into()]).unwrap();
        assert!(matches!(outcomes[0].delivery, Delivery::Delivered));
        assert!(matches!(outcomes[1].delivery, Delivery::Failed { .. }));
        assert_eq!(fake.sets.len(), 2);
    }

    #[test]
    fn second_switch_while_busy_is_rejected() {
        let mut gate = SwitchGate::default();
        gate.busy = true;
        let mut fake = Fake { fail_set: false, read: ReadMode::Match, sets: Vec::new() };
        let err = gate.run(&mut fake, &ready(), Destination::Mac, &["mon-1".into(), "mon-2".into()]).unwrap_err();
        assert!(matches!(err, PlanError::Busy));
        assert!(fake.sets.is_empty());
    }

    #[test]
    fn trial_does_not_require_full_configuration() {
        let mut gate = SwitchGate::default();
        let mut fake = Fake { fail_set: false, read: ReadMode::Match, sets: Vec::new() };
        let outcome = gate.trial(&mut fake, "mon-1", 17).unwrap();
        assert_eq!(outcome.role, 0);
        assert!(matches!(outcome.delivery, Delivery::Delivered));
    }

    #[test]
    fn individual_switch_only_writes_selected_monitor_without_other_configuration() {
        let mut settings = ready();
        settings.monitors.monitor1 = None;
        settings.monitors.monitor2.as_mut().unwrap().dp_code = None;
        let mut gate = SwitchGate::default();
        let mut fake = Fake { fail_set: false, read: ReadMode::Fail, sets: Vec::new() };
        let outcome = gate.run_monitor(&mut fake, &settings, Destination::Windows, 2, &["mon-2".into()]).unwrap();
        assert_eq!(fake.sets, vec![("mon-2".into(), 17)]);
        assert_eq!(outcome.role, 2);
        assert!(matches!(outcome.delivery, Delivery::Unconfirmed));
        assert!(!gate.busy);
    }

    #[test]
    fn individual_switch_uses_each_monitors_destination_port() {
        for (index, destination, expected) in [(1, Destination::Mac, 17), (1, Destination::Windows, 15), (2, Destination::Mac, 15), (2, Destination::Windows, 17)] {
            let mut gate = SwitchGate::default();
            let mut fake = Fake { fail_set: false, read: ReadMode::Fail, sets: Vec::new() };
            gate.run_monitor(&mut fake, &ready(), destination, index, &["mon-1".into(), "mon-2".into()]).unwrap();
            assert_eq!(fake.sets, vec![(format!("mon-{index}"), expected)]);
        }
    }

    #[test]
    fn individual_switch_rejects_missing_disconnected_and_invalid_targets_before_writing() {
        let mut no_code = ready();
        no_code.monitors.monitor1.as_mut().unwrap().hdmi_code = None;
        let mut no_monitor = ready();
        no_monitor.monitors.monitor1 = None;
        for (settings, index, connected) in [(no_code, 1, vec!["mon-1".into()]), (no_monitor, 1, vec!["mon-1".into()]), (ready(), 1, vec!["mon-2".into()]), (ready(), 3, vec!["mon-1".into()])] {
            let mut gate = SwitchGate::default();
            let mut fake = Fake { fail_set: false, read: ReadMode::Match, sets: Vec::new() };
            assert!(gate.run_monitor(&mut fake, &settings, Destination::Mac, index, &connected).is_err());
            assert!(fake.sets.is_empty());
        }
    }

    #[test]
    fn individual_switch_shares_busy_guard_with_all_monitor_switches() {
        let mut gate = SwitchGate { busy: true };
        let mut fake = Fake { fail_set: false, read: ReadMode::Match, sets: Vec::new() };
        assert!(matches!(gate.run_monitor(&mut fake, &ready(), Destination::Mac, 1, &["mon-1".into()]), Err(PlanError::Busy)));
        assert!(fake.sets.is_empty());
    }

    #[test]
    fn autostart_with_complete_settings_stays_hidden() {
        assert!(!should_show_window(true, true));
        assert!(should_show_window(true, false));
        assert!(should_show_window(false, true));
    }
}
