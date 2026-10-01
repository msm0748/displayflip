# DisplayFlip Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 버튼, 전역 단축키, 트레이 메뉴로 모니터 두 대를 맥 또는 Windows 입력으로 전환하는 Tauri 2 앱을 만든다.

**Architecture:** React 화면은 Tauri 명령만 호출한다. Rust 도메인이 목적지별 입력 번호를 계산하고, 설정 파일이 이 PC의 모니터 역할과 번호를 저장한다. 제어 모듈만 Win32 VCP 또는 macOS 미확인 구현을 안다.

**Tech Stack:** Tauri 2, TypeScript, React, Vite, pnpm, Rust, `windows` 0.62, `tauri-plugin-global-shortcut` 2, `tauri-plugin-autostart` 2, `tauri-plugin-notification` 2

## Global Constraints

- 앱 이름: DisplayFlip
- 식별자: `com.displayflip.app`
- 화면: TypeScript + React. 기존 Vanilla `src/main.ts`는 제거한다.
- UI 문구는 한국어로 둔다.
- 맥으로: 모니터 1 HDMI 번호, 모니터 2 DP 번호. Windows로: 모니터 1 DP 번호, 모니터 2 HDMI 번호. 이 대응은 고정이다.
- 어느 PC에서든 두 명령을 모두 실행할 수 있다. 현재 입력을 읽지 못해도 전송은 시도한다.
- 번호는 모니터 1, 모니터 2 순서로 보낸다. 한 대 실패로 다른 대를 되돌리지 않는다.
- 입력 선택 VCP 기능 번호는 `0x60`이다. 입력 번호는 0 이상 255 이하 정수다.
- 설정 파일은 앱 설정 디렉터리의 `settings.json`이다. 맥과 Windows는 설정을 주고받지 않는다.
- 두 역할은 서로 다른 모니터여야 한다.
- Windows 안정 식별자는 EDID 제조사, 제품, 시리얼을 우선하고, 시리얼이 없으면 장치 인스턴스 경로를 쓴다.
- 단축키 기본값은 `Ctrl+Alt+M`, `Ctrl+Alt+W`이다. macOS도 같은 조합을 등록한다.
- 로그인 시 실행 기본값은 켜짐이다.
- 설정이 완료되지 않으면 창을 연다. `--autostart`로 실행되고 설정이 완료되었으면 창을 열지 않는다.
- 창을 닫으면 프로세스는 남는다. 트레이 메뉴는 맥으로, Windows로, 열기, 종료다.
- 전환 중 잠금은 프로세스 메모리에만 둔다.
- 전달됨: 전송 성공이고 읽은 번호가 보낸 번호와 같다.
- 전달됐으나 확인 불가: 전송 성공, 현재 번호 읽기 실패.
- 실패: 전송 거부, 또는 전송 성공 뒤 읽은 번호가 다름. 전송 실패 시에는 읽지 않는다.
- macOS 검증 전 문구: `이 Mac에서는 모니터 제어가 아직 확인되지 않았습니다.`
- 자동 테스트는 실제 모니터를 사용하지 않는다.
- 패키지 관리자는 pnpm이다. Rust 의존성은 Cargo로 추가한다.
- 커밋 메시지는 한글로 쓴다. Git 사용자 이름이나 이메일이 없으면 `git config`를 바꾸지 말고 사용자에게 묻는다.

---

## File structure

- `src-tauri/src/domain.rs` — 목적지, 누락 항목, 전환 계획, 결과, 진행 잠금
- `src-tauri/src/settings.rs` — `settings.json` 읽기, 쓰기, 기본값, 같은 모니터 거부
- `src-tauri/src/control.rs` — `MonitorControl` 트레이트와 플랫폼 선택
- `src-tauri/src/identity.rs` — EDID 바이트에서 안정 식별자 계산
- `src-tauri/src/windows_monitor.rs` — Win32 물리 모니터와 VCP `0x60`
- `src-tauri/src/macos_monitor.rs` — 검증 전 미확인 제어
- `src-tauri/src/lib.rs` — 명령, 트레이, 단축키, 자동 실행, 알림
- `src/main.tsx`, `src/App.tsx`, `src/styles.css` — 한국어 화면
- `src/main.ts` — 삭제

## Task 1: 전환 계획

**Files:**
- Create: `src-tauri/src/domain.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/domain.rs`

**Interfaces:**
- Consumes: 없음
- Produces:
  - `Destination::{Mac, Windows}`
  - `MonitorRoleConfig { id: String, hdmi_code: Option<u8>, dp_code: Option<u8> }`
  - `Settings`, `plan_switch(settings, destination, connected_ids) -> Result<Vec<PlannedCommand>, PlanError>`
  - `PlannedCommand { role: u8, monitor_id: String, code: u8 }`
  - `PlanError::{Incomplete { missing }, SameMonitor, Busy}` with Korean `Display`
  - `is_configured(settings) -> bool`

- [ ] **Step 1: Write the failing test**

`src-tauri/src/domain.rs` 안에 아래 테스트만 먼저 넣고, `src-tauri/src/lib.rs` 첫 줄에 `mod domain;`을 추가한다. 타입은 아직 만들지 않는다.

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml mac_uses_hdmi_then_dp`

Expected: FAIL. `plan_switch` 또는 `Settings`를 찾을 수 없다.

- [ ] **Step 3: Write minimal implementation**

`src-tauri/src/domain.rs`의 테스트 위에 구현을 넣는다.

```rust
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
                let labels: Vec<&str> = missing.iter().copied().map(MissingItem::label).collect();
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib domain::`

Expected: PASS. 6 tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/domain.rs src-tauri/src/lib.rs
git commit -m "전환 목적지별 모니터 번호를 계산한다."
```

