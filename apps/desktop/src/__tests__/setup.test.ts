import { describe, expect, it } from "vitest";
import defaults from "../dev/defaults.json";
import { cleanupTag, neededMb, OLLAMA_SETUP_BYTES, planProgress, providerNeeds, setupPlan, stepLabel, stepStatus } from "../lib/setup";
import type { DownloadProgress, OllamaStatus, Settings, Snapshot } from "../lib/api";

const E2B = "gemma4:e2b-it-qat";
const snap = (over: { installed?: boolean; settings?: Partial<Settings>; keys?: Record<string, boolean> } = {}): Snapshot =>
  ({
    settings: { ...(structuredClone(defaults) as unknown as Settings), ...over.settings },
    keys: over.keys ?? {},
    models: [
      { id: "silero-vad", family: "vad", name: "Silero", bytes: 1_000_000, recommended: true, installed: true },
      { id: "parakeet", family: "parakeet", name: "Parakeet TDT 0.6B v2", bytes: 482_000_000, recommended: true, installed: !!over.installed },
      { id: "whisper", family: "whisper", name: "Whisper", bytes: 560_000_000, recommended: false, installed: false },
    ],
    cleanup_recommendation: { model: E2B, reason: "", slow_cpu: false, tier: "gpu" },
    ollama_catalog: [{ tag: E2B, name: "Gemma 4 E2B", download_bytes: 4_300_000_000, memory_mb: 2500, cpu_note: "", gpu_note: "" }],
  }) as unknown as Snapshot;
const ollama = (over: Partial<OllamaStatus> = {}): OllamaStatus => ({ installed: false, reachable: false, models: [], version: null, can_install: true, ...over });
const p = (id: string, done: number, total: number, stage: DownloadProgress["stage"] = "downloading"): DownloadProgress => ({ id, done, total, stage });

describe("setupPlan", () => {
  it("lists everything a fresh Private computer needs, in order", () => {
    const plan = setupPlan(snap(), ollama());
    expect(plan.map((s) => [s.kind, s.bytes, s.done])).toEqual([
      ["speech", 482_000_000, false],
      ["ollama", OLLAMA_SETUP_BYTES, false],
      ["cleanup", 4_300_000_000, false],
    ]);
    expect(plan[2].ids).toEqual([`ollama:${E2B}`]);
  });

  it("only starts an installed Ollama and skips what is there", () => {
    const plan = setupPlan(snap({ installed: true }), ollama({ installed: true }));
    expect(plan.map((s) => [s.kind, s.bytes, s.done])).toEqual([
      ["speech", 0, true],
      ["ollama", 0, false],
      ["cleanup", 4_300_000_000, false],
    ]);
    const ready = setupPlan(snap({ installed: true }), ollama({ installed: true, reachable: true, models: [E2B] }));
    expect(ready.every((s) => s.done)).toBe(true);
  });

  it("keeps a configured model that is installed instead of pulling the recommended one", () => {
    const s = snap();
    s.settings.providers.find((x) => x.id === "ollama")!.model = "gemma4:e4b-it-qat";
    const st = ollama({ installed: true, reachable: true, models: ["gemma4:e4b-it-qat"] });
    expect(cleanupTag(s, st)).toBe("gemma4:e4b-it-qat");
    expect(setupPlan(s, st).find((x) => x.kind === "cleanup")?.done).toBe(true);
    expect(cleanupTag(s, ollama({ reachable: true, models: [] }))).toBe(E2B);
  });

  it("needs nothing local and asks one OpenAI key in Cloud mode", () => {
    const s = snap({ settings: { asr_chain: ["openai-asr"], llm_chain: ["openai-chat"] } });
    expect(setupPlan(s, null)).toEqual([]);
    expect(providerNeeds(s).keyProviders.map((x) => x.id)).toEqual(["openai-chat"]);
  });
});

describe("planProgress", () => {
  const plan = setupPlan(snap(), ollama()).filter((s) => !s.done);
  const total = 482_000_000 + OLLAMA_SETUP_BYTES + 4_300_000_000;

  it("adds up bytes across steps and counts finished steps in full", () => {
    expect(planProgress(plan, {}, []).pct).toBe(0);
    const half = planProgress(plan, { parakeet: p("parakeet", 241_000_000, 482_000_000) }, []);
    expect(half.done).toBe(241_000_000);
    expect(half.total).toBe(total);
    const later = planProgress(plan, { "ollama:setup": p("ollama:setup", 1, 1, "installing") }, ["speech"]);
    expect(later.done).toBe(482_000_000 + OLLAMA_SETUP_BYTES);
    expect(planProgress(plan, {}, ["speech", "ollama", "cleanup"]).pct).toBe(100);
  });

  it("says what is happening", () => {
    expect(stepLabel(plan[0], { parakeet: p("parakeet", 100e6, 482e6) })).toBe("Downloading the speech model…");
    expect(stepLabel(plan[1], { "ollama:setup": p("ollama:setup", 1, 1, "installing") })).toBe("Installing Ollama…");
    expect(stepLabel(plan[1], { "ollama:setup": p("ollama:setup", 1, 1, "starting") })).toBe("Starting Ollama…");
    expect(stepLabel(plan[2], {})).toBe("Preparing the cleanup model…");
  });

  it("says what the running step is doing, on its own row", () => {
    expect(stepStatus(plan[0], { parakeet: p("parakeet", 100e6, 482e6) })).toBe("Downloading… 100 MB of 482 MB");
    expect(stepStatus(plan[0], { parakeet: p("parakeet", 1, 1, "installing") })).toBe("Unpacking…");
    expect(stepStatus(plan[1], { "ollama:setup": p("ollama:setup", 1, 1, "installing") })).toBe("Installing…");
    expect(stepStatus(plan[1], { "ollama:setup": p("ollama:setup", 1, 1, "starting") })).toBe("Starting…");
    expect(stepStatus(plan[2], {})).toBe("Preparing…");
  });

  it("estimates the disk space including the installed Ollama", () => {
    expect(neededMb([])).toBe(0);
    expect(neededMb(plan)).toBeGreaterThan(Math.round(total / 1e6) + 3000);
  });
});
