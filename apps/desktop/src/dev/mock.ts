// Dev-only Tauri bridge mock for rendering the UI in a plain browser (visual QA,
// screenshots). Loaded only when import.meta.env.DEV and the URL has ?mock.
import defaults from "./defaults.json";

type Cb = (payload: unknown) => void;
const callbacks = new Map<number, Cb>();
let nextId = 1;
const params = new URLSearchParams(location.search);
const scenario = params.get("mock") ?? "ready";
const now = Date.now();

const settings = structuredClone(defaults) as any;
settings.onboarding_done = scenario !== "onboarding";
settings.dictionary = [
  { term: "Lindqvist", replace_from: "lind quest", source: "manual", enabled: true },
  { term: "Tauri", replace_from: null, source: "auto", enabled: true },
];
if (scenario === "empty") settings.dictionary = [];
// `&preset=fast|balanced|private` previews that mode (chains as `preset_chains` + `follow_keys` with one OpenAI key).
const CHAINS: Record<string, [string[], string[]]> = {
  private: [["parakeet", "whisper"], ["ollama", "lmstudio"]],
  fast: [["openai-asr", "custom-asr"], ["openai-chat", "anthropic", "perplexity"]],
  balanced: [["parakeet", "whisper", "openai-asr"], ["anthropic", "ollama"]],
};
const presetParam = params.get("preset");
if (presetParam && CHAINS[presetParam]) {
  settings.preset = presetParam;
  [settings.asr_chain, settings.llm_chain] = CHAINS[presetParam];
}

const record = (i: number, over: Record<string, unknown> = {}) => ({
  id: `r${i}`,
  created_at: now - i * 7 * 60_000,
  status: "ok",
  mode: "dictation",
  raw_text: "um so let's meet at two actually three on thursday to go over the uh the budget",
  cleaned_text: "Let's meet at three on Thursday to go over the budget.",
  final_text: "Let's meet at three on Thursday to go over the budget.",
  edited_text: null,
  zero_edit: true,
  cleanup_level: "light",
  app: "slack.exe",
  window_title: "Slack — #design",
  url: null,
  text_before: null,
  selection: null,
  audio_path: "C:/audio/x.wav",
  audio_ms: 4200,
  error: null,
  timings: {
    hold_ms: 4400, preroll_used_ms: 1500, audio_ms: 5900, speech_ms: 3900, asr_provider: "parakeet", asr_ms: 231,
    asr_hedged: false, asr_attempts: ["parakeet 231 ms"], context_ms: 34, context_timed_out: false,
    cleanup_provider: "ollama", cleanup_model: "gemma4:e4b-it-qat", cleanup_ms: 305, cleanup_skipped: null,
    cleanup_rejected: null, insert_ms: 48, insert_verify: "clipboard read", clipboard_restored: true, total_ms: 842,
  },
  ...over,
});

const history =
  scenario === "empty"
    ? []
    : [
        record(0),
        record(1, { status: "ok_raw", final_text: "So the build is like broken because of the cache.", cleaned_text: "So the build is like broken because of the cache.", raw_text: "so the build is like broken because of the cache", timings: { ...record(0).timings, cleanup_skipped: "The cleanup changed 62 % of your words, more than the Light limit.", cleanup_ms: 540, total_ms: 790 } }),
        record(2, { status: "held", app: "code.exe", error: "The cursor moved before we could paste" }),
        record(3, { status: "failed", raw_text: null, final_text: null, cleaned_text: null, error: "OpenAI rejected the key", timings: { ...record(0).timings, total_ms: null } }),
        record(4, { mode: "command", raw_text: "make this more formal", final_text: "Could you please send me the report by Friday?" }),
      ];