## Task 2: 설정 파일

**Files:**
- Create: `src-tauri/src/settings.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/settings.rs`

**Interfaces:**
- Consumes: `domain::Settings`, `domain::MonitorRoles`, `domain::MonitorRoleConfig`, `domain::Hotkeys`, `domain::PlanError`
- Produces:
  - `default_settings() -> Settings`
  - `load_settings(path) -> Result<Settings, String>`
  - `save_settings(path, &Settings) -> Result<(), String>`
  - 기본 단축키 `Ctrl+Alt+M`, `Ctrl+Alt+W`, `launch_at_login: true`

- [ ] **Step 1: Write the failing test**

`src-tauri/src/lib.rs`에 `mod settings;`를 추가하고 `src-tauri/src/settings.rs`에 테스트만 작성한다.

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib settings::`

Expected: FAIL. `load_settings`를 찾을 수 없다.

- [ ] **Step 3: Write minimal implementation**

```rust
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib settings::`

Expected: PASS. 3 tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/settings.rs src-tauri/src/lib.rs
git commit -m "이 PC의 모니터 역할과 단축키를 설정 파일에 저장한다."
```

## Task 3: 가짜 모니터로 전송 결과 판정

**Files:**
- Modify: `src-tauri/src/domain.rs`
- Test: `src-tauri/src/domain.rs`

**Interfaces:**
- Consumes: `plan_switch`, `PlanError`, `Settings`, `Destination`
- Produces:
  - `Delivery::{Delivered, Unconfirmed, Failed { reason: String }}`
  - `MonitorOutcome { role: u8, monitor_id: String, delivery: Delivery }`
  - `trait MonitorControl { fn list_monitors(&mut self) -> Result<Vec<DetectedMonitor>, String>; fn set_input(&mut self, id: &str, code: u8) -> Result<(), String>; fn get_input(&mut self, id: &str) -> Result<u8, String>; }`
  - `DetectedMonitor { id: String, name: String }`
  - `SwitchGate::run`, `SwitchGate::trial`
  - 상수 `INPUT_SELECT: u8 = 0x60`는 제어 구현이 사용한다. 도메인 테스트는 코드 값만 검사한다.

- [ ] **Step 1: Write the failing test**

`domain.rs` 테스트 모듈에 추가한다.

```rust
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
```

