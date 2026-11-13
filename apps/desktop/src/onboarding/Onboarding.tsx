import { useEffect, useRef, useState } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, ExternalLink, Mic } from "lucide-react";
import { api, formatBytes, isMac, pillWord, Preset, ProviderConfig } from "../lib/api";
import { useStore } from "../lib/store";
import { useSetup } from "../lib/setupJob";
import { providerNeeds } from "../lib/setup";
import { autostartProblem, locationWarning, startupFailure, startupSettled } from "../lib/readiness";
import { APP_NAME, keychainName, osName } from "../lib/brand";
import { Banner, Chord, Meter, ms, Toggle } from "../components/ui";
import { PermissionRows } from "../components/Permissions";
import { CloudCleanupPick, PresetCards } from "../sections/Providers";
import { SetupCard } from "./SetupCard";
import { DevicePicker, useMicLevel } from "../sections/Audio";

const STEPS = ["Welcome", "Microphone", "Mode", "Install", "Ready"];
const INSTALL = 3;
const READY = 4;
/** macOS asks for more than the microphone: the second step covers every permission. */
const stepName = (i: number) => (i === 1 && isMac() ? "Permissions" : STEPS[i]);
/** How long the finished install card stays on screen before the wizard moves on by itself. */
const ADVANCE_AFTER_MS = 900;

/** Onboarding runs in a compact window; settings get the full size back. */
async function sizeWindow(onboarding: boolean) {
  try {
    const w = getCurrentWindow();
    if (onboarding) {
      await w.setMinSize(new LogicalSize(720, 520));
      await w.setSize(new LogicalSize(720, 520));
    } else {
      await w.setSize(new LogicalSize(1040, 700));
      await w.setMinSize(new LogicalSize(820, 560));
    }
    await w.center();
  } catch {
    // Not in a Tauri window (mock build): keep the current size.
  }
}

/** Enter on these targets means "use this control", not "next step". */
const OWN_ENTER = new Set(["TEXTAREA", "INPUT", "SELECT", "BUTTON", "A"]);

