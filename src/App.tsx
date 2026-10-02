import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { ShortcutInput } from "./ShortcutInput";
import { formatShortcut } from "./shortcutCapture";

type Destination = "mac" | "windows";
type MonitorIndex = "1" | "2";

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

const CODE_RANGE_NOTICE = "입력 번호는 0부터 255까지의 정수여야 합니다.";
const SAVED_NOTICE = "설정을 저장했습니다.";
const SHORTCUT_REGISTER_FAILURE = "단축키를 등록하지 못했습니다";
const AUTOLAUNCH_REGISTER_FAILURE = "로그인 자동 실행을 등록하지 못했습니다";
const AUTOLAUNCH_DISABLE_FAILURE = "로그인 자동 실행을 해제하지 못했습니다";

function noticeAfterSuccessfulSave(shortcutStatus: string | null): string {
  if (shortcutStatus?.includes(SHORTCUT_REGISTER_FAILURE)) {
    return shortcutStatus;
  }
  return SAVED_NOTICE;
}

function autolaunchFileWasRewritten(message: string): boolean {
  return message.includes(AUTOLAUNCH_REGISTER_FAILURE) || message.includes(AUTOLAUNCH_DISABLE_FAILURE);
}

function roleFrom(settings: Settings, index: MonitorIndex): MonitorRole {
  return settings.monitors[index] ?? { id: "", hdmiCode: null, dpCode: null };
}

function withLatestForm(saved: Settings, live: Settings): Settings {
  return { ...saved, hotkeys: live.hotkeys, launchAtLogin: live.launchAtLogin };
}

function sameFormFields(file: Settings, form: Settings): boolean {
  return file.launchAtLogin === form.launchAtLogin
    && file.hotkeys.toMac === form.hotkeys.toMac
    && file.hotkeys.toWindows === form.hotkeys.toWindows;
}

function outcomeText(outcome: Outcome): string {
  const delivery = outcome.delivery;
  if (delivery.status === "delivered") return `모니터 ${outcome.role} · 선택한 입력으로 변경했습니다.`;
  if (delivery.status === "unconfirmed") return `모니터 ${outcome.role} · 전환 요청을 보냈습니다. 실제 화면이 바뀌었는지 확인해주세요.`;
  return `모니터 ${outcome.role} · 전환하지 못했습니다. ${delivery.reason}`;
}

function parseCode(raw: string): number | null {
  const trimmed = raw.trim();
  if (!/^\d{1,3}$/.test(trimmed)) return null;
  const value = Number(trimmed);
  if (value > 255) return null;
  return value;
}

function savedCode(value: number | null): string {
  return value === null ? "설정 필요" : String(value);
}