`delivered_when_read_matches_sent_code`는 두 모니터 모두 17을 보내도록 설정을 바꾼다. `Fake::get_input`은 항상 17을 반환하므로 이 테스트에서만 두 코드가 17이어야 한다.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib delivered_when_read_matches_sent_code`

Expected: FAIL. `SwitchGate`를 찾을 수 없다.

- [ ] **Step 3: Write minimal implementation**

`domain.rs`에 추가한다.

```rust
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
    pub fn run<C: MonitorControl>(
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

    pub fn trial<C: MonitorControl>(
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

fn execute<C: MonitorControl>(control: &mut C, role: u8, id: &str, code: u8) -> MonitorOutcome {
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib`

Expected: PASS. Task 1과 Task 2 테스트를 포함해 전부 성공한다.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/domain.rs
git commit -m "전송 결과를 모니터별로 판정하고 전환 중에는 새 명령을 막는다."
```

## Task 4: EDID 식별자와 플랫폼 제어

**Files:**
- Create: `src-tauri/src/identity.rs`
- Create: `src-tauri/src/control.rs`
- Create: `src-tauri/src/windows_monitor.rs`
- Create: `src-tauri/src/macos_monitor.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/identity.rs`

**Interfaces:**
- Consumes: `MonitorControl`, `DetectedMonitor`
- Produces: `open_control() -> Box<dyn MonitorControl + Send>`, `edid_stable_id(&[u8]) -> Option<String>`
- Windows `set_input` / `get_input`은 VCP `0x60`만 사용한다.
- macOS `list_monitors`, `set_input`, `get_input`은 `이 Mac에서는 모니터 제어가 아직 확인되지 않았습니다.`를 반환한다.

- [ ] **Step 1: Write the failing test**

`lib.rs`에 `mod identity;`를 추가하고 `identity.rs`에 테스트만 작성한다.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn sample(serial: u32) -> Vec<u8> {
        let mut edid = vec![0u8; 128];
        edid[..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        let word: u16 = (1 << 10) | (2 << 5) | 3;
        edid[8..10].copy_from_slice(&word.to_be_bytes());
        edid[10..12].copy_from_slice(&0x1234u16.to_le_bytes());
        edid[12..16].copy_from_slice(&serial.to_le_bytes());
        edid
    }

    #[test]
    fn edid_id_uses_manufacturer_product_and_serial() {
        assert_eq!(edid_stable_id(&sample(42)).as_deref(), Some("ABC1234-42"));
    }

    #[test]
    fn zero_serial_has_no_edid_id() {
        assert_eq!(edid_stable_id(&sample(0)), None);
    }

    #[test]
    fn short_edid_has_no_id() {
        assert_eq!(edid_stable_id(&[0, 1, 2]), None);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib edid_id_uses_manufacturer_product_and_serial`

Expected: FAIL. `edid_stable_id`를 찾을 수 없다.

- [ ] **Step 3: Write minimal implementation**

`src-tauri`에서 실행한다.

```bash
cargo add windows@0.62 --features Win32_Foundation,Win32_Graphics_Gdi,Win32_Devices_Display,Win32_System_Registry
```

`identity.rs`:

```rust
pub fn edid_stable_id(edid: &[u8]) -> Option<String> {
    if edid.len() < 16 || edid[..8] != [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00] {
        return None;
    }
    let word = u16::from_be_bytes([edid[8], edid[9]]);
    let letters = [
        letter((word >> 10) & 0x1F)?,
        letter((word >> 5) & 0x1F)?,
        letter(word & 0x1F)?,
    ];
    let product = u16::from_le_bytes([edid[10], edid[11]]);
    let serial = u32::from_le_bytes([edid[12], edid[13], edid[14], edid[15]]);
    if serial == 0 {
        return None;
    }
    Some(format!("{}{product:04X}-{serial}", String::from_utf8_lossy(&letters)))
}

fn letter(value: u16) -> Option<u8> {
    if (1..=26).contains(&value) {
        Some(b'A' + (value as u8 - 1))
    } else {
        None
    }
}
```

`macos_monitor.rs`:

```rust
use crate::domain::{DetectedMonitor, MonitorControl};

pub struct UnverifiedMacControl;

const UNVERIFIED: &str = "이 Mac에서는 모니터 제어가 아직 확인되지 않았습니다.";

impl MonitorControl for UnverifiedMacControl {
    fn list_monitors(&mut self) -> Result<Vec<DetectedMonitor>, String> {
        Err(UNVERIFIED.into())
    }
    fn set_input(&mut self, _id: &str, _code: u8) -> Result<(), String> {
        Err(UNVERIFIED.into())
    }
    fn get_input(&mut self, _id: &str) -> Result<u8, String> {
        Err(UNVERIFIED.into())
    }
}
```

`windows_monitor.rs`는 `cfg(windows)`로 컴파일한다. 물리 모니터를 열거하고, `EnumDisplayDevicesW`의 `DeviceID`로 `HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\{hwid}\{instance}\Device Parameters\EDID`를 읽는다. `edid_stable_id`가 `Some`이면 그 문자열을 식별자로 쓰고, `None`이면 `DeviceID` 문자열을 식별자로 쓴다. 화면 이름은 물리 모니터 설명 문자열이다.

```rust
use std::collections::HashMap;

use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, GetVCPFeatureAndVCPFeatureReply, SetVCPFeature,
    PHYSICAL_MONITOR,
};
use windows::Win32::Foundation::{BOOL, ERROR_SUCCESS, HANDLE, LPARAM};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW, HMONITOR,
    MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
};
use windows::core::PCWSTR;

use crate::domain::{DetectedMonitor, MonitorControl};
use crate::identity::edid_stable_id;

const INPUT_SELECT: u8 = 0x60;

struct OpenMonitor {
    handle: HANDLE,
    name: String,
}

pub struct WindowsControl {
    monitors: HashMap<String, OpenMonitor>,
}

impl WindowsControl {
    pub fn open() -> Self {
        let mut monitors = HashMap::new();
        for found in enumerate_handles() {
            let id = found.device_id.clone();
            monitors.insert(id, OpenMonitor { handle: found.handle, name: found.name });
        }
        Self { monitors }
    }
}

impl Drop for WindowsControl {
    fn drop(&mut self) {
        let handles: Vec<PHYSICAL_MONITOR> = self
            .monitors
            .values()
            .map(|monitor| PHYSICAL_MONITOR {
                hPhysicalMonitor: monitor.handle,
                szPhysicalMonitorDescription: [0; 128],
            })
            .collect();
        if !handles.is_empty() {
            unsafe { let _ = DestroyPhysicalMonitors(&handles); }
        }
    }
}

impl MonitorControl for WindowsControl {
    fn list_monitors(&mut self) -> Result<Vec<DetectedMonitor>, String> {
        Ok(self
            .monitors
            .iter()
            .map(|(id, monitor)| DetectedMonitor { id: id.clone(), name: monitor.name.clone() })
            .collect())
    }

    fn set_input(&mut self, id: &str, code: u8) -> Result<(), String> {
        let handle = self.monitors.get(id).ok_or_else(|| format!("모니터를 찾을 수 없습니다: {id}"))?.handle;
        let status = unsafe { SetVCPFeature(handle, INPUT_SELECT, u32::from(code)) };
        if status == 0 {
            Err(format!("입력 번호 {code} 전송이 거부되었습니다. 코드 {status}"))
        } else {
            Ok(())
        }
    }

    fn get_input(&mut self, id: &str) -> Result<u8, String> {
        let handle = self.monitors.get(id).ok_or_else(|| format!("모니터를 찾을 수 없습니다: {id}"))?.handle;
        let mut current = 0u32;
        let status = unsafe { GetVCPFeatureAndVCPFeatureReply(handle, INPUT_SELECT, None, &mut current, None) };
        if status == 0 {
            Err(format!("현재 번호를 읽지 못했습니다. 코드 {status}"))
        } else {
            u8::try_from(current).map_err(|_| format!("현재 번호 {current}가 255를 넘습니다."))
        }
    }
}

struct Found {
    handle: HANDLE,
    name: String,
    device_id: String,
}

fn enumerate_handles() -> Vec<Found> {
    let mut raw = Vec::new();
    unsafe {
        let pointer = &mut raw as *mut Vec<HMONITOR>;
        let _ = EnumDisplayMonitors(None, None, Some(push_monitor), LPARAM(pointer as isize));
    }
    raw.into_iter().filter_map(open_physical).collect()
}

unsafe extern "system" fn push_monitor(monitor: HMONITOR, _: windows::Win32::Graphics::Gdi::HDC, _: *mut windows::Win32::Foundation::RECT, data: LPARAM) -> BOOL {
    let list = &mut *(data.0 as *mut Vec<HMONITOR>);
    list.push(monitor);
    BOOL(1)
}

fn open_physical(monitor: HMONITOR) -> Option<Found> {
    let mut count = 0u32;
    unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, &mut count).ok()? };
    if count == 0 {
        return None;
    }
    let mut physical = vec![PHYSICAL_MONITOR::default(); count as usize];
    unsafe { GetPhysicalMonitorsFromHMONITOR(monitor, &mut physical).ok()? };
    let first = physical[0];
    if physical.len() > 1 {
        unsafe { let _ = DestroyPhysicalMonitors(&physical[1..]); }
    }
    let device_name = monitor_device_name(monitor)?;
    let device_id = display_device_id(&device_name).unwrap_or(device_name);
    let stable = read_edid(&device_id).and_then(|bytes| edid_stable_id(&bytes)).unwrap_or(device_id);
    Some(Found {
        handle: first.hPhysicalMonitor,
        name: utf16_to_string(&first.szPhysicalMonitorDescription),
        device_id: stable,
    })
}

fn monitor_device_name(monitor: HMONITOR) -> Option<String> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let ok = unsafe { GetMonitorInfoW(monitor, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) };
    if ok.as_bool() { Some(utf16_to_string(&info.szDevice)) } else { None }
}

