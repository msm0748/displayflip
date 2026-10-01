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