function App() {
  const [page, setPage] = useState<"switch" | "settings">("switch");
  const [switching, setSwitching] = useState<Destination | null>(null);
  const [saving, setSaving] = useState(0);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [appliedSettings, setAppliedSettings] = useState<Settings | null>(null);
  const [monitors, setMonitors] = useState<DetectedMonitor[]>([]);
  const [refreshingMonitors, setRefreshingMonitors] = useState(false);
  const [notice, setNotice] = useState("");
  const [codes, setCodes] = useState<Record<MonitorIndex, string>>({ "1": "", "2": "" });
  const [shortcutCaptureReady, setShortcutCaptureReady] = useState(false);
  const shortcutCaptureFocused = useRef(false);
  const shortcutCaptureRequest = useRef(0);
  const shortcutCaptureQueue = useRef<Promise<void>>(Promise.resolve());
  const settingsRef = useRef<Settings | null>(null);
  const saveQueue = useRef<Promise<Settings | null>>(Promise.resolve(null));
  settingsRef.current = settings;

  async function captureShortcuts(capturing: boolean) {
    shortcutCaptureFocused.current = capturing;
    const request = ++shortcutCaptureRequest.current;
    setShortcutCaptureReady(false);
    const operation = shortcutCaptureQueue.current.then(() => invoke<void>("set_shortcut_capture", { capturing }));
    shortcutCaptureQueue.current = operation.catch(() => {});
    try {
      await operation;
      if (request === shortcutCaptureRequest.current && capturing && shortcutCaptureFocused.current) setShortcutCaptureReady(true);
    } catch (error) {
      setNotice(String(error));
    }
  }

  async function refreshMonitors() {
    setRefreshingMonitors(true);
    try {
      const detected = await invoke<DetectedMonitor[]>("list_monitors");
      setMonitors(detected);
      setNotice(detected.length ? `모니터 ${detected.length}대를 찾았습니다.` : "연결된 외부 모니터가 없습니다.");
    } catch (error) {
      setNotice(String(error));
    } finally {
      setRefreshingMonitors(false);
    }
  }

  function enqueueSave(apply: (current: Settings) => Settings) {
    setSaving((count) => count + 1);
    const task = saveQueue.current.then(async (committed) => {
      const live = settingsRef.current;
      if (!live) return committed;
      let onDisk = committed;
      try {
        const base = committed ? withLatestForm(committed, live) : live;
        let saved = apply(base);
        while (true) {
          await invoke("save_settings", { settings: saved });
          onDisk = saved;
          setAppliedSettings(saved);
          const form = settingsRef.current ?? saved;
          const aligned = withLatestForm(saved, form);
          setSettings(aligned);
          if (sameFormFields(saved, form)) {
            const shortcutError = await invoke<string | null>("shortcut_status");
            setNotice(noticeAfterSuccessfulSave(shortcutError));
            return saved;
          }
          saved = aligned;
        }
      } catch (error) {
        const message = String(error);
        setNotice(message);
        if (!autolaunchFileWasRewritten(message)) {
          return onDisk;
        }
        try {
          const reloaded = await invoke<Settings>("get_settings");
          const form = settingsRef.current ?? reloaded;
          const adopted = { ...reloaded, hotkeys: form.hotkeys };
          settingsRef.current = adopted;
          setSettings(adopted);
          setAppliedSettings(reloaded);
          return reloaded;
        } catch {
          return onDisk;
        }
      }
    });
    const tracked = task.finally(() => setSaving((count) => count - 1));
    saveQueue.current = tracked;
    return tracked;
  }

  useEffect(() => {
    const pauseCapture = () => { void captureShortcuts(false); };
    const resumeCapture = () => {
      if (document.activeElement?.matches("[data-shortcut-input]")) void captureShortcuts(true);
    };
    window.addEventListener("blur", pauseCapture);
    window.addEventListener("focus", resumeCapture);
    void invoke<Settings>("get_settings").then((loaded) => { setSettings(loaded); setAppliedSettings(loaded); }).catch((error: unknown) => {
      setNotice(String(error));
    });
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
      window.removeEventListener("blur", pauseCapture);
      window.removeEventListener("focus", resumeCapture);
      void invoke("set_shortcut_capture", { capturing: false }).catch(() => {});
      void unlisten.then((stop) => stop());
    };
  }, []);

  if (!settings) {
    return (
      <main className="app-shell" role="status">
        {notice ? <p className="notice">{notice}</p> : null}
        설정을 불러오는 중입니다.
      </main>
    );
  }

  async function switchTo(destination: Destination) {
    if (switching) return;
    setSwitching(destination);
    try {
      const outcomes = await invoke<Outcome[]>("switch_to", { destination });
      setNotice(outcomes.map(outcomeText).join("\n"));
    } catch (error) {
      setNotice(String(error));
    } finally {
      setSwitching(null);
    }
  }

  function role(index: MonitorIndex): MonitorRole {
    return roleFrom(settings!, index);
  }

  function draftCode(index: MonitorIndex): number | null {
    const value = parseCode(codes[index]);
    if (value === null) setNotice(CODE_RANGE_NOTICE);
    return value;
  }

  function assign(index: MonitorIndex, id: string) {
    return enqueueSave((current) => ({
      ...current,
      monitors: { ...current.monitors, [index]: { ...roleFrom(current, index), id } },
    }));
  }

  function saveCode(index: MonitorIndex, field: "hdmiCode" | "dpCode") {
    const value = draftCode(index);
    if (value === null) return Promise.resolve(null);
    return enqueueSave((current) => ({
      ...current,
      monitors: { ...current.monitors, [index]: { ...roleFrom(current, index), [field]: value } },
    }));
  }

  function saveDisplayedSettings() {
    return enqueueSave((current) => withLatestForm(current, settingsRef.current ?? current));
  }

  async function trial(index: MonitorIndex) {
    const value = draftCode(index);
    if (value === null) return;
    try {
      const outcome = await invoke<Outcome>("trial_set_input", { id: role(index).id, code: value });
      setNotice(outcomeText({ ...outcome, role: Number(index) }));
    } catch (error) {
      setNotice(String(error));
    }
  }

  const destinationReady = (destination: Destination) => (["1", "2"] as const).every((index) => {
    const monitor = role(index);
    const usesHdmi = destination === "mac" ? index === "1" : index === "2";
    return monitor.id && (usesHdmi ? monitor.hdmiCode : monitor.dpCode) !== null && monitors.some((item) => item.id === monitor.id);
  }) && role("1").id !== role("2").id;
  const configured = destinationReady("mac") && destinationReady("windows");
  const monitorName = (index: MonitorIndex) => monitors.find((monitor) => monitor.id === role(index).id)?.name ?? "모니터를 선택해주세요";
  const shortcutLabel = formatShortcut;
  const displayedHotkeys = appliedSettings?.hotkeys ?? settings.hotkeys;
  const unsaved = appliedSettings !== null && !sameFormFields(appliedSettings, settings);

  return (
    <main className="app-shell">
      <header className="app-header">
        <div className="brand"><img src="/displayflip-icon.png" alt="" width="32" height="32" /><span>DisplayFlip</span></div>
        <nav aria-label="화면 메뉴" className="navigation">
          <button type="button" aria-current={page === "switch" ? "page" : undefined} onClick={() => setPage("switch")}>화면 전환</button>
          <button type="button" aria-current={page === "settings" ? "page" : undefined} onClick={() => setPage("settings")}>설정</button>
        </nav>
      </header>
      {notice && <div className={`notice ${/실패|못했|못합니다|정수여야|동일|서로 다른/.test(notice) ? "notice-warning" : ""}`} role="status" aria-live="polite"><span className="notice-label">알림</span><p>{notice}</p><button type="button" className="notice-close" aria-label="알림 닫기" onClick={() => setNotice("")}>×</button></div>}

      {page === "switch" ? <>
        <div className="page-heading"><p className="eyebrow">두 모니터를 한 번에</p><h1>어느 컴퓨터를 사용할까요?</h1><p>사용할 컴퓨터를 선택하면 모니터 입력을 함께 바꿉니다.</p></div>
        {!configured && <div className="setup-prompt"><span>아직 준비되지 않은 연결이 있습니다. 모니터와 각 컴퓨터의 입력을 확인해주세요.</span><button type="button" onClick={() => setPage("settings")}>모니터 설정하기 <span aria-hidden="true">→</span></button></div>}
        <div className="switch-grid">
          {(["mac", "windows"] as const).map((destination) => <button
            className={`switch-card ${destination}`} type="button" key={destination}
            disabled={!destinationReady(destination) || switching !== null} aria-busy={switching === destination}
            onClick={() => void switchTo(destination)}>
            <span className="computer-mark">{destination === "mac" ? <MonitorIcon /> : <WindowsIcon />}</span>
            <span className="switch-card-title">{switching === destination ? "전환 요청 중…" : destination === "mac" ? "Mac 사용하기" : "Windows 사용하기"}<span aria-hidden="true">↗</span></span>
            <span className="switch-card-detail">모니터 1 {destination === "mac" ? "HDMI" : "DP"}<span aria-hidden="true"> · </span>모니터 2 {destination === "mac" ? "DP" : "HDMI"}</span>
            <span className="shortcut-badge" aria-label={`단축키 ${shortcutLabel(destination === "mac" ? displayedHotkeys.toMac : displayedHotkeys.toWindows)}`}>{shortcutLabel(destination === "mac" ? displayedHotkeys.toMac : displayedHotkeys.toWindows)}</span>
          </button>)}
        </div>
        <section className="monitor-overview" aria-labelledby="monitor-overview-title">
          <div className="section-heading"><h2 id="monitor-overview-title">전환할 모니터</h2><button className="text-button" type="button" onClick={() => setPage("settings")}>연결 설정 <span aria-hidden="true">→</span></button></div>
          {(["1", "2"] as const).map((index) => <div className="monitor-summary" key={index}>
            <span className="monitor-number">{index}</span><div><strong>{role(index).id ? monitorName(index) : "모니터를 선택해주세요"}</strong><span className="monitor-route">Mac {index === "1" ? "HDMI" : "DP"} <span aria-hidden="true">/</span> Windows {index === "1" ? "DP" : "HDMI"}</span></div>
            <span className={`connection-state ${monitors.some((monitor) => monitor.id === role(index).id) ? "connected" : ""}`}>{!role(index).id ? "설정 필요" : monitors.some((monitor) => monitor.id === role(index).id) ? "연결됨" : "연결 확인 필요"}</span>
          </div>)}
        </section>
        <p className="quiet-help">창을 닫아도 단축키로 전환할 수 있습니다.</p>
      </> : <>
        <div className="page-heading"><p className="eyebrow">처음 한 번만 설정하세요</p><h1>내 컴퓨터와 모니터 연결</h1><p>모니터를 고르고, 각 컴퓨터에 연결된 입력을 확인하세요.</p></div>
        <section className="settings-section" aria-labelledby="monitor-settings-title">
          <div className="section-heading"><h2 id="monitor-settings-title">모니터 연결</h2><button type="button" className="secondary-button" disabled={refreshingMonitors} onClick={() => void refreshMonitors()}>{refreshingMonitors ? "모니터 찾는 중…" : "모니터 다시 찾기"}</button></div>
          {!monitors.length && <p className="empty-help">연결된 외부 모니터를 찾지 못했습니다. 케이블을 확인한 뒤 ‘모니터 다시 찾기’를 눌러주세요.</p>}
          {(["1", "2"] as const).map((index) => <section className="monitor-editor" key={index} aria-labelledby={`monitor-${index}-title`}>
            <label className="field-label" htmlFor={`monitor-${index}`} id={`monitor-${index}-title`}><span className="monitor-number">{index}</span>모니터 {index}</label>
            <select id={`monitor-${index}`} value={role(index).id} onChange={(event) => void assign(index, event.target.value)}>
              <option value="">연결할 모니터를 선택하세요</option>
              {role(index).id && !monitors.some((monitor) => monitor.id === role(index).id) && <option value={role(index).id}>저장된 모니터 · 현재 연결되지 않음</option>}
              {monitors.map((monitor) => <option key={monitor.id} value={monitor.id} disabled={role(index === "1" ? "2" : "1").id === monitor.id}>{monitor.name}</option>)}
            </select>
            <div className="input-routes"><div><span>Mac을 사용할 때</span><strong>{index === "1" ? "HDMI" : "DP"}<small>입력 번호 {savedCode(index === "1" ? role(index).hdmiCode : role(index).dpCode)}</small></strong></div><div><span>Windows를 사용할 때</span><strong>{index === "1" ? "DP" : "HDMI"}<small>입력 번호 {savedCode(index === "1" ? role(index).dpCode : role(index).hdmiCode)}</small></strong></div></div>
            <details className="advanced-settings"><summary>화면이 전환되지 않나요? 입력 번호 조정</summary>
              <div className="advanced-content"><p>입력 번호는 모니터마다 다를 수 있습니다. 번호로 전환을 시험한 뒤, 원하는 컴퓨터 화면이 나오면 연결된 포트로 저장하세요.</p>
                <label className="field-label" htmlFor={`code-${index}`}>시험할 입력 번호</label>
                <div className="test-controls"><input id={`code-${index}`} type="number" min={0} max={255} placeholder="예: 17" value={codes[index]} onChange={(event) => setCodes((current) => ({ ...current, [index]: event.target.value }))} /><button className="secondary-button" type="button" disabled={!role(index).id || parseCode(codes[index]) === null} onClick={() => void trial(index)}>이 번호로 화면 전환 시험</button></div>
                <div className="advanced-actions"><button type="button" disabled={!role(index).id} className="text-button" onClick={() => void invoke<number>("read_input", { id: role(index).id }).then((value) => { setCodes((current) => ({ ...current, [index]: String(value) })); setNotice(`모니터 ${index}의 현재 입력 번호는 ${value}입니다. 시험 번호 칸에 채워두었습니다.`); }).catch((error: unknown) => setNotice(String(error)))}>현재 입력 번호 가져오기</button></div>
                <div className="port-save-actions"><button className="secondary-button" type="button" disabled={!role(index).id || parseCode(codes[index]) === null} onClick={() => void saveCode(index, "hdmiCode")}>HDMI 번호로 저장 <small>{index === "1" ? "Mac 연결" : "Windows 연결"}</small></button><button className="secondary-button" type="button" disabled={!role(index).id || parseCode(codes[index]) === null} onClick={() => void saveCode(index, "dpCode")}>DP 번호로 저장 <small>{index === "1" ? "Windows 연결" : "Mac 연결"}</small></button></div>
                <p className="quiet-help">요청이 전달돼도 화면이 검게 나올 수 있습니다. 실제 화면을 확인한 뒤 번호를 저장하세요.</p>
              </div>
            </details>
          </section>)}
          <p className="auto-save-note">모니터 선택과 입력 번호는 변경할 때 바로 저장됩니다.</p>
        </section>
        <section className="settings-section" aria-labelledby="shortcut-title"><div className="section-heading"><h2 id="shortcut-title">전환 단축키</h2></div><p className="section-description" id="shortcut-help">아래 칸을 클릭하고 원하는 키 조합을 누르세요. 여러 보조키와 일반 키 하나를 함께 사용할 수 있습니다.</p>
          <div className="shortcut-fields" onFocus={(event) => { if (!event.currentTarget.contains(event.relatedTarget)) void captureShortcuts(true); }} onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget)) void captureShortcuts(false); }}>
            <label>Mac으로 전환<ShortcutInput value={settings.hotkeys.toMac} ready={shortcutCaptureReady} onChange={(value) => setSettings((current) => current && ({ ...current, hotkeys: { ...current.hotkeys, toMac: value } }))} /></label>
            <label>Windows로 전환<ShortcutInput value={settings.hotkeys.toWindows} ready={shortcutCaptureReady} onChange={(value) => setSettings((current) => current && ({ ...current, hotkeys: { ...current.hotkeys, toWindows: value } }))} /></label>
          </div><p className="quiet-help">입력 중에는 화면이 전환되지 않습니다. Esc로 입력 종료 · Tab으로 다음 칸 이동</p>
        </section>
        <section className="settings-section startup-section"><label className="toggle-label"><span><strong>컴퓨터 로그인 시 자동 실행</strong><small>로그인하면 창을 열지 않고 백그라운드에서 준비합니다.</small></span><input className="toggle" type="checkbox" checked={settings.launchAtLogin} onChange={(event) => setSettings({ ...settings, launchAtLogin: event.target.checked })} /></label></section>
        <div className="save-bar"><span>{unsaved ? "아직 적용하지 않은 변경이 있습니다. 저장해주세요." : "단축키와 자동 실행 변경은 저장 후 적용됩니다."}</span><button className="primary-button" type="button" disabled={saving > 0} onClick={() => void saveDisplayedSettings()}>{saving > 0 ? "설정 저장 중…" : "설정 저장"}</button></div>
      </>}
    </main>
  );
}

function MonitorIcon() {
  return <svg viewBox="0 0 32 32" fill="none" aria-hidden="true"><rect x="3" y="5" width="26" height="18" rx="3" stroke="currentColor" strokeWidth="1.8" /><path d="M11 28h10M16 23v5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" /></svg>;
}

function WindowsIcon() {
  return <svg viewBox="0 0 32 32" fill="currentColor" aria-hidden="true"><path d="M3 5h12v10H3zm14 0h12v10H17zM3 17h12v10H3zm14 0h12v10H17z" /></svg>;
}

export default App;