fn display_device_id(device_name: &str) -> Option<String> {
    let mut device = DISPLAY_DEVICEW::default();
    device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
    let name = wide(device_name);
    let ok = unsafe { EnumDisplayDevicesW(PCWSTR(name.as_ptr()), 0, &mut device, 0) };
    if ok.as_bool() { Some(utf16_to_string(&device.DeviceID)) } else { None }
}

fn read_edid(device_id: &str) -> Option<Vec<u8>> {
    let (hwid, instance) = device_id.split('\\').nth(1).zip(device_id.split('\\').nth(3))?;
    let path = format!("SYSTEM\\CurrentControlSet\\Enum\\DISPLAY\\{hwid}\\{instance}\\Device Parameters");
    let wide_path = wide(&path);
    let mut key = HKEY::default();
    let opened = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, PCWSTR(wide_path.as_ptr()), Some(0), KEY_READ, &mut key) };
    if opened != ERROR_SUCCESS {
        return None;
    }
    let value = wide("EDID");
    let mut size = 0u32;
    let _ = unsafe { RegQueryValueExW(key, PCWSTR(value.as_ptr()), None, None, None, Some(&mut size)) };
    let mut buffer = vec![0u8; size as usize];
    let status = unsafe { RegQueryValueExW(key, PCWSTR(value.as_ptr()), None, None, Some(buffer.as_mut_ptr()), Some(&mut size)) };
    unsafe { let _ = RegCloseKey(key); }
    if status == ERROR_SUCCESS { Some(buffer) } else { None }
}

fn utf16_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|unit| *unit == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end]).trim().to_string()
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
```

`SetVCPFeature`와 `GetVCPFeatureAndVCPFeatureReply`는 성공 시 0이 아닌 값을 반환한다. 위 코드의 `status == 0`이 실패다.

`control.rs`:

```rust
use crate::domain::MonitorControl;

pub fn open_control() -> Box<dyn MonitorControl + Send> {
    #[cfg(windows)]
    {
        Box::new(crate::windows_monitor::WindowsControl::open())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(crate::macos_monitor::UnverifiedMacControl)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        Box::new(UnsupportedControl)
    }
}

struct UnsupportedControl;

impl MonitorControl for UnsupportedControl {
    fn list_monitors(&mut self) -> Result<Vec<crate::domain::DetectedMonitor>, String> {
        Err("이 운영체제에서는 모니터 제어를 지원하지 않습니다.".into())
    }
    fn set_input(&mut self, _id: &str, _code: u8) -> Result<(), String> {
        Err("이 운영체제에서는 모니터 제어를 지원하지 않습니다.".into())
    }
    fn get_input(&mut self, _id: &str) -> Result<u8, String> {
        Err("이 운영체제에서는 모니터 제어를 지원하지 않습니다.".into())
    }
}
```

`lib.rs` 모듈 선언:

```rust
mod control;
mod domain;
mod identity;
mod settings;
#[cfg(windows)]
mod windows_monitor;
#[cfg(target_os = "macos")]
mod macos_monitor;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib`

Expected: PASS. EDID 테스트 3개가 추가되고 Windows 모듈이 컴파일된다.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/identity.rs src-tauri/src/control.rs src-tauri/src/windows_monitor.rs src-tauri/src/macos_monitor.rs src-tauri/src/lib.rs
git commit -m "Windows는 VCP로 입력을 바꾸고 macOS는 검증 전 미확인을 반환한다."
```

## Task 5: Tauri 명령

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tauri.conf.json`
- Test: `cargo test --lib`가 계속 통과하고 `cargo check`가 성공한다.

**Interfaces:**
- Consumes: `load_settings`, `save_settings`, `default_settings`, `open_control`, `SwitchGate`, `is_configured`, `Destination`, `MonitorOutcome`, `DetectedMonitor`, `Settings`
- Produces: 명령 `get_settings`, `save_settings`, `list_monitors`, `read_input`, `trial_set_input`, `switch_to`, `shortcut_status`
- `save_settings`는 같은 모니터면 저장하지 않고 `모니터 1과 모니터 2는 서로 다른 모니터여야 합니다.`를 반환한다.
- 설정 경로는 `app.path().app_config_dir()?.join("settings.json")`이다.

- [ ] **Step 1: Write the failing test**

`src-tauri/src/domain.rs` 테스트에 추가한다. `should_show_window`는 아직 만들지 않는다.

```rust
#[test]
fn autostart_with_complete_settings_stays_hidden() {
    assert!(!should_show_window(true, true));
    assert!(should_show_window(true, false));
    assert!(should_show_window(false, true));
}
```

- [ ] **Step 1b: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib autostart_with_complete_settings_stays_hidden`

