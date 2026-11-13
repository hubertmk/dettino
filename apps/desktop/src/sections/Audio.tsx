import { useEffect, useState } from "react";
import { Mic } from "lucide-react";
import { api, DeviceView, isMac, MicEarlyOpen, MicMode, TestResult } from "../lib/api";
import { APP_NAME } from "../lib/brand";
import { useStore } from "../lib/store";
import { Banner, Meter, ms, Row, Toggle } from "../components/ui";

export function useMicLevel(active: boolean) {
  const [level, setLevel] = useState(0);
  useEffect(() => {
    if (!active) return;
    let smooth = 0;
    let on = false;
    const start = () => {
      if (on) return;
      on = true;
      api.micMonitor(true, (f) => {
        smooth = smooth * 0.6 + f.level * 0.4;
        setLevel(smooth);
      });
    };
    const stop = () => {
      if (!on) return;
      on = false;
      api.micMonitor(false, () => {});
      setLevel(0);
    };
    // Closing the settings window only hides it; the microphone must not stay open
    // behind a hidden window.
    const follow = () => (document.visibilityState === "hidden" ? stop() : start());
    follow();
    document.addEventListener("visibilitychange", follow);
    return () => {
      document.removeEventListener("visibilitychange", follow);
      stop();
    };
  }, [active]);
  return level;
}

export function DevicePicker() {
  const { snap, update } = useStore();
  const [devices, setDevices] = useState<DeviceView[]>([]);
  useEffect(() => {
    api.listDevices().then(setDevices);
  }, []);
  return (
    <select className="select" value={snap.settings.input_device ?? ""} onChange={(e) => update((x) => (x.input_device = e.target.value || null))} aria-label="Microphone">
      <option value="">System default{devices.find((d) => d.is_default) ? ` (${devices.find((d) => d.is_default)!.name})` : ""}</option>
      {devices.map((d) => (
        <option key={d.name} value={d.name}>
          {d.name}{d.kind === "bluetooth" ? " — Bluetooth, may clip the first word" : d.kind === "virtual" ? " — virtual" : ""}
        </option>
      ))}
    </select>
  );
}

export function MicTest() {
  const level = useMicLevel(true);
  const [res, setRes] = useState<TestResult | "running" | null>(null);
  return (
    <div className="stack">
      <Meter level={level} />
      <div className="hstack">
        <button className="btn" disabled={res === "running"} onClick={async () => { setRes("running"); setRes(await api.micCheck()); }}>
          <Mic /> {res === "running" ? "Listening for 3 seconds…" : "Test: say a sentence"}
        </button>
        {res === "running" && <span className="small muted">Say: "Testing, one two three — can you hear me clearly?"</span>}
      </div>
      {res && res !== "running" && (res.ok ? <Banner kind="ok">Heard: "{res.text}" · {ms(res.ms)}{res.detail ? ` · ${res.detail}` : ""}</Banner> : <Banner kind="warn">{res.error}</Banner>)}
    </div>
  );
}

export function Audio() {
  const { snap, update } = useStore();
  const s = snap.settings;
  const ctrl = isMac() ? "Control" : "Ctrl";
  return (
    <div className="page">
      <div className="page-head">
        <h1>Microphone</h1>
        <p>Which microphone to use and how {APP_NAME} keeps the first word from being cut off.</p>
      </div>
      <div className="card">
        <Row title="Microphone" desc="Follows the system default unless you pick one.">
          <DevicePicker />
        </Row>
        <Row title="Prefer wired or built-in over Bluetooth" desc="Bluetooth headsets switch to a low-quality mode while recording and can clip the first word.">
          <Toggle label="Prefer wired" on={s.prefer_wired} onChange={(v) => update((x) => (x.prefer_wired = v))} />
        </Row>
        <div className="card-pad" style={{ borderTop: "1px solid var(--border)" }}>
          <MicTest />
        </div>
      </div>
      <div className="card">
        <div className="card-head"><h3>Never miss the first word</h3></div>
        <Row title="Keep microphone ready" desc={`Keeps the mic open so the first syllable is never cut. ${isMac() ? "macOS shows its orange microphone dot while the mic is open; “Only while dictating” opens it when you start dictating and closes it right after." : "Windows shows the mic as in use."} Audio older than the pre-roll is discarded and never leaves memory unless you are dictating.`}>
          <select className="select" aria-label="Keep microphone ready" value={s.mic_mode} onChange={(e) => update((x) => (x.mic_mode = e.target.value as MicMode))}>
            <option value="always_warm">Always{isMac() ? "" : " (recommended)"}</option>
            <option value="warm_on_modifier">When I press {isMac() ? "Control" : "Ctrl"} (2 min)</option>
            <option value="on_demand">Only while dictating{isMac() ? " (recommended)" : ""}</option>
          </select>
        </Row>
        {s.mic_mode === "on_demand" && (
          <Row title="Open the microphone early" desc={`Opening when you press ${ctrl} saves your first syllable, but ${isMac() ? "lights the orange microphone dot" : "shows the mic as in use"} on every ${ctrl} shortcut.`}>
            <select className="select" aria-label="Open the microphone early" value={s.mic_early_open} onChange={(e) => update((x) => (x.mic_early_open = e.target.value as MicEarlyOpen))}>
              <option value="off">No (recommended)</option>
              <option value="close_on_other_key">When I press {ctrl}, close on other shortcuts</option>
              <option value="hold">When I press {ctrl} (3 s)</option>
            </select>
          </Row>
        )}
        <Row title="Pre-roll" desc="Audio from just before you pressed the shortcut that is included.">
          <div className="hstack">
            <input type="range" min={0} max={2000} step={100} value={s.preroll_ms} onChange={(e) => update((x) => (x.preroll_ms = Number(e.target.value)))} aria-label="Pre-roll" />
            <span className="small" style={{ width: 52 }}>{(s.preroll_ms / 1000).toFixed(1)} s</span>
          </div>
        </Row>
        {s.mic_mode === "on_demand" && s.mic_early_open === "off" && <div className="card-pad"><Banner kind="warn">With "Only while dictating", the first 100–300 ms (up to 1.5 s on Bluetooth) can be lost.</Banner></div>}
      </div>
    </div>
  );
}
