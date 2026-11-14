// Typed bridge to the Rust commands. Field names mirror the serde output.
import { Channel, invoke } from "@tauri-apps/api/core";

export type CleanupLevel = "off" | "light" | "medium";
export type Preset = "private" | "fast" | "balanced" | "custom";
/** What each preset is called on screen. The stored id "fast" stays for settings compatibility; the mode was renamed Cloud. */
export const PRESET_LABEL: Record<Preset, string> = { private: "Private", fast: "Cloud", balanced: "Balanced", custom: "Custom" };
export type MicMode = "always_warm" | "warm_on_modifier" | "on_demand";
/** Whether "Only while dictating" opens the mic when Control goes down, before the shortcut completes. */
export type MicEarlyOpen = "off" | "close_on_other_key" | "hold";
/** Look of the pill window: the original dot + rounded pill, or the One line. */
export type PillStyle = "classic" | "line";
/** Voice of the start / stop / hands-free / error sounds (crates/platform/src/tones.rs). */
export type SoundStyle = "classic" | "soft_pluck" | "mallet" | "glass" | "warm_pop";
export type SuccessAnimation = "flash" | "check" | "check_word";
export type Accent = "surf" | "cobalt" | "ultramarine" | "indigo" | "lettera" | "plum" | "orchid" | "graphite" | "red" | "system";
export type ProviderType =
  | "whisper_local"
  | "parakeet_local"
  | "openai_batch"
  | "openai_compat_asr"
  | "anthropic"
  | "openai_chat"
  | "perplexity_agent"
  | "ollama"
  | "openai_compat_chat";