Expected: FAIL. `should_show_window`를 찾을 수 없다.

- [ ] **Step 2: Replace the greet command**

`domain.rs`에 추가하고, 시작 시 창 표시는 이 함수를 사용한다.

```rust
pub fn should_show_window(autostart: bool, configured: bool) -> bool {
    !(autostart && configured)
}
```

`greet`를 삭제한다. `tauri.conf.json`의 `productName`과 창 `title`을 `DisplayFlip`로 바꾸고 창 label을 `main`으로 명시한다. `visible`은 `false`로 둔다.

```json
"productName": "DisplayFlip",
"app": {
  "windows": [
    {
      "label": "main",
      "title": "DisplayFlip",
      "width": 800,
      "height": 600,
      "visible": false
    }
  ]
}
```

`lib.rs`에 상태를 두고 명령을 등록한다.

```rust
struct AppState {
    settings_path: std::path::PathBuf,
    gate: std::sync::Mutex<domain::SwitchGate>,
    shortcut_error: std::sync::Mutex<Option<String>>,
}

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> Result<domain::Settings, String> {
    settings::load_settings(&state.settings_path)
}

#[tauri::command]
fn save_settings(state: tauri::State<AppState>, settings: domain::Settings) -> Result<(), String> {
    settings::save_settings(&state.settings_path, &settings)
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
    let settings = settings::load_settings(&state.settings_path)?;
    let mut control = control::open_control();
    let connected = control.list_monitors()?.into_iter().map(|monitor| monitor.id).collect::<Vec<_>>();
    gate.run(&mut *control, &settings, destination, &connected).map_err(|err| err.to_string())
}

#[tauri::command]
fn shortcut_status(state: tauri::State<AppState>) -> Option<String> {
    state.shortcut_error.lock().ok().and_then(|error| error.clone())
}
```

`run`에서 config dir을 만들고 `AppState`를 `manage`한 뒤, 설정이 완료되지 않았거나 인자에 `--autostart`가 없으면 `main` 창을 `show`한다. `--autostart`이고 `is_configured`이면 숨긴 채로 둔다.

```rust
let autostart = std::env::args().any(|arg| arg == "--autostart");
let configured = settings::load_settings(&settings_path)
    .map(|settings| domain::is_configured(&settings))
    .unwrap_or(false);
if domain::should_show_window(autostart, configured) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
    }
}
```

- [ ] **Step 3: Run check**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib`

Expected: PASS.

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: PASS. `greet`는 없다.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/tauri.conf.json
git commit -m "화면이 부를 전환, 시험, 설정 명령을 연다."
```

## Task 6: React 화면

**Files:**
- Create: `src/main.tsx`
- Create: `src/App.tsx`
- Modify: `src/styles.css`
- Modify: `index.html`
- Modify: `vite.config.ts`
- Modify: `tsconfig.json`
- Delete: `src/main.ts`
- Test: 스펙은 UI 자동 테스트를 두지 않는다. 확인은 `pnpm exec tsc --noEmit`과 `pnpm build`다.

**Interfaces:**
- Consumes: `get_settings`, `save_settings`, `list_monitors`, `read_input`, `trial_set_input`, `switch_to`, `shortcut_status`
- `switch_to` 인자: `{ destination: "mac" | "windows" }`
- `trial_set_input` 인자: `{ id, code }`
- `read_input` 인자: `{ id }`
- Produces: 맥으로, Windows로 버튼, 역할 선택, 시험 전환, HDMI/DP 저장, 현재 번호 읽기, 단축키 입력, 로그인 시 실행 체크박스

- [ ] **Step 1: Add React and point the page at a missing entry**

```bash
pnpm add react react-dom
pnpm add -D @types/react @types/react-dom @vitejs/plugin-react
```

`index.html`의 script를 `/src/main.tsx`로 바꾸고 본문은 `<div id="root"></div>`만 남긴다. `lang`은 `ko`, title은 `DisplayFlip`이다.

`tsconfig.json` `compilerOptions`에 `"jsx": "react-jsx"`를 추가한다.

`vite.config.ts`에 `import react from "@vitejs/plugin-react"`와 `plugins: [react()]`를 추가한다.

`src/main.ts`를 삭제한다.

- [ ] **Step 2: Run the type check to verify it fails**

Run: `pnpm exec tsc --noEmit`

Expected: FAIL. `src/main.tsx`가 없다.

- [ ] **Step 3: Write the screen**

`src/main.tsx`:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

`src/styles.css`는 아래만 남긴다.

```css
body {
  font-family: sans-serif;
}

main {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin: 24px;
}

.row {
  display: flex;
  gap: 8px;
  align-items: center;
}
```

`src/App.tsx`:

