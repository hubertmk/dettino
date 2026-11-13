import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ArrowRight, Mic, PauseCircle, PlayCircle } from "lucide-react";
import { api, HistoryRecord, isMac, pillWord } from "../lib/api";
import { permissionsMissing, usePermissions } from "../components/Permissions";
import { APP_NAME } from "../lib/brand";
import { useStore } from "../lib/store";
import { autostartProblem, locationWarning, setupRows } from "../lib/readiness";
import { expiry, expiryWarning } from "../lib/expiry";
import { Banner, Chord, ms, timeAgo } from "../components/ui";
import type { SectionId } from "../App";

export function Home({ go, rerunSetup }: { go: (s: SectionId) => void; rerunSetup: () => void }) {
  const { snap, setSnap, refresh } = useStore();
  const s = snap.settings;
  const r = snap.readiness;
  const [recent, setRecent] = useState<HistoryRecord[]>([]);

  useEffect(() => {
    const load = () => api.historyList(undefined, 200).then(setRecent);
    load();
    const un = listen("history:changed", load);
    return () => {
      un.then((u) => u());
    };
  }, []);

  const perms = usePermissions(isMac());
  const asrReady = s.asr_chain.filter((id) => snap.ready[id]);
  const name = (id: string) => s.providers.find((p) => p.id === id)?.display_name ?? id;
  const cleanup = s.cleanup_level === "off" ? "off" : r.cleanup.state;
  const ends = expiry(snap.build.expires);

  const todo: { text: string; action: () => void; label: string }[] = [];
  if (!asrReady.length) todo.push({ text: "Set up a transcription provider or download a local model.", action: () => go("providers"), label: "Providers" });
  if (s.cleanup_level !== "off") {
    for (const row of setupRows(r)) {
      const action = row.action === "setup" ? rerunSetup : row.action === "start_ollama" ? () => api.ollamaLaunch().then(refresh, refresh) : () => go("providers");
      todo.push({ text: row.text, action, label: row.label });
    }
  }
  if (permissionsMissing(perms)) todo.push({ text: "macOS hasn't given Dettino every permission it needs yet.", action: () => go("advanced"), label: "Permissions" });
  if (snap.hotkeys_error) todo.push({ text: `Shortcuts are unavailable: ${snap.hotkeys_error}`, action: () => go("dictation"), label: "Dictation" });
  const where = locationWarning(snap.app_location);
  if (where) todo.push({ text: where, action: () => api.openDataFolder("applications"), label: "Open Applications" });
  const autostart = autostartProblem(snap.autostart, snap.platform);
  if (autostart) todo.push({ text: autostart, action: () => go("advanced"), label: "Startup" });
  if (!snap.history_available) todo.push({ text: "History is unavailable because the keychain could not be opened.", action: () => go("privacy"), label: "Privacy" });

  const today = useMemo(() => {
    const start = new Date();
    start.setHours(0, 0, 0, 0);
    const t = recent.filter((r) => r.created_at >= start.getTime() && r.final_text);
    const words = t.reduce((n, r) => n + (r.final_text?.split(/\s+/).filter(Boolean).length ?? 0), 0);
    const judged = recent.filter((r) => r.zero_edit !== null).slice(0, 50);
    const zero = judged.length ? Math.round((judged.filter((r) => r.zero_edit).length / judged.length) * 100) : null;
    const lat = recent.map((r) => r.timings.total_ms).filter((x): x is number => x != null).slice(0, 30).sort((a, b) => a - b);
    const p50 = lat.length ? lat[Math.floor(lat.length / 2)] : null;
    return { count: t.length, words, zero, p50 };
  }, [recent]);

  return (
    <div className="page">
      <div className="page-head hstack" style={{ justifyContent: "space-between", alignItems: "flex-start", flexWrap: "nowrap" }}>
        <div>
          <h1>{s.paused ? "Dictation is paused" : asrReady.length ? "Ready to dictate" : "Almost ready"}</h1>
          <p>
            {asrReady.length ? (
              <>
                Hold <Chord c={s.hotkeys.push_to_talk} /> in any text field, speak, and let go. Hearing with {name(asrReady[0])}
                {cleanup === "ready" ? `, cleaning up with ${name(s.llm_chain[0])}` : cleanup === "warming" ? ", the cleanup model is still loading" : ", no AI cleanup"}.
              </>
            ) : (
              `Choose how ${APP_NAME} hears you to start dictating.`
            )}
          </p>
        </div>
        <button
          className="btn small ghost"
          onClick={async () => setSnap(await api.setPaused(!s.paused))}
          aria-label={s.paused ? "Resume dictation" : "Pause dictation"}
        >
          {s.paused ? <><PlayCircle /> Resume</> : <><PauseCircle /> Pause</>}
        </button>
      </div>

      {ends?.warn && (
        <Banner kind="warn">
          {expiryWarning(ends)} Download a new one from github.com/hubertmk/dettino/releases; your settings and history stay.
        </Banner>
      )}

      {todo.length > 0 && (
        <div className="card">
          <div className="card-head">
            <h3>Finish setup</h3>
            <span className="sub">{todo.length} {todo.length === 1 ? "step" : "steps"} left</span>
            <span className="grow" style={{ flex: 1 }} />
            <button className="btn small" onClick={rerunSetup}>Run setup again</button>
          </div>
          {todo.map((t, i) => (
            <div className="row" key={i}>
              <div className="grow">{t.text}</div>
              <button className="btn small" onClick={t.action}>{t.label} <ArrowRight /></button>
            </div>
          ))}
        </div>
      )}

      <div className="grid3">
        <Stat label="Dictations today" value={String(today.count)} />
        <Stat label="Words today" value={String(today.words)} />
        <Stat label="Typical wait after release" value={ms(today.p50)} hint={today.zero !== null ? `${today.zero}% needed no edits` : undefined} />
      </div>

      {recent.length > 0 && (
        <div className="card">
          <div className="card-head">
            <h3>Recent</h3>
            <span className="grow" style={{ flex: 1 }} />
            <button className="btn small ghost" onClick={() => go("history")}>All history <ArrowRight /></button>
          </div>
          {recent.slice(0, 3).map((r) => (
            <div className="row" key={r.id}>
              <span className="small muted" style={{ width: 70 }}>{timeAgo(r.created_at)}</span>
              <div className="grow" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {r.final_text ?? r.raw_text ?? <span className="muted">{statusText(r.status)}</span>}
              </div>
            </div>
          ))}
        </div>
      )}

      {!asrReady.length && (
        <Banner>
          <b>Tip:</b> the Private mode downloads a 480 MB English model once and then works offline, with nothing leaving this computer. <button type="button" className="linkish" onClick={() => go("providers")}>Choose a mode</button>
        </Banner>
      )}
      <p className="small muted" style={{ textAlign: "center" }}>
        <Mic size={12} style={{ verticalAlign: -1 }} /> The {pillWord(s.pill_style)} at the bottom of your screen is {APP_NAME}. Hover it to pause, undo or paste the last transcript.
      </p>
      {ends && !ends.warn && <p className="small muted" style={{ textAlign: "center" }}>Test version · works until {ends.date}</p>}
    </div>
  );
}

export function statusText(s: HistoryRecord["status"]) {
  return {
    ok: "Pasted",
    ok_raw: "Pasted as heard",
    held: "Kept — not pasted",
    failed: "Failed — audio saved",
    cancelled: "Cancelled",
    nothing_heard: "Nothing heard",
    unprocessed: "Not processed yet",
  }[s];
}

function Stat(p: { label: string; value: string; hint?: string }) {
  return (
    <div className="card card-pad">
      <div className="small muted">{p.label}</div>
      <div style={{ fontSize: 20, fontWeight: 600, lineHeight: "26px", marginTop: 4 }}>{p.value}</div>
      {p.hint && <div className="small muted">{p.hint}</div>}
    </div>
  );
}
