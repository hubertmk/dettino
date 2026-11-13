import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Copy, History as HistoryIcon, Play, RotateCcw, Search, Trash2 } from "lucide-react";
import { api, HistoryRecord } from "../lib/api";
import { useStore } from "../lib/store";
import { Banner, Chord, Empty, ms, Seg, timeAgo } from "../components/ui";
import { statusText } from "./Home";

/** Word-level diff (LCS) for the Raw / Cleaned comparison. */
export function wordDiff(a: string, b: string): { t: "eq" | "del" | "ins"; s: string }[] {
  const A = a.split(/(\s+)/).filter((x) => x !== "");
  const B = b.split(/(\s+)/).filter((x) => x !== "");
  const n = A.length, m = B.length;
  if (n * m > 400_000) return [{ t: "del", s: a }, { t: "ins", s: b }];
  const dp = Array.from({ length: n + 1 }, () => new Uint16Array(m + 1));
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) dp[i][j] = A[i] === B[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
  const out: { t: "eq" | "del" | "ins"; s: string }[] = [];
  let i = 0, j = 0;
  const push = (t: "eq" | "del" | "ins", s: string) => {
    const last = out[out.length - 1];
    if (last && last.t === t) last.s += s;
    else out.push({ t, s });
  };
  while (i < n && j < m) {
    if (A[i] === B[j]) { push("eq", A[i]); i++; j++; }
    else if (dp[i + 1][j] >= dp[i][j + 1]) { push("del", A[i]); i++; }
    else { push("ins", B[j]); j++; }
  }
  while (i < n) push("del", A[i++]);
  while (j < m) push("ins", B[j++]);
  return out;
}