// `&hw=gpu24|gpu8|gpu6|cpu32|cpu8` picks the detected hardware; `&ollama=running|missing|stopped|installing|pulling`
// the state of the local cleanup setup (`installing` also presses "Install everything" in the onboarding);
// `&pull=<tag>` which model the pulling scenario downloads; `&disk=<MB>` the free disk space.
const HW: Record<string, { gpus: unknown[]; ram_mb: number; rec: Record<string, unknown> }> = {
  gpu24: {
    gpus: [{ name: "NVIDIA RTX PRO 5000 Blackwell Generation Laptop GPU", vram_mb: 24463, vendor: "NVIDIA" }],
    ram_mb: 65536,
    rec: { model: "gemma4:e2b-it-qat", reason: "Your NVIDIA GPU has 24 GB of memory: the cleanup model answers in about half a second.", slow_cpu: false, tier: "gpu" },
  },
  gpu8: {
    gpus: [{ name: "NVIDIA GeForce RTX 4060", vram_mb: 8191, vendor: "NVIDIA" }],
    ram_mb: 32768,
    rec: { model: "gemma4:e2b-it-qat", reason: "Your NVIDIA GPU has 8 GB of memory: the cleanup model answers in about half a second.", slow_cpu: false, tier: "gpu" },
  },
  gpu6: {
    gpus: [{ name: "NVIDIA GeForce GTX 1660", vram_mb: 6144, vendor: "NVIDIA" }],
    ram_mb: 16384,
    rec: { model: "gemma4:e2b-it-qat", reason: "Your NVIDIA GPU has 6 GB of memory: the cleanup model answers in about half a second.", slow_cpu: false, tier: "gpu" },
  },
  cpu32: { gpus: [], ram_mb: 32768, rec: { model: "gemma4:e2b-it-qat", reason: "No NVIDIA GPU: the cleanup model runs on the processor in a few seconds.", slow_cpu: false, tier: "cpu" } },
  cpu8: {
    gpus: [],
    ram_mb: 8192,
    rec: { model: "gemma4:e2b-it-qat", reason: "No NVIDIA GPU and 8 GB of RAM: the cleanup model fits, but cleanup may take several seconds. Dettino waits for it.", slow_cpu: true, tier: "cpu_low_ram" },
  },
};
const hw = HW[params.get("hw") ?? "gpu24"] ?? HW.gpu24;
const CATALOG = [
  { tag: "gemma4:e2b-it-qat", name: "Gemma 4 E2B", download_bytes: 4_300_000_000, memory_mb: 2500, cpu_note: "Several seconds per cleanup on the processor", gpu_note: "About half a second on an NVIDIA GPU, faster on large cards" },
];
const ollamaScenario = params.get("ollama") ?? (scenario === "onboarding" ? "running" : "running");
const ollama = {
  installed: ollamaScenario !== "missing" && ollamaScenario !== "installing",
  running: ollamaScenario === "running" || ollamaScenario === "pulling",
  models: ["pulling", "missing", "installing"].includes(ollamaScenario) ? [] : scenario === "onboarding" ? ["gemma4:e2b-it-qat"] : ["gemma4:e4b-it-qat", "qwen3.8:27b", "deepseek-r1:8b"],
};
const ollamaStatus = () => ({ installed: ollama.installed, reachable: ollama.running, models: ollama.running ? ollama.models : [], version: ollama.running ? "0.40.3" : null, can_install: true });

// `&ready=warming|failed` previews the start-up row once everything is installed.
type MockStage = { state: "not_needed" | "missing" | "warming" | "ready" } | { state: "failed"; error: string };
const WARM_TIMEOUT = "The cleanup model did not answer within 90 seconds.";
const readiness: { speech: MockStage; ollama: MockStage; cleanup: MockStage; all: boolean } = {
  speech: { state: scenario === "onboarding" && !params.has("installed") ? "missing" : "ready" },
  ollama: ollama.running ? { state: "ready" } : ollama.installed ? { state: "failed", error: "Ollama is installed but not answering" } : { state: "missing" },
  // Outside the onboarding the app would have picked one of the installed models.
  cleanup: ollama.running && (scenario !== "onboarding" || ollama.models.includes("gemma4:e2b-it-qat")) ? { state: "ready" } : { state: "missing" },
  all: false,
};
if (settings.preset === "fast") Object.assign(readiness, { speech: { state: "ready" }, ollama: { state: "not_needed" }, cleanup: { state: params.has("openai_key") ? "ready" : "missing" } });
function settle() {
  readiness.all = [readiness.speech, readiness.ollama, readiness.cleanup].every((s) => s.state === "ready" || s.state === "not_needed");
}
settle();
if (params.get("ready") === "warming") Object.assign(readiness, { speech: { state: "ready" }, ollama: { state: "ready" }, cleanup: { state: "warming" }, all: false });
if (params.get("ready") === "failed") Object.assign(readiness, { speech: { state: "ready" }, ollama: { state: "ready" }, cleanup: { state: "failed", error: WARM_TIMEOUT }, all: false });
/** A stage loads, then (unless `&still`, or `&ready=failed` for the cleanup model) is ready a moment later. */
function warm(which: "speech" | "cleanup", ms = 1500) {
  readiness[which] = { state: "warming" };
  settle();
  emit("readiness", { ...readiness });
  if (params.has("still")) return;
  setTimeout(() => {
    readiness[which] = which === "cleanup" && params.get("ready") === "failed" ? { state: "failed", error: WARM_TIMEOUT } : { state: "ready" };
    settle();
    emit("readiness", { ...readiness });
  }, ms);
}
function ollamaUp() {
  readiness.ollama = { state: "ready" };
  settle();
  emit("readiness", { ...readiness });
}

