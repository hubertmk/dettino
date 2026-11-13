// The onboarding's local setup card: what gets installed, one button for all of it,
// one progress bar for the whole job, and a last row that turns green only when the
// models are loaded and Ollama answers.
import { useEffect } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, Circle, Download, ExternalLink, Loader } from "lucide-react";
import { api, formatBytes } from "../lib/api";
import { useStore } from "../lib/store";
import { useSetup } from "../lib/setupJob";
import { neededMb, PlanStep, stepStage, stepStatus } from "../lib/setup";
import { modelHint } from "../lib/ollama";
import { startupFailure, startupLabel } from "../lib/readiness";
import { APP_NAME } from "../lib/brand";
import { Banner } from "../components/ui";

const DOWNLOAD_PAGE = "https://ollama.com/download";

/** The bar and the line under it while the job runs. */
export function SetupProgress() {
  const job = useSetup();
  const { total, pct } = job.progress;
  // Unpacking, installing and starting have no byte count: keep the bar visibly busy.
  const stage = job.current ? stepStage(job.current, job.prog) : null;
  const busy = job.state === "running" && stage !== "downloading";
  // The bar covers the whole job; the step's own bytes are on its row.
  const steps = job.queue.length;
  const n = job.queue.findIndex((s) => s.kind === job.current?.kind) + 1;
  const where = [steps > 1 && n > 0 ? `Step ${n} of ${steps}` : "", total > 0 ? `${pct} %` : ""].filter(Boolean).join(" · ");
  return (
    <div className="stack" style={{ gap: 4 }}>
      <div className={`progress${busy ? " indeterminate" : ""}`} role="progressbar" aria-label="Installing" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}><i style={{ width: `${pct}%` }} /></div>
      <div className="hstack small muted" style={{ justifyContent: "space-between" }}>
        <span>{job.label}</span>
        {where && <span>{where}</span>}
      </div>
    </div>
  );
}

