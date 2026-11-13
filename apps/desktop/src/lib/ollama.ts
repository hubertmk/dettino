// Pure helpers for the in-app Ollama setup (no Tauri calls, unit-tested).
import { CleanupRecommendation, DownloadProgress, formatBytes, GpuInfo, OllamaModel, OllamaStatus } from "./api";

export type OllamaPhase = "probing" | "missing" | "stopped" | "running";

/** What the setup card should show for a status probe. */
export function ollamaPhase(status: OllamaStatus | null): OllamaPhase {
  if (!status) return "probing";
  if (status.reachable) return "running";
  return status.installed ? "stopped" : "missing";
}

/** One line under a model row: size, memory, and the speed to expect here. */
export function modelHint(m: OllamaModel, rec: CleanupRecommendation, gpus: GpuInfo[]): string {
  const nvidia = gpus.some((g) => g.vendor === "NVIDIA");
  const speed = nvidia || rec.tier.startsWith("apple") ? m.gpu_note : m.cpu_note;
  return `${formatBytes(m.download_bytes)} download · needs about ${Math.round(m.memory_mb / 1024)} GB of ${nvidia ? "GPU memory" : "RAM"} · ${speed}`;
}

/** Percent for a progress bar; 0 while the total is unknown. */
export function pullPercent(p?: DownloadProgress): number {
  if (!p || !p.total) return p?.stage === "ready" ? 100 : 0;
  return Math.max(0, Math.min(100, Math.round((p.done / p.total) * 100)));
}

/** Short status line for a running download, install or pull. */
export function progressLabel(p: DownloadProgress | undefined, what: string): string {
  if (!p) return `Starting ${what}…`;
  switch (p.stage) {
    case "starting":
      return what === "Ollama" ? "Starting Ollama…" : `Preparing ${what}…`;
    case "installing":
      return "Installing…";
    case "verifying":
      return "Checking the download…";
    case "ready":
      return "Done";
    default:
      return p.total ? `${formatBytes(p.done)} of ${formatBytes(p.total)}` : p.detail || "Downloading…";
  }
}