export function Onboarding({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState(Number(new URLSearchParams(location.search).get("step") ?? 0));
  const { snap, setSnap } = useStore();
  const job = useSetup();
  // Whether the user came to the install step going forward (Continue) rather than Back.
  const forward = useRef(true);
  const next = () => {
    forward.current = true;
    setStep((s) => Math.min(STEPS.length - 1, s + 1));
  };
  const back = () => {
    forward.current = false;
    setStep((s) => Math.max(0, s - 1));
  };
  // Install: the keys are saved, everything local is downloaded, unpacked and running
  // (or it says why it cannot, which still lets you go on).
  const keysMissing = providerNeeds(snap).keyProviders.some((p) => !snap.keys[p.id]);
  const installing = job.plan.length > 0 && !job.ready;
  const starting = job.ready && !startupSettled(snap.readiness);
  const blocked = step === INSTALL && (keysMissing || installing || starting);
  const blockedWhy = keysMissing
    ? "Save your API key to continue"
    : job.state === "running"
      ? "Continue unlocks when everything is installed and running"
      : job.state === "error"
        ? "Click Try again to continue"
        : installing
          ? job.checked
            ? "Click Install everything to continue"
            : "Checking what's installed…"
          : "Starting everything up…";
  const advance = () => {
    if (blocked) return;
    next();
  };
  const finish = async () => {
    // The backend hides the window and, when everything runs, says "Pronto!" on the pill.
    setSnap(await api.finishOnboarding());
    await sizeWindow(false);
    onDone();
  };
  const body = useRef<HTMLDivElement>(null);

  useEffect(() => {
    sizeWindow(true);
  }, []);

  // Arriving at the install step starts the downloads: Continue on the mode step was
  // the consent (the note there says so). Nothing starts on a page the user did not reach.
  useEffect(() => {
    if (step === INSTALL && job.checked && !job.ready && job.state === "idle" && job.plan.length > 0) job.start();
  }, [step, job.checked, job.ready, job.state, job.plan.length]);

  // Everything installed and running: move on to the Ready page by itself.
  useEffect(() => {
    if (step !== INSTALL || !forward.current || !job.ready || !snap.readiness.all) return;
    const t = setTimeout(next, ADVANCE_AFTER_MS);
    return () => clearTimeout(t);
  }, [step, job.ready, snap.readiness.all]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement;
      if (OWN_ENTER.has(el.tagName) || el.getAttribute("role") === "switch" || el.isContentEditable) return;
      if (e.key !== "Enter") return;
      if (step < READY) advance();
      else finish();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // Move focus to the new step's heading so screen readers announce it.
  useEffect(() => {
    body.current?.querySelector<HTMLElement>("h1")?.focus();
  }, [step]);

  return (
    <div className="ob">
      <div className="ob-top">
        <span>Step {step + 1} of {STEPS.length} · {stepName(step)}</span>
        <div className="ob-steps" aria-hidden="true">
          {STEPS.map((_, i) => <i key={i} className={i <= step ? "done" : ""} />)}
        </div>
      </div>
      <div className="ob-body">
        <div className="ob-inner" key={step} ref={body}>
          {step === 0 && <Welcome />}
          {step === 1 && <MicStep />}
          {step === 2 && <ModeStep />}
          {step === 3 && <InstallStep />}
          {step === 4 && <ReadyStep />}
        </div>
      </div>
      <div className="ob-foot">
        {step === 0 ? <button className="btn ghost" onClick={finish}>Skip setup</button> : <span />}
        <div className="hstack">
          {blocked && <span className="small muted">{blockedWhy}</span>}
          {step > 0 && <button className="btn" onClick={back}>Back</button>}
          {step < READY ? (
            <button className="btn primary" onClick={advance} disabled={blocked}>
              {step === 0 ? "Get started" : "Continue"}
            </button>
          ) : (
            <button className="btn primary" onClick={finish}>Finish</button>
          )}
        </div>
      </div>
    </div>
  );
}

function Welcome() {
  const { snap } = useStore();
  const where = locationWarning(snap.app_location);
  return (
    <div className="ob-welcome">
      <div className="stack" style={{ gap: 12 }}>
        <h1 tabIndex={-1}>Hold a key. Say it. Let go.</h1>
        <p className="lead">
          {APP_NAME} types it where your cursor is, cleaned up, in any app. No account. Nothing leaves this computer unless you choose a cloud provider.
        </p>
        {where && (
          <Banner kind="warn">
            {where}{" "}
            <button type="button" className="linkish" onClick={() => api.openDataFolder("applications")}>
              Open Applications <ExternalLink size={11} />
            </button>
          </Banner>
        )}
      </div>
      <div className="demo" role="img" aria-label="Demo: while the shortcut is held, spoken words are typed into the text field">
        <div className="demo-field" aria-hidden="true">
          <span className="typed">Let's meet at three on Thursday.</span>
          <span className="caret" />
        </div>
        {snap.settings.pill_style === "line" ? (
          <div className="hero-line" aria-hidden="true">
            <svg viewBox="0 0 56 12" width="56" height="12">
              <path d="M1 6 C 8 6, 11 6, 14 6 S 20 6, 22 4.5 S 26 7.5, 28 6 S 32 4.8, 34 6 S 40 6, 42 6 S 52 6, 55 6" />
            </svg>
          </div>
        ) : (
          <div className="hero-pill" aria-hidden="true">
            <Mic size={16} color="var(--accent)" />
            <span className="bars" style={{ display: "inline-flex", gap: 2, alignItems: "center" }}>
              {[0.45, 0.7, 0.9, 1, 0.9, 0.7, 0.45].map((w, i) => (
                <i key={i} style={{ width: 3, height: 18 * w, borderRadius: 2, background: "var(--accent)", display: "block" }} />
              ))}
            </span>
          </div>
        )}
        <span className="small muted" aria-hidden="true"><Chord c={snap.settings.hotkeys.push_to_talk} /> held</span>
      </div>
    </div>
  );
}

function MicStep() {
  const level = useMicLevel(true);
  const [heard, setHeard] = useState(false);
  useEffect(() => {
    if (level > 0.08) setHeard(true);
  }, [level]);
  const mac = isMac();
  return (
    <>
      <h1 tabIndex={-1}>{mac ? "Permissions" : "Your microphone"}</h1>
      <p className="lead">
        {mac
          ? `${APP_NAME} needs the microphone, and Accessibility to see your shortcut and paste. Nothing else.`
          : `${APP_NAME} needs the microphone. It doesn't need administrator rights or access to anything else.`}
      </p>
      {mac && (
        <div className="card card-pad">
          <PermissionRows />
        </div>
      )}
      <div className="card card-pad stack">
        <div className="hstack" style={{ justifyContent: "space-between" }}>
          <b>Microphone</b>
          {heard ? <span className="chip ok"><Check /> Working</span> : <span className="chip warn">Say something to check it</span>}
        </div>
        <DevicePicker />
        <Meter level={level} />
        {!heard && (
          <p className="small muted">
            Nothing moving? {mac ? "macOS" : "Windows"} may be blocking microphone access.{" "}
            <button type="button" className="linkish" onClick={() => api.openPermissionSettings("microphone")}>
              Open {mac ? "System Settings" : "Windows Settings"} <ExternalLink size={11} />
            </button>
          </p>
        )}
      </div>
      {!mac && <Banner>Apps that run as administrator can't receive pasted text from a normal app — {APP_NAME} tells you when that happens and copies the text instead.</Banner>}
    </>
  );
}

function ModeStep() {
  const { snap, setSnap } = useStore();
  const job = useSetup();
  const bytes = job.plan.filter((s) => !s.done).reduce((n, s) => n + s.bytes, 0);
  return (
    <>
      <h1 tabIndex={-1}>How should {APP_NAME} think?</h1>
      <p className="lead">You can change this any time, or pair any providers yourself in Settings → Providers.</p>
      <PresetCards value={snap.settings.preset} onChange={async (p: Preset) => setSnap(await api.setPreset(p))} gpus={snap.gpus} recommended="private" />
      <p className="small muted">
        Detected: {snap.gpus.length ? snap.gpus.map((g) => `${g.name}${g.vram_mb ? ` (${Math.round(g.vram_mb / 1024)} GB)` : ""}`).join(", ") : "no dedicated GPU"}
        {snap.ram_mb > 0 ? ` · ${Math.round(snap.ram_mb / 1024)} GB RAM` : ""} · {snap.cpu_threads} CPU threads.
        On-device transcription runs well on this computer either way.
      </p>
      {job.checked && bytes > 0 && job.state === "idle" && (
        <Banner>This mode downloads about {formatBytes(bytes)} to this computer. It starts when you click Continue; the next step shows the progress and waits until everything is installed and running.</Banner>
      )}
    </>
  );
}

function KeyField({ p }: { p: ProviderConfig }) {
  const { snap, setSnap } = useStore();
  const [key, setKey] = useState("");
  const [state, setState] = useState<"idle" | "checking" | "ok" | "err">(snap.keys[p.id] ? "ok" : "idle");
  const [msg, setMsg] = useState<string>("");
  const links: Record<string, string> = {
    openai_batch: "https://platform.openai.com/api-keys",
    openai_chat: "https://platform.openai.com/api-keys",
    anthropic: "https://console.anthropic.com/settings/keys",
    perplexity_agent: "https://www.perplexity.ai/settings/api",
  };
  const save = async (k: string) => {
    if (!k.trim()) return;
    setState("checking");
    setSnap(await api.setProviderKey(p.id, k));
    if (p.type === "openai_batch") {
      setState("ok");
      setMsg("Saved. It's used from your first dictation on.");
      return;
    }
    const r = await api.testProvider(p.id);
    setState(r.ok ? "ok" : "err");
    setMsg(r.ok ? `Key works · ${ms(r.ms)}` : r.error ?? "Didn't work");
  };
  return (
    <div className="field">
      <label htmlFor={`key-${p.id}`}>{p.display_name} API key</label>
      <div className="hstack">
        <input
          id={`key-${p.id}`}
          className="input"
          style={{ flex: 1 }}
          type="password"
          placeholder={snap.keys[p.id] ? "Saved — paste to replace" : "Paste your key"}
          value={key}
          onChange={(e) => setKey(e.target.value)}
          onPaste={(e) => {
            const v = e.clipboardData.getData("text");
            setTimeout(() => save(v), 0);
          }}
        />
        <button className="btn" onClick={() => save(key)} disabled={!key.trim() || state === "checking"}>{state === "checking" ? "Checking…" : "Save"}</button>
        {state === "ok" && <span className="chip ok"><Check /> Saved</span>}
      </div>
      <span className="small" style={{ color: state === "err" ? "var(--danger)" : "var(--text-2)" }}>
        {msg || <>Stored in the system keychain. <button type="button" className="linkish" onClick={() => openUrl(links[p.type])}>Get a key</button></>}
      </span>
    </div>
  );
}

function InstallStep() {
  const { snap } = useStore();
  const s = snap.settings;
  const job = useSetup();
  const provider = (id: string) => s.providers.find((p) => p.id === id)!;
  const { keyProviders } = providerNeeds(snap);
  const local = job.plan.length > 0;
  const fallbacks = [...s.asr_chain.slice(1), ...s.llm_chain.slice(1)].map(provider).filter(Boolean);
  const title =
    s.preset === "private" ? `Installing what ${APP_NAME} needs` : s.preset === "fast" ? "Your API key" : local ? "Speech model and cleanup key" : "Set up your providers";
  return (
    <>
      <h1 tabIndex={-1}>{title}</h1>
      <p className="lead">
        {s.preset === "private" && `Everything runs on this computer: the speech model, Ollama and the cleanup model. Nothing else to do here — this takes a while.`}
        {s.preset === "fast" && `Paste your API key. It is stored in the ${keychainName(snap.platform)} and sent only to that provider.`}
        {s.preset === "balanced" && "The speech model is downloaded once; add a cloud key for the cleanup."}
        {s.preset === "custom" && "Set up the providers you picked."}
      </p>
      {local && <SetupCard />}
      {s.preset === "fast" && <div className="card"><CloudCleanupPick /></div>}
      {keyProviders.length > 0 && (
        <div className="card card-pad stack">
          {keyProviders.map((p) => <KeyField key={p.id} p={p} />)}
        </div>
      )}
      {s.preset === "private" ? (
        <p className="small muted">Nothing goes to the cloud unless you add a cloud provider yourself, later, in Settings.</p>
      ) : (
        fallbacks.length > 0 && (
          <p className="small muted">
            Optional fallbacks: {fallbacks.map((p) => `${p.display_name}${snap.ready[p.id] ? " (ready)" : ""}`).join(", ")}. Set them up later in Settings → Providers; until then they are skipped.
          </p>
        )
      )}
    </>
  );
}

function ReadyStep() {
  const { snap, update } = useStore();
  const h = snap.settings.hotkeys;
  const pill = pillWord(snap.settings.pill_style);
  // Start-up ended with a problem: the install step let the user through with it.
  const trouble = snap.readiness.all ? null : startupFailure(snap.readiness);
  // The switch registers the login item at once; the OS may say no (not in Applications, switched off).
  const autostart = autostartProblem(snap.autostart, snap.platform);
  return (
    <>
      <h1 tabIndex={-1}>Pronto! {APP_NAME} is ready.</h1>
      <p className="lead">
        Click into any text field — Word, an email, a chat — hold <Chord c={h.push_to_talk} />, say a sentence, and let go. The words appear where your cursor is.
      </p>
      {trouble && (
        <Banner kind="warn">
          {trouble} Dictation still works; the text is pasted as heard. Settings → Hearing &amp; cleanup has the fix.
        </Banner>
      )}
      <div className="card">
        <div className="card-head"><h3>Good to know</h3></div>
        <div className="row"><Mic size={18} color="var(--accent)" /><span className="grow">Hold <Chord c={h.push_to_talk} />, talk, release. <Chord c={h.hands_free} /> starts hands-free; Space finishes it.</span></div>
        <div className="row"><span style={{ width: 18 }} /><span className="grow"><Chord c={h.undo_ai} /> undoes what the AI changed.</span></div>
        <div className="row"><span style={{ width: 18 }} /><span className="grow">The {pill} at the bottom of your screen is {APP_NAME}. Hover it to pause or open Settings, where you can also change the shortcuts.</span></div>
        <div className="row">
          <span style={{ width: 18 }} />
          <span className="grow">Start {APP_NAME} with {osName(snap.platform)}</span>
          <Toggle label={`Start with ${osName(snap.platform)}`} on={snap.settings.launch_at_login} onChange={(v) => update((x) => (x.launch_at_login = v))} />
        </div>
        {autostart && snap.settings.onboarding_done && (
          <div className="row"><span style={{ width: 18 }} /><span className="grow small" style={{ color: "var(--warning)" }}>{autostart}</span></div>
        )}
      </div>
      <p className="small muted">
        Finish closes this window. {APP_NAME} keeps running as the {pill}; click the {APP_NAME} icon in the {isMac() ? "menu bar" : "system tray"} to open Settings.
      </p>
    </>
  );
}
