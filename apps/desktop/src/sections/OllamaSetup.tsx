// Local cleanup setup without leaving the app: detect Ollama, install or start it,
// pull a cleanup model with progress, and recommend one for this computer.
import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, Download, ExternalLink, Play } from "lucide-react";
import { api, formatBytes, OllamaModel, OllamaStatus } from "../lib/api";
import { useStore } from "../lib/store";
import { useModelProgress } from "../lib/progress";
import { modelHint, ollamaPhase, progressLabel, pullPercent } from "../lib/ollama";
import { OLLAMA_SETUP_BYTES } from "../lib/setup";
import { Banner, ms } from "../components/ui";

const DOWNLOAD_PAGE = "https://ollama.com/download";
const SETUP_ID = "ollama:setup";

function Progress({ pct, label }: { pct: number; label: string }) {
  return (
    <div style={{ marginTop: 8 }}>
      <div className="progress" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}><i style={{ width: `${pct}%` }} /></div>
      <div className="small muted" style={{ marginTop: 4 }}>{label}</div>
    </div>
  );
}

export function OllamaSetup({ base, compact }: { base: string; compact?: boolean }) {
  const { snap, setSnap, update, toast } = useStore();
  const [status, setStatus] = useState<OllamaStatus | null>(null);
  // "install" | "launch" | a model tag being pulled
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<{ action: "install" | "launch" | string; message: string } | null>(null);
  const prog = useModelProgress();
  const phase = ollamaPhase(status);
  const provider = snap.settings.providers.find((p) => p.id === "ollama");
  const rec = snap.cleanup_recommendation;

  const refresh = useCallback(async () => {
    try {
      setStatus(await api.ollamaStatus(base));
    } catch {
      setStatus({ installed: false, reachable: false, models: [], version: null, can_install: false });
    }
  }, [base]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Ollama may be started outside the app (Start menu, autostart): keep looking.
  useEffect(() => {
    if (phase === "running" || busy) return;
    const t = setInterval(refresh, 3000);
    return () => clearInterval(t);
  }, [phase, busy, refresh]);

  const run = async (action: "install" | "launch") => {
    setBusy(action);
    setError(null);
    try {
      setStatus(await (action === "install" ? api.ollamaInstall(base) : api.ollamaLaunch(base)));
    } catch (e) {
      setError({ action, message: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const pull = async (tag: string) => {
    setBusy(tag);
    setError(null);
    try {
      setSnap(await api.ollamaPull(tag, base));
      await refresh();
      toast(`${tag} is ready`);
    } catch (e) {
      setError({ action: tag, message: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const retry = () => {
    if (!error) return;
    if (error.action === "install" || error.action === "launch") run(error.action);
    else pull(error.action);
  };

  const downloadPage = (
    <button type="button" className="linkish" onClick={() => openUrl(DOWNLOAD_PAGE)}>
      Download page <ExternalLink size={11} />
    </button>
  );

  if (busy === "install") {
    const p = prog[SETUP_ID];
    return (
      <div className="stack">
        <b>Installing Ollama</b>
        <Progress pct={pullPercent(p)} label={progressLabel(p, "Ollama")} />
        <span className="small muted">The installer runs for your user only and needs no administrator rights. Downloads resume if the connection drops.</span>
      </div>
    );
  }
  if (busy === "launch") {
    return <span className="muted">Starting Ollama…</span>;
  }

  if (error) {
    return (
      <Banner kind="err">
        {error.message}
        <div className="hstack" style={{ marginTop: 8 }}>
          <button className="btn small" data-action="retry" onClick={retry}>Try again</button>
          {downloadPage}
        </div>
      </Banner>
    );
  }

  if (phase === "probing") {
    return <span className="muted">Looking for Ollama…</span>;
  }

  if (phase === "missing") {
    return (
      <div className="stack">
        <Banner kind="warn">
          Ollama isn't installed. It's the free program that runs the cleanup model on this computer. Until it is set up, your words are pasted as heard (with punctuation).
        </Banner>
        <div className="hstack">
          {status?.can_install ? (
            <button className="btn primary" data-action="install" onClick={() => run("install")}>
              <Download /> Install Ollama ({formatBytes(OLLAMA_SETUP_BYTES)})
            </button>
          ) : (
            <span className="small muted">Install it from the download page, then come back here.</span>
          )}
          {downloadPage}
        </div>
      </div>
    );
  }

  if (phase === "stopped") {
    return (
      <div className="stack">
        <Banner kind="warn">Ollama is installed but not running.</Banner>
        <div className="hstack">
          <button className="btn primary" data-action="launch" onClick={() => run("launch")}><Play /> Start Ollama</button>
          <span className="small muted">Or start it from the Start menu; this card updates by itself.</span>
        </div>
      </div>
    );
  }

  // running
  const installed = new Set(status?.models ?? []);
  const current = provider?.model ?? "";
  const others = [...installed].filter((tag) => !snap.ollama_catalog.some((m) => m.tag === tag));
  const measured = snap.stats.ollama?.p50_ms;
  const use = (tag: string) =>
    update((x) => {
      const o = x.providers.find((p) => p.id === "ollama");
      if (o) o.model = tag;
    });

  const row = (m: OllamaModel) => {
    const has = installed.has(m.tag);
    const p = prog[`ollama:${m.tag}`];
    return (
      <div className="row" key={m.tag}>
        <div className="grow">
          <div className="title hstack">
            {m.name}
            <span className="mono small muted">{m.tag}</span>
            {rec.model === m.tag && <span className="chip info">Recommended</span>}
            {has && current === m.tag && <span className="chip ok"><Check /> In use</span>}
          </div>
          <div className="desc">{modelHint(m, rec, snap.gpus)}</div>
          {busy === m.tag && <Progress pct={pullPercent(p)} label={progressLabel(p, m.tag)} />}
        </div>
        {has ? (
          current !== m.tag && <button className="btn small" onClick={() => use(m.tag)}>Use</button>
        ) : (
          <button className="btn small" data-action={`pull:${m.tag}`} disabled={!!busy} onClick={() => pull(m.tag)}>
            <Download /> {busy === m.tag ? "Downloading…" : `Download (${formatBytes(m.download_bytes)})`}
          </button>
        )}
      </div>
    );
  };

  return (
    <div className="stack">
      <div className="hstack">
        <span className="chip ok"><Check /> Ollama running · {installed.size} {installed.size === 1 ? "model" : "models"}</span>
        {status?.version && <span className="small muted">v{status.version}</span>}
        {measured != null && installed.has(current) && <span className="small muted">Measured: {ms(measured)} per cleanup</span>}
      </div>
      <div className="card">{snap.ollama_catalog.map(row)}</div>
      {!compact && <p className="small muted" style={{ margin: 0 }}>{rec.reason}</p>}
      {rec.slow_cpu && (
        <Banner kind="warn">
          Cleanup on this computer takes a few seconds per dictation. Dettino waits for it and says so on the pill. If that feels slow, switch to Cloud mode or add a cloud cleanup provider later.
        </Banner>
      )}
      {others.length > 0 && (
        <div className="field">
          <label htmlFor="ollama-other">Or use another installed model</label>
          <select id="ollama-other" className="select" value={others.includes(current) ? current : ""} onChange={(e) => e.target.value && use(e.target.value)}>
            <option value="">Choose…</option>
            {others.map((t) => <option key={t}>{t}</option>)}
          </select>
          <span className="small muted">A 3–9B instruct model works best. Reasoning-only models like deepseek-r1 are slow for cleanup.</span>
        </div>
      )}
    </div>
  );
}