export interface ChordSpec {
  mods: number;
  key: number;
}
export interface Hotkeys {
  push_to_talk: ChordSpec;
  hands_free: ChordSpec;
  command: ChordSpec;
  undo_ai: ChordSpec;
  paste_last: ChordSpec;
}
export interface ProviderConfig {
  id: string;
  type: ProviderType;
  display_name: string;
  base_url: string | null;
  model: string;
  models: string[];
  enabled: boolean;
  accepts_prompt: boolean;
  treat_as_local: boolean;
}
export interface ContextSettings {
  enabled: boolean;
  app_name: boolean;
  window_title: boolean;
  caret_text: boolean;
  selection: boolean;
  visible_text: boolean;
  local_only: boolean;
}
export interface AppRule {
  app: string;
  cleanup: CleanupLevel | null;
  context: boolean | null;
  never_dictate: boolean;
  insert_mode: "paste" | "type" | null;
  paste_delay_ms: number | null;
  paste_chord: string | null;
}
export interface Snippet {
  trigger: string;
  body: string;
  enabled: boolean;
}
export interface DictionaryEntry {
  term: string;
  replace_from: string | null;
  source: string;
  enabled: boolean;
}
export interface Settings {
  version: number;
  preset: Preset;
  providers: ProviderConfig[];
  asr_chain: string[];
  llm_chain: string[];
  hedge_ms: number | null;
  cleanup_wait_ms: number | null;
  cleanup_level: CleanupLevel;
  standing_instruction: string;
  /** ISO 639-1 code of the spoken language; null = let the model detect it. */
  language: string | null;
  hotkeys: Hotkeys;
  tap_max_ms: number;
  double_tap_handsfree: boolean;
  handsfree_max_min: number;
  mic_mode: MicMode;
  mic_early_open: MicEarlyOpen;
  input_device: string | null;
  preroll_ms: number;
  prefer_wired: boolean;
  context: ContextSettings;
  app_rules: AppRule[];
  dictionary: DictionaryEntry[];
  learn_from_corrections: boolean;
  snippets: Snippet[];
  keep_on_clipboard: boolean;
  sounds: boolean;
  sound_volume: number;
  sound_style: SoundStyle;
  pill_style: PillStyle;
  success_animation: SuccessAnimation;
  accent: Accent;
  launch_at_login: boolean;
  start_ollama: boolean;
  paused: boolean;
  store_history: boolean;
  audio_retention_days: number;
  onboarding_done: boolean;
  pill_positions: Record<string, { dx: number; dy: number }>;
}
export interface StatsSummary {
  count: number;
  p50_ms: number | null;
  p95_ms: number | null;
  last_error: string | null;
  last_error_ts: number | null;
  recent_ms: number[];
}
export interface ModelStatus {
  id: string;
  family: string;
  name: string;
  description: string;
  url: string;
  bytes: number;
  license: string;
  dir: string;
  archive: boolean;
  english_only: boolean;
  recommended: boolean;
  installed: boolean;
}
export interface GpuInfo {
  name: string;
  vram_mb: number;
  vendor: string;
}
/** A local cleanup model the app can pull into Ollama. */
export interface OllamaModel {
  tag: string;
  name: string;
  download_bytes: number;
  memory_mb: number;
  cpu_note: string;
  gpu_note: string;
}
export interface CleanupRecommendation {
  model: string;
  reason: string;
  slow_cpu: boolean;
  tier: string;
}
export interface OllamaStatus {
  installed: boolean;
  reachable: boolean;
  models: string[];
  version: string | null;
  can_install: boolean;
}
/** One part of what Dettino needs, as the backend sees it right now. */
export type Stage = { state: "not_needed" | "missing" | "warming" | "ready" } | { state: "failed"; error: string };
/** What is loaded and answering: the speech model, Ollama, the cleanup model. */
export interface Readiness {
  speech: Stage;
  ollama: Stage;
  cleanup: Stage;
  /** Every stage is ready or not needed. */
  all: boolean;
}
/** Which build this is (mirrors `build_info.rs`). `label` is what people read: `0.2.0` for a release, `0.2.0+a1b2c3d4e[.dirty]` otherwise. */
export interface BuildInfo {
  version: string;
  label: string;
  commit: string;
  dirty: boolean;
  release: boolean;
  commit_date: string;
  profile: "release" | "debug";
  /** Last day a test build works, YYYY-MM-DD; null for a build that never expires. */
  expires: string | null;
}
export interface Snapshot {
  settings: Settings;
  keys: Record<string, boolean>;
  ready: Record<string, boolean>;
  stats: Record<string, StatsSummary>;
  models: ModelStatus[];
  gpus: GpuInfo[];
  cpu_threads: number;
  ram_mb: number;
  cleanup_recommendation: CleanupRecommendation;
  ollama_catalog: OllamaModel[];
  build: BuildInfo;
  data_dir: string;
  hotkeys_error: string | null;
  history_available: boolean;
  fully_local: boolean;
  platform: string;
  readiness: Readiness;
  /** Launch at login: the setting next to what the OS will really do. */
  autostart: AutostartState;
  /** macOS: where the app runs from; "applications" everywhere else. */
  app_location: AppLocation;
}
export interface AutostartState {
  wanted: boolean;
  active: boolean;
  /** Why it is off when the OS knows (Login Items switch, Task Manager, not in Applications). */
  detail: string | null;
}
export type AppLocation = "applications" | "translocated" | "elsewhere";
export interface Timings {
  hold_ms: number;
  preroll_used_ms: number;
  audio_ms: number;
  speech_ms: number;
  asr_provider: string | null;
  asr_ms: number | null;
  asr_hedged: boolean;
  asr_attempts: string[];
  context_ms: number | null;
  context_timed_out: boolean;
  cleanup_provider: string | null;
  cleanup_model: string | null;
  cleanup_ms: number | null;
  cleanup_skipped: string | null;
  cleanup_rejected: string | null;
  insert_ms: number | null;
  insert_verify: string | null;
  clipboard_restored: boolean | null;
  total_ms: number | null;
}
export type HistoryStatus = "ok" | "ok_raw" | "held" | "failed" | "cancelled" | "nothing_heard" | "unprocessed";
export interface HistoryRecord {
  id: string;
  created_at: number;
  status: HistoryStatus;
  mode: string;
  raw_text: string | null;
  cleaned_text: string | null;
  final_text: string | null;
  edited_text: string | null;
  zero_edit: boolean | null;
  cleanup_level: string | null;
  app: string | null;
  window_title: string | null;
  url: string | null;
  text_before: string | null;
  selection: string | null;
  audio_path: string | null;
  audio_ms: number;
  error: string | null;
  timings: Timings;
}
export interface TestResult {
  ok: boolean;
  ms: number;
  text: string | null;
  detail: string | null;
  error: string | null;
}
export interface DeviceView {
  name: string;
  kind: string;
  is_default: boolean;
}
export interface Suggestion {
  from: string;
  to: string;
  ts: number;
}
export interface ContextSnapshot {
  control: string | null;
  is_password: boolean;
  is_address_bar: boolean;
  before: string | null;
  after: string | null;
  selection: string | null;
  visible_text: string | null;
  url: string | null;
  ms: number;
  timed_out: boolean;
}
export interface ContextPreview {
  app: string;
  title: string;
  context: ContextSnapshot;
  nouns: string[];
  providers: [string, boolean][];
  rule_context: boolean;
}
export interface LevelFrame {
  level: number;
  speech: boolean;
}
export interface DownloadProgress {
  id: string;
  done: number;
  total: number;
  stage: "starting" | "downloading" | "verifying" | "installing" | "ready";
  /** Free-text status (Ollama's "pulling manifest" etc.), when the source has one. */
  detail?: string | null;
}