const listeners = new Map<string, number[]>();
function emit(event: string, payload: unknown) {
  for (const id of listeners.get(event) ?? []) callbacks.get(id)?.({ event, id, payload });
}
/** Ticks a fake download; with `&still` it stops at 40 % so screenshots are stable. */
function fakeProgress(id: string, total: number, stage: "downloading", detail?: string): Promise<void> {
  return new Promise((resolve) => {
    let done = 0;
    const t = setInterval(() => {
      done = Math.min(total, done + total / 25);
      if (params.has("still")) done = total * 0.4;
      emit("model:progress", { id, done, total, stage, detail: detail ?? null });
      if (params.has("still")) return clearInterval(t);
      if (done >= total) {
        clearInterval(t);
        resolve();
      }
    }, 300);
  });
}

// Speech models start missing in the onboarding scenario (`&installed` keeps them); `model_download` installs them.
const installedModels = new Set(["silero-vad", ...(scenario === "onboarding" && !params.has("installed") ? [] : ["parakeet-tdt-0.6b-v2-int8"])]);

const snapshot = () => ({
  settings,
  keys: { anthropic: scenario !== "onboarding", "openai-asr": params.has("openai_key"), perplexity: false, "openai-chat": params.has("openai_key") },
  ready: { parakeet: true, whisper: false, "openai-asr": false, "custom-asr": true, anthropic: scenario !== "onboarding", "openai-chat": false, perplexity: false, ollama: true, lmstudio: true },
  stats: {
    parakeet: { count: 30, p50_ms: 228, p95_ms: 260, last_error: null, last_error_ts: null, recent_ms: [240, 231, 225, 228, 260, 219, 233, 230, 241, 226] },
    ollama: { count: 30, p50_ms: 561, p95_ms: 780, last_error: null, last_error_ts: null, recent_ms: [560, 590, 541, 780, 566, 554, 571, 548] },
    anthropic: { count: 4, p50_ms: 410, p95_ms: 520, last_error: "timeout", last_error_ts: now - 120_000, recent_ms: [410, 520, 390] },
  },
  models: [
    { id: "silero-vad", family: "vad", name: "Silero voice detection", description: "Detects when you speak, so silence is never transcribed.", url: "", bytes: 643854, license: "MIT", dir: "vad", archive: false, english_only: false, recommended: true, installed: installedModels.has("silero-vad") },
    { id: "parakeet-tdt-0.6b-v2-int8", family: "parakeet", name: "Parakeet TDT 0.6B v2", description: "Fastest and most accurate for English. Runs on any modern CPU.", url: "", bytes: 482440461, license: "CC-BY-4.0 (NVIDIA)", dir: "parakeet", archive: true, english_only: true, recommended: true, installed: installedModels.has("parakeet-tdt-0.6b-v2-int8") },
    { id: "whisper-turbo", family: "whisper", name: "Whisper Large v3 Turbo", description: "Robust with accents and noise; slower than Parakeet on CPU.", url: "", bytes: 563790207, license: "MIT (OpenAI)", dir: "whisper", archive: true, english_only: false, recommended: false, installed: false },
  ],
  gpus: hw.gpus,
  cpu_threads: 24,
  ram_mb: hw.ram_mb,
  cleanup_recommendation: hw.rec,
  ollama_catalog: CATALOG,
  // `&build=dev` previews the label of a build that is not a tagged release;
  // `&expires=3` a test build with 3 days left.
  build: {
    ...(params.get("build") === "dev"
      ? { version: "0.2.0", label: "0.2.0+a1b2c3d4e.dirty", commit: "a1b2c3d4e", dirty: true, release: false, commit_date: "2026-10-30", profile: "debug" as const }
      : { version: "0.2.0", label: "0.2.0", commit: "a1b2c3d4e", dirty: false, release: true, commit_date: "2026-10-30", profile: "release" as const }),
    expires: mockExpires(params.get("expires")),
  },
  data_dir: "C:\\Users\\you\\AppData\\Local\\dev.dettino.desktop",
  hotkeys_error: null,
  history_available: true,
  fully_local: false,
  readiness: { ...readiness },
  // `&os=macos` previews the macOS wording and the permissions rows.
  platform: params.get("os") ?? "windows",
  // `&autostart=off` previews the row for a login item the OS switched off; `&location=downloads` the Applications nudge.
  autostart: params.get("autostart") === "off" ? { wanted: true, active: false, detail: null } : { wanted: true, active: true, detail: null },
  app_location: params.get("location") === "downloads" ? "elsewhere" : "applications",
});

