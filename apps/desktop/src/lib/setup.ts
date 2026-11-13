// What the chosen mode needs installed before dictation works, as one plan the
// onboarding's "Install everything" button runs (pure helpers, unit-tested; the job
// itself lives in setupJob.tsx).
import { DownloadProgress, formatBytes, needsKey, OllamaStatus, ProviderConfig, Snapshot } from "./api";
import { progressLabel } from "./ollama";

/** Size of the Ollama installer as of 2026-10 (shown on the button). */
export const OLLAMA_SETUP_BYTES = 1_584_629_376;
/** Disk use of an installed Ollama (runtime and GPU libraries; 2.8 GB measured 2026-10); only for the low-disk warning. */
const OLLAMA_INSTALLED_BYTES = 3_000_000_000;
/** Room for unpacking archives and everything else. */
const HEADROOM_BYTES = 1_000_000_000;

export type StepKind = "speech" | "ollama" | "cleanup";

export interface PlanStep {
  kind: StepKind;
  name: string;
  /** Bytes still to download for this step (0 when it only needs starting). */
  bytes: number;
  /** `model:progress` ids this step reports under. */
  ids: string[];
  done: boolean;
  /** The Ollama model to pull (cleanup step). */
  tag?: string;
}

const isOpenAi = (p: ProviderConfig) =>
  (p.type === "openai_batch" || p.type === "openai_chat") && (!p.base_url || p.base_url.includes("api.openai.com"));

/** What the first provider of each chain needs; later ones are optional fallbacks. */
export function providerNeeds(snap: Snapshot) {
  const s = snap.settings;
  const provider = (id: string) => s.providers.find((p) => p.id === id);
  const primaries = [s.asr_chain[0], s.llm_chain[0]].filter(Boolean).map((id) => provider(id)).filter((p): p is ProviderConfig => !!p);
  const askKey = primaries.filter((p) => needsKey(p.type));
  // One OpenAI key covers transcription and cleanup, so ask for it once (on the
  // cleanup provider, whose save runs a live test).
  const keyProviders = askKey.filter(isOpenAi).length > 1 ? askKey.filter((p) => !isOpenAi(p) || p.type === "openai_chat") : askKey;
  return {
    localAsr: primaries.some((p) => p.type === "parakeet_local" || p.type === "whisper_local"),
    ollama: primaries.find((p) => p.type === "ollama"),
    keyProviders,
  };
}

/** The Ollama model cleanup will use: the configured one if it is installed, else the recommended one. */
export function cleanupTag(snap: Snapshot, status: OllamaStatus | null): string {
  const current = providerNeeds(snap).ollama?.model;
  return current && status?.models.includes(current) ? current : snap.cleanup_recommendation.model;
}

/** Everything the chosen mode installs, in the order the job runs it. */
export function setupPlan(snap: Snapshot, status: OllamaStatus | null): PlanStep[] {
  const needs = providerNeeds(snap);
  const steps: PlanStep[] = [];
  if (needs.localAsr) {
    const missing = snap.models.filter((m) => m.recommended && !m.installed);
    steps.push({
      kind: "speech",
      name: snap.models.find((m) => m.recommended && m.family !== "vad")?.name ?? "Speech model",
      bytes: missing.reduce((n, m) => n + m.bytes, 0),
      ids: missing.map((m) => m.id),
      done: missing.length === 0,
    });
  }
  if (needs.ollama) {
    const running = !!status?.reachable;
    steps.push({ kind: "ollama", name: "Ollama", bytes: status?.installed ? 0 : OLLAMA_SETUP_BYTES, ids: ["ollama:setup"], done: running });
    const tag = cleanupTag(snap, status);
    const m = snap.ollama_catalog.find((c) => c.tag === tag);
    const has = running && !!status?.models.includes(tag);
    steps.push({ kind: "cleanup", name: m?.name ?? tag, bytes: has ? 0 : m?.download_bytes ?? 0, ids: [`ollama:${tag}`], done: has, tag });
  }
  return steps;
}

/** Fraction of one step that is done, from its progress events. */
function stepFraction(step: PlanStep, prog: Record<string, DownloadProgress>): number {
  if (step.kind === "speech") {
    if (!step.bytes) return 0;
    const done = step.ids.reduce((n, id) => {
      const p = prog[id];
      return n + (!p ? 0 : p.stage === "installing" || p.stage === "ready" ? p.total : p.done);
    }, 0);
    return Math.min(1, done / step.bytes);
  }
  const p = prog[step.ids[0]];
  if (!p) return 0;
  if (p.stage !== "downloading") return p.stage === "starting" && step.kind === "cleanup" ? 0 : 1;
  return p.total ? Math.min(1, p.done / p.total) : 0;
}

/** Bytes done of the whole plan; finished steps count in full. */
export function planProgress(plan: PlanStep[], prog: Record<string, DownloadProgress>, finished: StepKind[]) {
  const total = plan.reduce((n, s) => n + s.bytes, 0);
  const done = plan.reduce((n, s) => n + (finished.includes(s.kind) ? s.bytes : s.bytes * stepFraction(s, prog)), 0);
  return { done, total, pct: total ? Math.round((done / total) * 100) : finished.length === plan.length ? 100 : 0 };
}

/** The progress event the running step is at: the first of its ids not yet ready. */
function stepEvent(step: PlanStep, prog: Record<string, DownloadProgress>): DownloadProgress | undefined {
  return prog[step.ids.find((id) => prog[id] && prog[id].stage !== "ready") ?? step.ids[0]];
}

/** The stage the running step is in, or null before its first event. */
export function stepStage(step: PlanStep, prog: Record<string, DownloadProgress>): DownloadProgress["stage"] | null {
  return stepEvent(step, prog)?.stage ?? null;
}

/** What the running step is doing, in words (the bytes are on the step's row). */
export function stepLabel(step: PlanStep, prog: Record<string, DownloadProgress>): string {
  const p = stepEvent(step, prog);
  const what = { speech: "the speech model", ollama: "Ollama", cleanup: "the cleanup model" }[step.kind];
  if (!p) return step.kind === "ollama" && !step.bytes ? "Starting Ollama…" : `Preparing ${what}…`;
  switch (p.stage) {
    case "downloading":
      return `Downloading ${what}…`;
    case "installing":
      return step.kind === "speech" ? "Unpacking the speech model…" : "Installing Ollama…";
    default:
      return progressLabel(p, step.kind === "ollama" ? "Ollama" : what);
  }
}

/** What the running step is doing, for its own row (the name is already there). */
export function stepStatus(step: PlanStep, prog: Record<string, DownloadProgress>): string {
  const p = stepEvent(step, prog);
  if (!p) return step.kind === "ollama" && !step.bytes ? "Starting…" : "Preparing…";
  switch (p.stage) {
    case "downloading":
      return p.total ? `Downloading… ${formatBytes(p.done)} of ${formatBytes(p.total)}` : "Downloading…";
    case "installing":
      return step.kind === "speech" ? "Unpacking…" : "Installing…";
    case "verifying":
      return "Checking the download…";
    case "starting":
      return step.kind === "ollama" ? "Starting…" : "Preparing…";
    default:
      return "Done.";
  }
}

/** Disk space the plan needs, in MB, including the installed Ollama and some headroom. */
export function neededMb(plan: PlanStep[]): number {
  const bytes = plan.reduce((n, s) => n + s.bytes + (s.kind === "ollama" && s.bytes ? OLLAMA_INSTALLED_BYTES : 0), 0);
  return bytes ? Math.round((bytes + HEADROOM_BYTES) / 1e6) : 0;
}