export function SetupCard() {
  const { snap } = useStore();
  const job = useSetup();
  const rec = snap.cleanup_recommendation;
  const r = snap.readiness;
  const pending = job.plan.filter((s) => !s.done);
  const bytes = pending.reduce((n, s) => n + s.bytes, 0);
  const needMb = neededMb(pending);
  const lowDisk = job.state !== "running" && job.freeMb != null && needMb > job.freeMb;
  const ollamaFailed = job.state === "error" && job.current?.kind !== "speech";
  // Installed: now the models load and Ollama answers, or something says why not.
  const failure = job.ready ? startupFailure(r) : null;
  const running = job.ready && r.all;
  const ollamaDown = r.ollama.state === "failed" || r.ollama.state === "missing";
  const retry = () => {
    if (r.ollama.state === "missing") job.start();
    else if (r.ollama.state === "failed") api.ollamaLaunch().catch(() => {});
    else api.warmNow();
  };

  // Ollama may be started or installed outside the app: keep looking while it is missing.
  const reachable = !!job.ollama?.reachable;
  const needsOllama = job.plan.some((s) => s.kind === "ollama");
  useEffect(() => {
    if (!needsOllama || reachable || job.state === "running") return;
    const t = setInterval(job.refreshOllama, 3000);
    return () => clearInterval(t);
  }, [needsOllama, reachable, job.state, job.refreshOllama]);

  // Each row says where it stands: what it is before, what it does now, "Installed." after.
  const active = (s: PlanStep) => job.state === "running" && job.current?.kind === s.kind;
  const desc = (s: PlanStep) => {
    if (s.done) return s.kind === "ollama" ? "Running." : "Installed.";
    if (active(s)) return stepStatus(s, job.prog);
    switch (s.kind) {
      case "speech":
        return "Turns your voice into text, on this computer.";
      case "ollama":
        return job.ollama?.installed ? `Installed; ${APP_NAME} starts it.` : "The free program that runs the cleanup model.";
      case "cleanup": {
        const m = snap.ollama_catalog.find((c) => c.tag === s.tag);
        // The size is already on the right; keep the memory and speed part of the hint.
        const hint = m ? modelHint(m, rec, snap.gpus).split(" · ").slice(1).join(" · ") : "";
        return `Adds punctuation and removes filler words.${hint ? ` Needs ${hint.replace(/^needs /, "")}.` : ""}`;
      }
    }
  };

  const footer =
    job.state === "running" ? (
      <>
        <SetupProgress />
        <span className="small muted">This takes a while. Continue unlocks when everything is installed and running. If the connection drops, click Try again — the model downloads continue where they stopped.</span>
      </>
    ) : job.state === "error" ? (
      <Banner kind="err">
        {job.error}
        <div className="hstack" style={{ marginTop: 8 }}>
          <button className="btn small" data-action="install-all" onClick={job.start}>Try again</button>
          {ollamaFailed && (
            <button type="button" className="linkish" onClick={() => openUrl(DOWNLOAD_PAGE)}>
              Ollama download page <ExternalLink size={11} />
            </button>
          )}
        </div>
      </Banner>
    ) : !job.ready && job.checked ? (
      <div className="hstack">
        <button className="btn primary" data-action="install-all" onClick={job.start}>
          <Download /> {bytes ? `Install everything (${formatBytes(bytes)})` : "Start Ollama"}
        </button>
        <span className="small muted">Needs no administrator rights.</span>
      </div>
    ) : failure ? (
      <Banner kind="err">
        {failure}
        <div className="hstack" style={{ marginTop: 8 }}>
          <button className="btn small" data-action="retry-startup" onClick={retry}>Try again</button>
          {ollamaDown && (
            <button type="button" className="linkish" onClick={() => openUrl(DOWNLOAD_PAGE)}>
              Ollama download page <ExternalLink size={11} />
            </button>
          )}
        </div>
      </Banner>
    ) : running ? (
      <span className="small muted">{rec.reason}</span>
    ) : null;
  const slowCpu = rec.slow_cpu && needsOllama;

  return (
    <div className="card">
      <div className="card-head hstack" style={{ justifyContent: "space-between" }}>
        <h3>On this computer</h3>
        {running ? (
          <span className="chip ok"><Check /> Installed and running</span>
        ) : job.ready ? (
          failure ? <span className="chip warn">Needs attention</span> : <span className="chip info">Installed · starting up…</span>
        ) : !job.checked ? (
          <span className="small muted">Checking…</span>
        ) : null}
      </div>
      {job.plan.map((s) => (
        <div className="row" key={s.kind}>
          {s.done ? (
            <Check size={18} color="var(--success)" aria-label="Installed" />
          ) : active(s) ? (
            <Loader size={18} color="var(--accent)" className="spin" aria-label="Installing now" />
          ) : (
            <Circle size={18} color="var(--text-3)" aria-label="Not installed yet" />
          )}
          <div className="grow">
            <div className="title">{s.name}</div>
            <div className="desc">{desc(s)}</div>
          </div>
          {!s.done && !active(s) && s.bytes > 0 && <span className="small muted">{formatBytes(s.bytes)}</span>}
        </div>
      ))}
      {job.ready && (
        <div className="row">
          {running ? (
            <Check size={18} color="var(--success)" aria-label="Running" />
          ) : failure ? (
            <Circle size={18} color="var(--warning)" aria-label="Needs attention" />
          ) : (
            <Loader size={18} color="var(--accent)" className="spin" aria-label="Starting" />
          )}
          <div className="grow">
            <div className="title">Starting up</div>
            <div className="desc">{running ? "The speech model is loaded and the cleanup model answers." : failure ?? startupLabel(r) ?? "Starting everything up…"}</div>
          </div>
        </div>
      )}
      {(footer || lowDisk || slowCpu) && (
        <div className="card-pad stack" style={{ borderTop: "1px solid var(--border)" }}>
          {footer}
          {lowDisk && (
            <Banner kind="warn">
              Only {formatBytes(job.freeMb! * 1e6)} free on this drive; the setup needs about {formatBytes(needMb * 1e6)}. Free up some space first.
            </Banner>
          )}
          {slowCpu && (
            <Banner kind="warn">
              Cleanup on this computer takes a few seconds per dictation. {APP_NAME} waits for it and says so on the pill. If that feels slow, switch to Cloud mode or add a cloud cleanup provider later.
            </Banner>
          )}
        </div>
      )}
    </div>
  );
}