export const api = {
  getState: () => invoke<Snapshot>("get_state"),
  saveSettings: (settings: Settings) => invoke<Snapshot>("save_settings", { settings }),
  setPreset: (preset: Preset) => invoke<Snapshot>("set_preset", { preset }),
  setProviderKey: (id: string, key: string) => invoke<Snapshot>("set_provider_key", { id, key }),
  setPaused: (paused: boolean) => invoke<Snapshot>("set_paused", { paused }),
  testProvider: (id: string) => invoke<TestResult>("test_provider", { id }),
  micCheck: () => invoke<TestResult>("mic_check"),
  listDevices: () => invoke<DeviceView[]>("list_devices"),
  micMonitor: (on: boolean, onLevel: (f: LevelFrame) => void) => {
    const channel = new Channel<LevelFrame>();
    channel.onmessage = onLevel;
    return invoke<void>("mic_monitor", { on, channel });
  },
  modelDownload: (id: string) => invoke<Snapshot>("model_download", { id }),
  /** Free MB where the setup downloads go; null when unknown. */
  freeDiskMb: () => invoke<number | null>("free_disk_mb"),
  /** Load the models again after a failed warm-up. */
  warmNow: () => invoke<void>("warm_now"),
  modelDelete: (id: string) => invoke<Snapshot>("model_delete", { id }),
  historyList: (query?: string, limit?: number) => invoke<HistoryRecord[]>("history_list", { query, limit }),
  historyDelete: (id: string) => invoke<void>("history_delete", { id }),
  historyDeleteAll: () => invoke<void>("history_delete_all"),
  historyAudio: (id: string) => invoke<ArrayBuffer>("history_audio", { id }),
  historyRetry: (id: string) => invoke<void>("history_retry", { id }),
  copyText: (text: string) => invoke<boolean>("copy_text", { text }),
  suggestionsList: () => invoke<Suggestion[]>("suggestions_list"),
  suggestionResolve: (to: string, accept: boolean) => invoke<Snapshot>("suggestion_resolve", { to, accept }),
  previewContext: () => invoke<ContextPreview>("preview_context"),
  ollamaStatus: (base?: string) => invoke<OllamaStatus>("ollama_status", { base }),
  ollamaInstall: (base?: string) => invoke<OllamaStatus>("ollama_install", { base }),
  ollamaLaunch: (base?: string) => invoke<OllamaStatus>("ollama_launch", { base }),
  ollamaPull: (model: string, base?: string) => invoke<Snapshot>("ollama_pull", { model, base }),
  ollamaCancel: () => invoke<void>("ollama_cancel"),
  finishOnboarding: () => invoke<Snapshot>("finish_onboarding"),
  openDataFolder: (which: "logs" | "models" | "root" | "applications") => invoke<void>("open_data_folder", { which }),
  exportSettings: () => invoke<string>("export_settings"),
  importSettings: (json: string) => invoke<Snapshot>("import_settings", { json }),
  hotkeysSuspend: (suspend: boolean) => invoke<void>("hotkeys_suspend", { suspend }),
  pillResetPosition: () => invoke<void>("pill_reset_position"),
  previewSound: (style: SoundStyle) => invoke<void>("preview_sound", { style }),
  resolveApp: (name: string) => invoke<string | null>("resolve_app", { name }),
  permissions: () => invoke<Permissions>("permissions"),
  requestPermission: (kind: PermissionKind) => invoke<Permissions>("request_permission", { kind }),
  openPermissionSettings: (kind: PermissionKind) => invoke<void>("open_permission_settings", { kind }),
};

