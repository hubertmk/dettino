import { describe, expect, it } from "vitest";
import { modelHint, ollamaPhase, progressLabel, pullPercent } from "../lib/ollama";
import type { CleanupRecommendation, OllamaModel } from "../lib/api";

const status = (over: Partial<Parameters<typeof ollamaPhase>[0] & object>) => ({ installed: false, reachable: false, models: [], version: null, can_install: true, ...over });
const model: OllamaModel = { tag: "gemma4:e2b-it-qat", name: "Gemma 4 E2B", download_bytes: 4_300_000_000, memory_mb: 2_500, cpu_note: "Several seconds per cleanup on the processor", gpu_note: "About half a second on an NVIDIA GPU" };
const rec = (tier: string): CleanupRecommendation => ({ model: "gemma4:e2b-it-qat", reason: "", slow_cpu: false, tier });

describe("ollamaPhase", () => {
  it("maps the four states", () => {
    expect(ollamaPhase(null)).toBe("probing");
    expect(ollamaPhase(status({}))).toBe("missing");
    expect(ollamaPhase(status({ installed: true }))).toBe("stopped");
    expect(ollamaPhase(status({ installed: true, reachable: true }))).toBe("running");
  });
});

describe("modelHint", () => {
  it("talks about GPU memory on an NVIDIA box and RAM otherwise", () => {
    const nv = modelHint(model, rec("gpu"), [{ name: "NVIDIA RTX", vram_mb: 24_000, vendor: "NVIDIA" }]);
    expect(nv).toContain("GPU memory");
    expect(nv).toContain("About half a second");
    const cpu = modelHint(model, rec("cpu"), []);
    expect(cpu).toContain("RAM");
    expect(cpu).toContain("Several seconds");
    expect(cpu).toContain("2 GB");
  });
});

describe("pullPercent / progressLabel", () => {
  it("handles unknown totals and the ready stage", () => {
    expect(pullPercent(undefined)).toBe(0);
    expect(pullPercent({ id: "x", done: 5, total: 0, stage: "downloading" })).toBe(0);
    expect(pullPercent({ id: "x", done: 50, total: 200, stage: "downloading" })).toBe(25);
    expect(pullPercent({ id: "x", done: 0, total: 0, stage: "ready" })).toBe(100);
    expect(progressLabel({ id: "x", done: 0, total: 0, stage: "installing" }, "Ollama")).toBe("Installing…");
    expect(progressLabel({ id: "x", done: 0, total: 0, stage: "downloading", detail: "pulling manifest" }, "gemma4:e2b-it-qat")).toBe("pulling manifest");
  });
});