const pillKind = params.get("pill") ?? "idle";
const pillStates: Record<string, unknown> = {
  idle: { kind: "idle" },
  paused: { kind: "idle", badge: "paused", badge_label: "Paused — click to resume" },
  listening: { kind: "listening" },
  handsfree: { kind: "listening", hands_free: true, label: "Space to finish · Esc to cancel", action: { id: "stop", label: "Stop" } },
  command: { kind: "listening", mode: "command", label: "Say how to change the selection" },
  quiet: { kind: "listening", quiet: true, label: "Can't hear you — check Logitech Webcam" },
  thinking: { kind: "thinking", since_ms: now - 1500 },
  pasted: { kind: "pasted", label: "Pasted", undo_until_ms: now + 50_000 },
  pasted_raw: { kind: "pasted_raw", label: "Pasted as heard — AI skipped", action: { id: "why:x", label: "Why?" } },
  held: { kind: "held", label: "Kept your text", action: { id: "paste_held", label: "Paste" } },
  error: { kind: "error", label: "OpenAI rejected the key", action: { id: "open:providers", label: "Fix key" } },
  blocked: { kind: "blocked", label: "Off in 1Password", action: { id: "allow:x", label: "Allow 5 min" } },
  cancelled: { kind: "cancelled", label: "Cancelled", action: { id: "restore", label: "Restore" } },
  nothing: { kind: "nothing", label: "Didn't hear anything" },
  notice: { kind: "notice", label: "Dictation is on" },
  no_mic: { kind: "idle", badge: "no_mic", badge_label: "No microphone found" },
  hotkeys_off: { kind: "idle", badge: "hotkeys_off", badge_label: "Shortcuts are not available" },
  error_dot: { kind: "idle", badge: "error", badge_label: "Last dictation failed" },
  downloading: { kind: "idle", badge: "downloading", badge_label: "Downloading Parakeet TDT 0.6B v2" },
  starting: { kind: "starting", label: "Un momento…" },
  starting_ollama: { kind: "starting", label: "Brewing the espresso…" },
  starting_pencil: { kind: "starting", label: "Sharpening the pencil…" },
  ready: { kind: "ready", label: "Pronto!" },
  cleanup_off: { kind: "idle", badge: "cleanup_off", badge_label: "Ollama didn't start — pasting without AI cleanup" },
};
// `&style=classic|line` picks the pill look; `&success=check|check_word` previews the One line's success animations.
const pillState = {
  kind: "idle", mode: "dictation", hands_free: false, quiet: false, label: null, detail: null, action: null,
  since_ms: now - 4000, seq: 1, badge: null, badge_label: null, undo_until_ms: null,
  style: params.get("style") ?? settings.pill_style ?? "line",
  success: params.get("success") ?? settings.success_animation ?? "flash", hint: "Ctrl + Space",
  ...(pillStates[pillKind] as object),
};