/** macOS privacy permissions (other systems report them granted). */
export interface Permissions {
  accessibility: boolean;
  input_monitoring: boolean;
  microphone: "granted" | "denied" | "undetermined" | "restricted" | "unknown";
}
export type PermissionKind = "accessibility" | "input_monitoring" | "microphone";

// ---- platform ----
let mac = false;
/** Set once from the snapshot: key names and shortcuts follow the OS the app runs on. */
export function setPlatform(platform: string) {
  mac = platform === "macos";
}
export function isMac() {
  return mac;
}

// ---- chord helpers (Windows virtual-key codes; macOS keys are mapped onto them) ----
export const MODS = { CTRL: 1, SHIFT: 2, ALT: 4, WIN: 8 } as const;
const NAMED: Record<number, string> = {
  0x08: "Backspace", 0x09: "Tab", 0x0d: "Enter", 0x14: "Caps Lock", 0x1b: "Esc", 0x20: "Space", 0x2d: "Insert",
  0xa0: "Left Shift", 0xa1: "Right Shift", 0xa2: "Left Ctrl", 0xa3: "Right Ctrl", 0xa4: "Left Alt", 0xa5: "Right Alt",
  0x5b: "Left Win", 0x5c: "Right Win", 0xff: "Fn",
};
const MAC_NAMED: Record<number, string> = {
  0x08: "Delete", 0x0d: "Return", 0xa2: "Left Control", 0xa3: "Right Control", 0xa4: "Left Option", 0xa5: "Right Option",
  0x5b: "Left Command", 0x5c: "Right Command",
};
export function keyName(vk: number): string {
  if (mac && MAC_NAMED[vk]) return MAC_NAMED[vk];
  if (NAMED[vk]) return NAMED[vk];
  if ((vk >= 0x30 && vk <= 0x39) || (vk >= 0x41 && vk <= 0x5a)) return String.fromCharCode(vk);
  if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x6f}`;
  if (vk === 0) return "—";
  return `Key ${vk.toString(16)}`;
}
export function chordParts(c: ChordSpec): string[] {
  const p: string[] = [];
  if (c.mods & MODS.CTRL) p.push(mac ? "Control" : "Ctrl");
  if (c.mods & MODS.ALT) p.push(mac ? "Option" : "Alt");
  if (c.mods & MODS.SHIFT) p.push("Shift");
  if (c.mods & MODS.WIN) p.push(mac ? "Command" : "Win");
  p.push(keyName(c.key));
  return p;
}
export function chordLabel(c: ChordSpec): string {
  return chordParts(c).join(" + ");
}

/** Map a browser KeyboardEvent to a Windows virtual-key code. */
export function vkFromEvent(e: KeyboardEvent): number | null {
  const c = e.code;
  const map: Record<string, number> = {
    Space: 0x20, Enter: 0x0d, Tab: 0x09, Backspace: 0x08, CapsLock: 0x14, Insert: 0x2d,
    ControlLeft: 0xa2, ControlRight: 0xa3, ShiftLeft: 0xa0, ShiftRight: 0xa1, AltLeft: 0xa4, AltRight: 0xa5,
    MetaLeft: 0x5b, MetaRight: 0x5c, Fn: 0xff,
  };
  if (map[c] !== undefined) return map[c];
  if (/^Key[A-Z]$/.test(c)) return c.charCodeAt(3);
  if (/^Digit[0-9]$/.test(c)) return c.charCodeAt(5);
  const f = /^F(\d{1,2})$/.exec(c);
  if (f) return 0x6f + Number(f[1]);
  return null;
}

export function formatBytes(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
  if (n >= 1e6) return `${Math.round(n / 1e6)} MB`;
  return `${Math.round(n / 1e3)} KB`;
}

/** What to call the pill in copy: the classic look is a dot when idle, the One line a line. */
export function pillWord(style: PillStyle): string {
  return style === "line" ? "small line" : "dot";
}

export function isLocalType(t: ProviderType) {
  return t === "whisper_local" || t === "parakeet_local" || t === "ollama";
}
export function needsKey(t: ProviderType) {
  return t === "openai_batch" || t === "anthropic" || t === "openai_chat" || t === "perplexity_agent";
}
export function kindOf(t: ProviderType): "asr" | "llm" {
  return t === "whisper_local" || t === "parakeet_local" || t === "openai_batch" || t === "openai_compat_asr" ? "asr" : "llm";
}