```tsx
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

type Destination = "mac" | "windows";

interface MonitorRole {
  id: string;
  hdmiCode: number | null;
  dpCode: number | null;
}

interface Settings {
  monitors: { "1": MonitorRole | null; "2": MonitorRole | null };
  hotkeys: { toMac: string; toWindows: string };
  launchAtLogin: boolean;
}

interface DetectedMonitor {
  id: string;
  name: string;
}

interface Outcome {
  role: number;
  monitorId: string;
  delivery: { status: "delivered" } | { status: "unconfirmed" } | { status: "failed"; reason: string };
}

function outcomeText(outcome: Outcome): string {
  const delivery = outcome.delivery;
  if (delivery.status === "delivered") return "전달됨";
  if (delivery.status === "unconfirmed") return "전달됐으나 확인 불가";
  return `실패: ${delivery.reason}`;
}

function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [monitors, setMonitors] = useState<DetectedMonitor[]>([]);
  const [notice, setNotice] = useState("");
  const [code, setCode] = useState(17);

  useEffect(() => {
    void invoke<Settings>("get_settings").then(setSettings);
    void invoke<DetectedMonitor[]>("list_monitors").then(setMonitors).catch((error: unknown) => {
      setNotice(String(error));
    });
    void invoke<string | null>("shortcut_status").then((error) => {
      if (error) setNotice(error);
    });
    const unlisten = listen<{ outcomes: Outcome[] | null; error: string | null }>("switch-finished", (event) => {
      if (event.payload.error) setNotice(event.payload.error);
      else if (event.payload.outcomes) setNotice(event.payload.outcomes.map(outcomeText).join("\n"));
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  if (!settings) return <main>설정을 불러오는 중입니다.</main>;

  async function switchTo(destination: Destination) {
    try {
      const outcomes = await invoke<Outcome[]>("switch_to", { destination });
      setNotice(outcomes.map(outcomeText).join("\n"));
    } catch (error) {
      setNotice(String(error));
    }
  }

  function role(index: "1" | "2"): MonitorRole {
    return settings!.monitors[index] ?? { id: "", hdmiCode: null, dpCode: null };
  }

  async function assign(index: "1" | "2", id: string) {
    const next = { ...settings!, monitors: { ...settings!.monitors, [index]: { ...role(index), id } } };
    await invoke("save_settings", { settings: next });
    setSettings(next);
  }

  async function saveCode(index: "1" | "2", field: "hdmiCode" | "dpCode") {
    const next = {
      ...settings!,
      monitors: { ...settings!.monitors, [index]: { ...role(index), [field]: code } },
    };
    await invoke("save_settings", { settings: next });
    setSettings(next);
  }

  return (
    <main>
      <p>{notice}</p>
      <div className="row">
        <button type="button" onClick={() => void switchTo("mac")}>맥으로</button>
        <button type="button" onClick={() => void switchTo("windows")}>Windows로</button>
      </div>
      {(["1", "2"] as const).map((index) => (
        <section key={index}>
          <label>
            모니터 {index}
            <select value={role(index).id} onChange={(event) => void assign(index, event.target.value)}>
              <option value="">선택</option>
              {monitors.map((monitor) => (
                <option key={monitor.id} value={monitor.id}>{monitor.name}</option>
              ))}
            </select>
          </label>
          <div className="row">
            <input type="number" min={0} max={255} value={code} onChange={(event) => setCode(Number(event.target.value))} />
            <button type="button" onClick={() => void invoke("trial_set_input", { id: role(index).id, code }).then((outcome) => setNotice(outcomeText(outcome as Outcome))).catch((error: unknown) => setNotice(String(error)))}>이 번호로 시험</button>
            <button type="button" onClick={() => void saveCode(index, "hdmiCode")}>HDMI로 저장</button>
            <button type="button" onClick={() => void saveCode(index, "dpCode")}>DP로 저장</button>
            <button type="button" onClick={() => void invoke<number>("read_input", { id: role(index).id }).then((value) => setNotice(`현재 번호 ${value}`)).catch((error: unknown) => setNotice(String(error)))}>현재 번호 읽기</button>
          </div>
        </section>
      ))}
      <label>
        맥으로 단축키
        <input value={settings.hotkeys.toMac} onChange={(event) => setSettings({ ...settings, hotkeys: { ...settings.hotkeys, toMac: event.target.value } })} />
      </label>
      <label>
        Windows로 단축키
        <input value={settings.hotkeys.toWindows} onChange={(event) => setSettings({ ...settings, hotkeys: { ...settings.hotkeys, toWindows: event.target.value } })} />
      </label>
      <label>
        <input type="checkbox" checked={settings.launchAtLogin} onChange={(event) => setSettings({ ...settings, launchAtLogin: event.target.checked })} />
        로그인 시 실행
      </label>
      <button type="button" onClick={() => void invoke("save_settings", { settings }).then(() => setNotice("설정을 저장했습니다.")).catch((error: unknown) => setNotice(String(error)))}>설정 저장</button>
    </main>
  );
}

export default App;
```

- [ ] **Step 4: Run the type check and build**

Run: `pnpm exec tsc --noEmit`

Expected: PASS.

Run: `pnpm build`

Expected: PASS. `dist`가 생성된다.

- [ ] **Step 5: Commit**

```bash
git add package.json pnpm-lock.yaml index.html vite.config.ts tsconfig.json src/main.tsx src/App.tsx src/styles.css
git add -u src/main.ts
git commit -m "전환 버튼과 모니터 번호 설정 화면을 한국어로 띄운다."
```

## Task 7: 트레이, 단축키, 자동 실행, 알림

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- `src-tauri/capabilities/default.json`은 수정하지 않는다. 웹뷰는 플러그인 명령을 직접 호출하지 않는다.
- Test: `cargo test --lib` PASS. 하드웨어 동작은 아래 수동 확인이다.