async function invoke(cmd: string, args: any): Promise<unknown> {
  switch (cmd) {
    case "get_state":
    case "save_settings":
    case "set_preset":
    case "set_provider_key":
    case "set_paused":
    case "finish_onboarding":
      if (cmd === "save_settings") {
        const accentChanged = args.settings.accent !== settings.accent;
        Object.assign(settings, args.settings);
        if (accentChanged) emit("accent-changed", null);
      }
      if (cmd === "set_preset") settings.preset = args.preset;
      return snapshot();
    case "history_list":
      return history;
    case "history_get":
      return history.find((h) => h.id === args.id) ?? null;
    case "suggestions_list":
      return scenario === "empty" ? [] : [{ from: "lindquist", to: "Lindqvist", ts: now }];
    case "list_devices":
      return [
        { name: "Microphone (2- Logitech Webcam C925e)", kind: "usb", is_default: true },
        { name: "Headset (WH-1000XM5)", kind: "bluetooth", is_default: false },
      ];
    case "ollama_models":
      return ollama.running ? ollama.models : [];
    case "ollama_status":
      return ollamaStatus();
    case "ollama_launch":
      await new Promise((r) => setTimeout(r, params.has("still") ? 1e9 : 1500));
      ollama.running = true;
      ollamaUp();
      return ollamaStatus();
    case "ollama_install":
      await fakeProgress("ollama:setup", 1_584_629_376, "downloading");
      emit("model:progress", { id: "ollama:setup", done: 1, total: 1, stage: "installing", detail: null });
      await new Promise((r) => setTimeout(r, 1500));
      emit("model:progress", { id: "ollama:setup", done: 1, total: 1, stage: "starting", detail: null });
      await new Promise((r) => setTimeout(r, 1000));
      ollama.installed = true;
      ollama.running = true;
      ollamaUp();
      return ollamaStatus();
    case "ollama_pull": {
      const m = CATALOG.find((c) => c.tag === args.model);
      await fakeProgress(`ollama:${args.model}`, m?.download_bytes ?? 1e9, "downloading", "pulling a1b2c3d4");
      emit("model:progress", { id: `ollama:${args.model}`, done: 1, total: 1, stage: "verifying", detail: "verifying sha256 digest" });
      await new Promise((r) => setTimeout(r, 800));
      ollama.models = [...ollama.models, args.model];
      const o = settings.providers.find((p: any) => p.id === "ollama");
      if (o) o.model = args.model;
      warm("cleanup", 2000);
      return snapshot();
    }
    case "model_download": {
      const m = snapshot().models.find((x) => x.id === args.id);
      if (!m) throw "Unknown model";
      await fakeProgress(m.id, m.bytes, "downloading");
      emit("model:progress", { id: m.id, done: m.bytes, total: m.bytes, stage: "installing", detail: null });
      await new Promise((r) => setTimeout(r, 600));
      emit("model:progress", { id: m.id, done: m.bytes, total: m.bytes, stage: "ready", detail: null });
      installedModels.add(m.id);
      if (m.id.startsWith("parakeet")) warm("speech");
      return snapshot();
    }
    case "warm_now":
      warm("cleanup");
      return null;
    case "free_disk_mb":
      return Number(params.get("disk") ?? 250_000);
    case "pill_ready":
      return pillState;
    case "subscribe_levels": {
      const ch = args.channel;
      let t = 0;
      setInterval(() => {
        const level = pillKind === "quiet" ? 0.02 : 0.45 + 0.4 * Math.abs(Math.sin(t / 3));
        const cb = callbacks.get(ch.id ?? ch);
        cb?.({ message: { level, speech: true }, index: t });
        t += 1;
      }, 33);
      return null;
    }
    case "mic_monitor":
      return null;
    case "permissions":
    case "request_permission":
    case "open_permission_settings": {
      // `&perm=missing` previews a Mac that has not granted anything yet.
      const missing = params.get("perm") === "missing";
      return { accessibility: !missing, input_monitoring: !missing, microphone: missing ? "undetermined" : "granted" };
    }
    case "resolve_app":
      return args?.name ? `com.example.${String(args.name).toLowerCase()}` : null;
    case "accent": {
      // `&accent=CA5010` previews an OS accent colour under "System".
      const palette = settings.accent;
      return { palette, os: palette === "system" && params.get("accent") ? "#" + params.get("accent") : null };
    }
    case "plugin:event|listen": {
      // `args.handler` is the id from transformCallback; emit() calls it with {event, id, payload}.
      const id = nextId++;
      const ev = String(args.event);
      listeners.set(ev, [...(listeners.get(ev) ?? []), Number(args.handler)]);
      return id;
    }
    default:
      return null;
  }
}

(window as any).__TAURI_INTERNALS__ = {
  invoke,
  transformCallback(cb: Cb) {
    const id = nextId++;
    callbacks.set(id, cb);
    return id;
  },
  unregisterCallback(id: number) {
    callbacks.delete(id);
  },
  convertFileSrc: (p: string) => p,
  metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
};
(window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };

// `&ollama=installing` / `&ollama=pulling` press the matching button once the page is up,
// so the in-progress states can be captured without a click.
if (ollamaScenario === "installing" || ollamaScenario === "pulling") {
  const actions = ollamaScenario === "installing" ? ["install-all", "install"] : [`pull:${params.get("pull") ?? "gemma4:e2b-it-qat"}`];
  const tryClick = (left: number) => {
    const btn = actions.map((a) => document.querySelector<HTMLButtonElement>(`button[data-action="${a}"]`)).find(Boolean);
    if (btn) btn.click();
    else if (left > 0) setTimeout(() => tryClick(left - 1), 200);
  };
  setTimeout(() => tryClick(25), 300);
}

// Screenshots: headless capture does not advance CSS animations or rAF reliably.
if (params.has("still")) {
  const st = document.createElement("style");
  st.textContent = "*,*::before,*::after{animation:none!important;transition:none!important}";
  document.head.appendChild(st);
  
}

function mockExpires(days: string | null): string | null {
  if (days === null) return null;
  const d = new Date();
  d.setDate(d.getDate() + Number(days));
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}
