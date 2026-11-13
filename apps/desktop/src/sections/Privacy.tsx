import { useState } from "react";
import { Eye, Trash2 } from "lucide-react";
import { api, ContextPreview, ContextSettings } from "../lib/api";
import { APP_NAME } from "../lib/brand";
import { useStore } from "../lib/store";
import { Banner, Row, Toggle } from "../components/ui";

const ITEMS: { key: keyof ContextSettings; title: string; desc: string }[] = [
  { key: "app_name", title: "App name", desc: "So emails, chats and code get the right tone and formatting." },
  { key: "window_title", title: "Window title", desc: "Helps with names, e.g. the person you are chatting with." },
  { key: "caret_text", title: "Text around the cursor", desc: "Up to 300 characters before and 100 after: continues sentences correctly." },
  { key: "selection", title: "Selected text", desc: "Needed for editing by voice." },
  { key: "visible_text", title: "Names on screen", desc: "Proper nouns from the window (extracted on this computer) to spell names right." },
];

export function Privacy() {
  const { snap, update, toast } = useStore();
  const s = snap.settings;
  const [preview, setPreview] = useState<ContextPreview | "waiting" | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const cloudInUse = [...s.asr_chain, ...s.llm_chain].map((id) => s.providers.find((p) => p.id === id)).filter((p) => p && !(p.type === "parakeet_local" || p.type === "whisper_local" || p.type === "ollama" || p.treat_as_local));
  return (
    <div className="page">
      <div className="page-head">
        <h1>Context & privacy</h1>
        <p>What {APP_NAME} reads from your screen, who receives it, and what is kept on this computer.</p>
      </div>
      <Banner kind="ok">
        No accounts. No telemetry. No screenshots. Nothing is sent anywhere except to the providers you chose
        {cloudInUse.length ? ` (${cloudInUse.map((p) => p!.display_name).join(", ")})` : " — right now, all of them run on this computer"}.
      </Banner>

      <div className="card">
        <div className="card-head">
          <h3>Screen context</h3>
          <span style={{ flex: 1 }} />
          <Toggle label="Read screen context" on={s.context.enabled} onChange={(v) => update((x) => (x.context.enabled = v))} />
        </div>
        {ITEMS.map((it) => (
          <Row key={it.key} title={it.title} desc={it.desc}>
            <Toggle label={it.title} disabled={!s.context.enabled} on={s.context[it.key] as boolean} onChange={(v) => update((x) => ((x.context[it.key] as boolean) = v))} />
          </Row>
        ))}
        <Row title="Send context only to providers on this computer" desc="Cloud providers then get the audio or the transcript, but nothing from your screen.">
          <Toggle label="Context only to local providers" on={s.context.local_only} onChange={(v) => update((x) => (x.context.local_only = v))} />
        </Row>
        <div className="row small muted">Never read, always: password fields, browser address bars, and apps you marked "Never dictate" in App rules.</div>
        <div className="row">
          <div className="grow">
            <div className="title">What would be sent?</div>
            <div className="desc">Click, then switch to any app within 3 seconds. You'll see exactly what {APP_NAME} would read there.</div>
          </div>
          <button className="btn" disabled={preview === "waiting"} onClick={async () => { setPreview("waiting"); setPreview(await api.previewContext()); }}>
            <Eye /> {preview === "waiting" ? "Switch to an app…" : "Preview"}
          </button>
        </div>
        {preview && preview !== "waiting" && (
          <div className="card-pad stack" style={{ background: "var(--surface-2)", borderTop: "1px solid var(--border)" }}>
            <div className="small"><b>App:</b> {preview.app || "unknown"} · <b>Window:</b> {preview.title || "—"}</div>
            {preview.context.is_password && <Banner kind="ok">This is a password field — nothing is read and nothing would be pasted.</Banner>}
            {!preview.rule_context && <Banner>Context is turned off for this app in App rules.</Banner>}
            <div className="small"><b>Before the cursor:</b> <span className="mono">{preview.context.before ?? "—"}</span></div>
            <div className="small"><b>After the cursor:</b> <span className="mono">{preview.context.after ?? "—"}</span></div>
            <div className="small"><b>Selection:</b> <span className="mono">{preview.context.selection ?? "—"}</span></div>
            <div className="small"><b>Names on screen:</b> {preview.nouns.length ? preview.nouns.join(", ") : "—"}</div>
            <div className="small"><b>URL:</b> {preview.context.url ?? "—"}</div>
            <div className="small">
              <b>Who gets it:</b>{" "}
              {preview.providers.map(([name, gets], i) => (
                <span key={i}>{i > 0 && " · "}{name}: {gets ? "context" : "no context"}</span>
              ))}
            </div>
            <div className="small muted">Read in {preview.context.ms} ms{preview.context.timed_out ? " (time limit reached — partial)" : ""}.</div>
          </div>
        )}
      </div>

      <div className="card">
        <div className="card-head"><h3>History on this computer</h3></div>
        <Row title="Keep history" desc="Raw and cleaned text, app and timings, encrypted with a key stored in the system keychain.">
          <Toggle label="Keep history" on={s.store_history} onChange={(v) => update((x) => (x.store_history = v))} />
        </Row>
        <Row title="Keep recordings for" desc="So you can replay or retry a dictation. Deleted automatically afterwards.">
          <select className="select" aria-label="Keep recordings for" value={s.audio_retention_days} onChange={(e) => update((x) => (x.audio_retention_days = Number(e.target.value)))}>
            <option value={1}>1 day</option>
            <option value={3}>3 days</option>
            <option value={7}>7 days</option>
          </select>
        </Row>
        <Row title="Delete everything" desc="History, recordings and statistics. Settings and keys stay.">
          {confirmDelete ? (
            <div className="hstack">
              <button className="btn small" onClick={() => setConfirmDelete(false)}>Keep</button>
              <button className="btn small danger" onClick={async () => { await api.historyDeleteAll(); setConfirmDelete(false); toast("History deleted"); }}>
                <Trash2 /> Delete now
              </button>
            </div>
          ) : (
            <button className="btn small danger" onClick={() => setConfirmDelete(true)}><Trash2 /> Delete…</button>
          )}
        </Row>
      </div>
    </div>
  );
}