**Interfaces:**
- Consumes: `switch_to`와 같은 `SwitchGate::run` 경로, `Settings.hotkeys`, `Settings.launch_at_login`
- Produces: 창이 숨겨져 있을 때 알림. 창이 보일 때는 이벤트 `switch-finished`만 보내고 알림은 보내지 않는다. 이벤트 payload는 `Vec<MonitorOutcome>` 또는 문자열 오류다.

- [ ] **Step 1: Add plugins**

`src-tauri/Cargo.toml`의 tauri dependency에 feature `tray-icon`을 추가한다.

```bash
cargo add tauri-plugin-global-shortcut@2 --manifest-path src-tauri/Cargo.toml
cargo add tauri-plugin-autostart@2 --manifest-path src-tauri/Cargo.toml
cargo add tauri-plugin-notification@2 --manifest-path src-tauri/Cargo.toml
```

`src-tauri` 디렉터리에서 `cargo add`가 manifest path 없이 동작하면 그 디렉터리에서 실행한다.

- [ ] **Step 2: Register plugins before the handlers exist**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: 플러그인 crate는 해석된다. 트레이와 단축키 동작은 아직 없다.

- [ ] **Step 3: Write tray, shortcuts, autostart, and notifications**

`lib.rs`의 builder에 플러그인을 등록한다.

```rust
.plugin(tauri_plugin_notification::init())
.plugin(tauri_plugin_autostart::init(
    tauri_plugin_autostart::MacosLauncher::LaunchAgent,
    Some(vec!["--autostart"]),
))
.plugin(tauri_plugin_global_shortcut::Builder::new().build())
```

창 `CloseRequested`에서는 `api.prevent_close()` 후 `window.hide()`를 호출한다. 단축키 핸들러는 `ShortcutState::Pressed`에서만 `perform_switch`를 호출하고 `Released`는 무시한다.

```rust
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SwitchFinished {
    outcomes: Option<Vec<domain::MonitorOutcome>>,
    error: Option<String>,
}

fn perform_switch(app: &tauri::AppHandle, destination: domain::Destination) {
    let state = app.state::<AppState>();
    let result = (|| {
        let mut gate = state.gate.try_lock().map_err(|_| domain::PlanError::Busy.to_string())?;
        let settings = settings::load_settings(&state.settings_path)?;
        let mut control = control::open_control();
        let connected = control.list_monitors()?.into_iter().map(|monitor| monitor.id).collect::<Vec<_>>();
        gate.run(&mut *control, &settings, destination, &connected).map_err(|err| err.to_string())
    })();
    let window = app.get_webview_window("main");
    let visible = window.as_ref().and_then(|window| window.is_visible().ok()).unwrap_or(false);
    if visible {
        let payload = match result {
            Ok(outcomes) => SwitchFinished { outcomes: Some(outcomes), error: None },
            Err(message) => SwitchFinished { outcomes: None, error: Some(message) },
        };
        let _ = app.emit("switch-finished", payload);
        return;
    }
    let body = match &result {
        Ok(outcomes) => outcomes.iter().map(outcome_line).collect::<Vec<_>>().join("\n"),
        Err(message) => message.clone(),
    };
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title("DisplayFlip").body(body).show();
}

fn outcome_line(outcome: &domain::MonitorOutcome) -> String {
    let delivery = match &outcome.delivery {
        domain::Delivery::Delivered => "전달됨".to_string(),
        domain::Delivery::Unconfirmed => "전달됐으나 확인 불가".to_string(),
        domain::Delivery::Failed { reason } => format!("실패: {reason}"),
    };
    format!("모니터 {}: {delivery}", if outcome.role == 0 { outcome.monitor_id.clone() } else { outcome.role.to_string() })
}
```

트레이는 setup에서 만든다. 메뉴 id는 `to-mac`, `to-windows`, `open`, `quit`이고 문구는 `맥으로`, `Windows로`, `열기`, `종료`다. 아이콘은 `app.default_window_icon()`이다. 왼쪽 클릭과 `open`은 `main` 창을 `show`한다. `quit`는 `app.exit(0)`이다.

```rust
let to_mac = tauri::menu::MenuItem::with_id(app, "to-mac", "맥으로", true, None::<&str>)?;
let to_windows = tauri::menu::MenuItem::with_id(app, "to-windows", "Windows로", true, None::<&str>)?;
let open = tauri::menu::MenuItem::with_id(app, "open", "열기", true, None::<&str>)?;
let quit = tauri::menu::MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
let menu = tauri::menu::Menu::with_items(app, &[&to_mac, &to_windows, &open, &quit])?;
let icon = app.default_window_icon().cloned();
let mut tray = tauri::tray::TrayIconBuilder::new().menu(&menu).on_menu_event(|app, event| {
    match event.id().as_ref() {
        "to-mac" => perform_switch(app, domain::Destination::Mac),
        "to-windows" => perform_switch(app, domain::Destination::Windows),
        "open" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        "quit" => app.exit(0),
        _ => {}
    }
});
if let Some(icon) = icon {
    tray = tray.icon(icon);
}
let _ = tray.build(app);
```

단축키 파서는 `lib.rs`에 둔다. 해석 실패 문구는 `단축키를 해석할 수 없습니다: {value}`이고, 등록 실패 문구는 `단축키를 등록하지 못했습니다: {error}`다. 등록에 실패해도 명령 버튼 경로는 그대로 둔다.