function Waterfall({ r }: { r: HistoryRecord }) {
  const t = r.timings;
  const parts: [string, number | null, string?][] = [
    ["Transcribe", t.asr_ms, t.asr_provider ?? undefined],
    ["Clean up", t.cleanup_ms, t.cleanup_skipped ? `skipped: ${t.cleanup_skipped}` : t.cleanup_model ?? t.cleanup_provider ?? undefined],
    ["Paste", t.insert_ms, t.insert_verify ?? undefined],
  ];
  const total = Math.max(1, parts.reduce((n, p) => n + (p[1] ?? 0), 0));
  let offset = 0;
  return (
    <div className="waterfall">
      <div className="small muted">
        Held {ms(t.hold_ms)} · speech {ms(t.speech_ms)} · pre-roll {ms(t.preroll_used_ms)} · release → text {ms(t.total_ms)}
      </div>
      {parts.map(([label, v, note]) => {
        const left = (offset / total) * 100;
        const width = ((v ?? 0) / total) * 100;
        offset += v ?? 0;
        return (
          <div className="wf-row" key={label}>
            <span>{label}</span>
            <div className="wf-bar" title={note}>
              <i style={{ left: `${left}%`, width: `${Math.max(width, v ? 1 : 0)}%` }} />
            </div>
            <span className="small muted">{ms(v)}</span>
          </div>
        );
      })}
      {t.asr_attempts.length > 1 && <div className="small muted">Attempts: {t.asr_attempts.join(" · ")}</div>}
      {t.cleanup_rejected && <div className="small" style={{ color: "var(--warning)" }}>Cleanup rejected: {t.cleanup_rejected}</div>}
      {t.clipboard_restored === false && <div className="small muted">Clipboard was left as is (you copied something meanwhile, or it held content that can't be restored).</div>}
    </div>
  );
}

function Detail({ r, onDeleted }: { r: HistoryRecord; onDeleted: () => void }) {
  const { toast } = useStore();
  const [view, setView] = useState<"cleaned" | "raw" | "diff">(r.cleaned_text || r.final_text ? "cleaned" : "raw");
  const [audioUrl, setAudioUrl] = useState<string | null>(null);
  const [audioErr, setAudioErr] = useState<string | null>(null);
  const [retrying, setRetrying] = useState(false);
  const urlRef = useRef<string | null>(null);
  useEffect(() => {
    setAudioUrl(null);
    setAudioErr(null);
    setView(r.cleaned_text || r.final_text ? "cleaned" : "raw");
    return () => {
      if (urlRef.current) URL.revokeObjectURL(urlRef.current);
    };
  }, [r.id]);
  const loadAudio = async () => {
    try {
      const buf = await api.historyAudio(r.id);
      const url = URL.createObjectURL(new Blob([buf], { type: "audio/wav" }));
      urlRef.current = url;
      setAudioUrl(url);
    } catch (e) {
      setAudioErr(String(e));
    }
  };
  const finalText = r.final_text ?? r.cleaned_text ?? "";
  const raw = r.raw_text ?? "";
  return (
    <div className="card">
      <div className="card-head">
        <h3>{r.mode === "command" ? "Edit by voice" : "Dictation"}</h3>
        <span className="sub">{new Date(r.created_at).toLocaleString()} · {r.app ?? "unknown app"}</span>
        <span style={{ flex: 1 }} />
        <span className={`chip ${r.status === "ok" ? "ok" : r.status === "failed" ? "err" : "warn"}`}>{statusText(r.status)}</span>
      </div>
      <div className="card-pad stack">
        {r.error && <Banner kind={r.status === "failed" ? "err" : "warn"}>{r.error}</Banner>}
        {r.timings.cleanup_skipped && r.status === "ok_raw" && <Banner kind="warn">Pasted as heard: {r.timings.cleanup_skipped}</Banner>}
        {(raw || finalText) && (
          <Seg
            label="Version"
            value={view}
            onChange={setView}
            options={[{ value: "cleaned", label: r.mode === "command" ? "Result" : "Cleaned" }, { value: "raw", label: r.mode === "command" ? "Instruction" : "Raw" }, { value: "diff", label: "Diff" }]}
          />
        )}
        <div className="diff" style={{ minHeight: 40 }}>
          {view === "raw" && (raw || <span className="muted">No transcript.</span>)}
          {view === "cleaned" && (finalText || <span className="muted">No cleaned version.</span>)}
          {view === "diff" && wordDiff(raw, finalText.trim()).map((d, i) => (d.t === "eq" ? <span key={i}>{d.s}</span> : d.t === "del" ? <del key={i}>{d.s}</del> : <ins key={i}>{d.s}</ins>))}
        </div>
        {r.edited_text && r.zero_edit === false && (
          <div className="small muted">You edited it afterwards to: <span className="mono">{r.edited_text}</span></div>
        )}
        <div className="hstack">
          {finalText && <button className="btn small" onClick={async () => { await api.copyText(finalText.trim()); toast("Copied"); }}><Copy /> Copy</button>}
          {raw && <button className="btn small ghost" onClick={async () => { await api.copyText(raw); toast("Raw text copied"); }}>Copy raw</button>}
          {r.audio_path && (
            <button className="btn small" disabled={retrying} onClick={async () => { setRetrying(true); try { await api.historyRetry(r.id); toast("Transcribing again…"); } catch (e) { toast(String(e)); } setTimeout(() => setRetrying(false), 1500); }}>
              <RotateCcw /> {r.status === "failed" || r.status === "unprocessed" || r.status === "cancelled" ? "Transcribe" : "Transcribe again"}
            </button>
          )}
          <span style={{ flex: 1 }} />
          <button className="btn small ghost danger" onClick={async () => { await api.historyDelete(r.id); onDeleted(); }} aria-label="Delete"><Trash2 /></button>
        </div>
        {r.audio_path ? (
          audioUrl ? <audio controls src={audioUrl} style={{ width: "100%" }} autoPlay /> : <button className="btn small ghost" onClick={loadAudio}><Play /> Play recording ({ms(r.audio_ms)})</button>
        ) : (
          <span className="small muted">Recording deleted after the retention period.</span>
        )}
        {audioErr && <span className="small" style={{ color: "var(--danger)" }}>{audioErr}</span>}
        {r.timings.total_ms != null && <Waterfall r={r} />}
      </div>
    </div>
  );
}

export function History({ focusId }: { focusId: string | null }) {
  const { snap } = useStore();
  const [q, setQ] = useState("");
  const [items, setItems] = useState<HistoryRecord[]>([]);
  const [sel, setSel] = useState<string | null>(focusId);
  const [filter, setFilter] = useState<"all" | "issues">("all");
  const load = () => api.historyList(q || undefined, 300).then(setItems);
  useEffect(() => {
    const t = setTimeout(load, 150);
    return () => clearTimeout(t);
  }, [q]);
  useEffect(() => {
    const un = listen("history:changed", load);
    return () => {
      un.then((u) => u());
    };
  }, [q]);
  useEffect(() => {
    if (focusId) setSel(focusId);
  }, [focusId]);
  const shown = useMemo(() => (filter === "issues" ? items.filter((r) => r.status !== "ok") : items), [items, filter]);
  const selected = items.find((r) => r.id === sel) ?? null;
  return (
    <div className="page" style={{ maxWidth: 820 }}>
      <div className="page-head">
        <h1>History</h1>
        <p>Every dictation with what you said, what was pasted, and how long each step took. Encrypted on this computer.</p>
      </div>
      <div className="hstack">
        <div className="search" style={{ flex: 1, margin: 0 }}>
          <Search />
          <input placeholder="Search raw and cleaned text" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search history" />
        </div>
        <Seg label="Filter" value={filter} onChange={setFilter} options={[{ value: "all", label: "All" }, { value: "issues", label: "Needs attention" }]} />
      </div>
      {selected && <Detail r={selected} onDeleted={() => { setSel(null); load(); }} />}
      <div className="card">
        {shown.length === 0 ? (
          q ? (
            <Empty icon={<Search />} title={`No matches for "${q}"`}>Search looks at both raw and cleaned text.</Empty>
          ) : (
            <Empty icon={<HistoryIcon />} title="Nothing dictated yet">
              Hold <Chord c={snap.settings.hotkeys.push_to_talk} /> in any text field and say something. Every dictation shows up here with the raw and cleaned versions and how long each step took.
            </Empty>
          )
        ) : (
          <div className="hist">
            {shown.map((r) => (
              <div key={r.id} className={`hist-item ${r.id === sel ? "sel" : ""}`} onClick={() => setSel(r.id)} role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); setSel(r.id); } }}>
                <span className="when">{timeAgo(r.created_at)}</span>
                <span className="text">{r.final_text?.trim() || r.raw_text || <span className="muted">{r.error ?? statusText(r.status)}</span>}</span>
                {r.status !== "ok" && <span className={`chip ${r.status === "failed" ? "err" : "warn"}`}>{statusText(r.status)}</span>}
                {r.timings.total_ms != null && <span className="small muted">{ms(r.timings.total_ms)}</span>}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
