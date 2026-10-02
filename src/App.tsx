import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { ShortcutInput } from "./ShortcutInput";

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
  if (delivery.status === "delivered") return "전달됨";
  if (delivery.status === "unconfirmed") return "전달됐으나 확인 불가";
  return `실패: ${delivery.reason}`;
}

function parseCode(raw: string): number | null {
  const trimmed = raw.trim();
  if (!/^\d{1,3}$/.test(trimmed)) return null;
  const value = Number(trimmed);
  if (value > 255) return null;
  return value;
}

function savedCode(value: number | null): string {
  return value === null ? "없음" : String(value);
}

function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
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
          return reloaded;
        } catch {
          return onDisk;
        }
      }
    });
    saveQueue.current = task;
    return task;
  }

  useEffect(() => {
    const pauseCapture = () => { void captureShortcuts(false); };
    const resumeCapture = () => {
      if (document.activeElement?.matches("[data-shortcut-input]")) void captureShortcuts(true);
    };
    window.addEventListener("blur", pauseCapture);
    window.addEventListener("focus", resumeCapture);
    void invoke<Settings>("get_settings").then(setSettings).catch((error: unknown) => {
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
      <main>
        {notice ? <p className="notice">{notice}</p> : null}
        설정을 불러오는 중입니다.
      </main>
    );
  }

  async function switchTo(destination: Destination) {
    try {
      const outcomes = await invoke<Outcome[]>("switch_to", { destination });
      setNotice(outcomes.map(outcomeText).join("\n"));
    } catch (error) {
      setNotice(String(error));
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
      setNotice(outcomeText(outcome));
    } catch (error) {
      setNotice(String(error));
    }
  }

  return (
    <main>
      <p className="notice">{notice}</p>
      <div className="row">
        <button type="button" onClick={() => void switchTo("mac")}>맥으로</button>
        <button type="button" onClick={() => void switchTo("windows")}>Windows로</button>
        <button type="button" disabled={refreshingMonitors} onClick={() => void refreshMonitors()}>
          {refreshingMonitors ? "찾는 중…" : "모니터 새로고침"}
        </button>
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
          <p>HDMI {savedCode(role(index).hdmiCode)} DP {savedCode(role(index).dpCode)}</p>
          <div className="row">
            <input
              type="number"
              min={0}
              max={255}
              value={codes[index]}
              onChange={(event) => setCodes({ ...codes, [index]: event.target.value })}
            />
            <button type="button" onClick={() => void trial(index)}>이 번호로 시험</button>
            <button type="button" onClick={() => void saveCode(index, "hdmiCode")}>HDMI로 저장</button>
            <button type="button" onClick={() => void saveCode(index, "dpCode")}>DP로 저장</button>
            <button type="button" onClick={() => void invoke<number>("read_input", { id: role(index).id }).then((value) => setNotice(`현재 번호 ${value}`)).catch((error: unknown) => setNotice(String(error)))}>현재 번호 읽기</button>
          </div>
        </section>
      ))}
      <div
        className="shortcut-fields"
        onFocus={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget)) void captureShortcuts(true);
        }}
        onBlur={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget)) void captureShortcuts(false);
        }}
      >
      <label>
        맥으로 단축키
        <ShortcutInput value={settings.hotkeys.toMac} ready={shortcutCaptureReady} onChange={(value) => setSettings((current) => current && ({ ...current, hotkeys: { ...current.hotkeys, toMac: value } }))} />
      </label>
      <label>
        Windows로 단축키
        <ShortcutInput value={settings.hotkeys.toWindows} ready={shortcutCaptureReady} onChange={(value) => setSettings((current) => current && ({ ...current, hotkeys: { ...current.hotkeys, toWindows: value } }))} />
      </label>
      <small id="shortcut-help">칸을 선택하고 키 조합을 누르세요. Ctrl·Alt·Shift·Command와 일반 키 하나를 함께 사용할 수 있습니다. Esc로 입력 종료, Tab으로 이동합니다.</small>
      </div>
      <label>
        <input type="checkbox" checked={settings.launchAtLogin} onChange={(event) => setSettings({ ...settings, launchAtLogin: event.target.checked })} />
        로그인 시 실행
      </label>
      <button type="button" onClick={() => void saveDisplayedSettings()}>설정 저장</button>
    </main>
  );
}

export default App;