```rust
fn parse_shortcut(value: &str) -> Result<tauri_plugin_global_shortcut::Shortcut, String> {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
    let mut modifiers = Modifiers::empty();
    let mut key = None;
    for part in value.split('+') {
        match part {
            "Ctrl" => modifiers |= Modifiers::CONTROL,
            "Alt" => modifiers |= Modifiers::ALT,
            "Shift" => modifiers |= Modifiers::SHIFT,
            "Super" => modifiers |= Modifiers::SUPER,
            "A" => key = Some(Code::KeyA),
            "B" => key = Some(Code::KeyB),
            "C" => key = Some(Code::KeyC),
            "D" => key = Some(Code::KeyD),
            "E" => key = Some(Code::KeyE),
            "F" => key = Some(Code::KeyF),
            "G" => key = Some(Code::KeyG),
            "H" => key = Some(Code::KeyH),
            "I" => key = Some(Code::KeyI),
            "J" => key = Some(Code::KeyJ),
            "K" => key = Some(Code::KeyK),
            "L" => key = Some(Code::KeyL),
            "M" => key = Some(Code::KeyM),
            "N" => key = Some(Code::KeyN),
            "O" => key = Some(Code::KeyO),
            "P" => key = Some(Code::KeyP),
            "Q" => key = Some(Code::KeyQ),
            "R" => key = Some(Code::KeyR),
            "S" => key = Some(Code::KeyS),
            "T" => key = Some(Code::KeyT),
            "U" => key = Some(Code::KeyU),
            "V" => key = Some(Code::KeyV),
            "W" => key = Some(Code::KeyW),
            "X" => key = Some(Code::KeyX),
            "Y" => key = Some(Code::KeyY),
            "Z" => key = Some(Code::KeyZ),
            "0" => key = Some(Code::Digit0),
            "1" => key = Some(Code::Digit1),
            "2" => key = Some(Code::Digit2),
            "3" => key = Some(Code::Digit3),
            "4" => key = Some(Code::Digit4),
            "5" => key = Some(Code::Digit5),
            "6" => key = Some(Code::Digit6),
            "7" => key = Some(Code::Digit7),
            "8" => key = Some(Code::Digit8),
            "9" => key = Some(Code::Digit9),
            _ => return Err(format!("단축키를 해석할 수 없습니다: {value}")),
        }
    }
    let key = key.ok_or_else(|| format!("단축키를 해석할 수 없습니다: {value}"))?;
    Ok(Shortcut::new(Some(modifiers), key))
}
```

```rust
#[cfg(test)]
mod tests {
    use super::parse_shortcut;

    #[test]
    fn default_shortcuts_parse() {
        assert!(parse_shortcut("Ctrl+Alt+M").is_ok());
        assert!(parse_shortcut("Ctrl+Alt+W").is_ok());
        assert!(parse_shortcut("Nope").unwrap_err().contains("단축키를 해석할 수 없습니다"));
    }
}
```

위 테스트는 `parse_shortcut` 구현과 함께 `lib.rs`에 둔다. Step 4의 `cargo test`가 이 테스트를 포함한다.

`save_settings` 명령은 저장 후 단축키를 다시 등록하고, `launch_at_login`이 true면 `app.autolaunch().enable()`, false면 `disable()`을 호출한다. enable이 실패하면 디스크의 `launch_at_login`을 false로 다시 저장하고 `로그인 자동 실행을 등록하지 못했습니다: {error}`를 반환한다.

`src/App.tsx`는 `listen("switch-finished", ...)`로 같은 결과 영역을 갱신한다.

- [ ] **Step 4: Run tests and check**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib`

Expected: PASS.

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

Run: `pnpm exec tsc --noEmit`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs src/App.tsx
git commit -m "창을 닫아도 단축키와 트레이로 전환하고 로그인 시 실행한다."
```

## Task 8: Windows 수동 확인

자동 테스트로 대체하지 않는다. 주연테크 Q27C15 두 대가 연결된 Windows PC에서 `pnpm tauri dev`로 확인한다.

- [ ] 모니터 두 대가 목록에 나온다.
- [ ] 모니터 1과 모니터 2에 서로 다른 모니터를 저장할 수 있다. 같은 모니터는 거부된다.
- [ ] 시험 전환으로 각 모니터의 HDMI 번호와 DP 번호를 저장할 수 있다.
- [ ] 앱을 다시 실행해도 역할, 번호, 단축키가 유지된다.
- [ ] `맥으로`는 모니터 1에 HDMI 번호, 모니터 2에 DP 번호를 보낸다.
- [ ] `Windows로`는 그 반대 번호를 보낸다.
- [ ] 창을 닫아도 `Ctrl+Alt+M`, `Ctrl+Alt+W`와 트레이 메뉴가 동작한다.
- [ ] 다시 로그인하면 앱이 실행되고, 설정이 완료된 상태면 창은 열리지 않는다.
- [ ] 화면을 가지고 있지 않은 PC에서 명령을 보내면 실패 이유가 모니터별로 보인다.

macOS 구현은 이 계획에 포함하지 않는다. M4에서 m1ddc로 같은 시험 전환이 확인되기 전에는 `macos_monitor.rs`의 미확인 문구만 유지한다.

## Spec coverage

- 두 목적지 명령과 고정 입력 대응: Task 1
- 설정 파일, 기본 단축키, 로그인 기본값, 같은 모니터 거부: Task 2
- 세 가지 전송 결과, 되돌리지 않음, 진행 잠금, 시험 전환: Task 3
- Windows VCP `0x60`, EDID 식별자, macOS 미확인 문구: Task 4
- Tauri 명령, DisplayFlip 이름, 미완료 시 창 표시, autostart 시 창 숨김: Task 5
- 한국어 버튼과 설정 화면: Task 6
- 트레이, 전역 단축키, 자동 실행, 숨은 창 알림: Task 7
- 실제 모니터 수동 확인과 macOS 검증 전 보류: Task 8
