import { useState } from "react";
import {
  ArrowDown, ArrowUp, Check, ChevronDown, ChevronRight, Cloud, Download, HardDrive, Lock, Plus, Trash2, Wallet, Zap, X,
} from "lucide-react";
import { api, DownloadProgress, formatBytes, isLocalType, kindOf, ModelStatus, needsKey, Preset, ProviderConfig, Snapshot, TestResult } from "../lib/api";
import { useModelProgress } from "../lib/progress";
import { OllamaSetup } from "./OllamaSetup";

export { useModelProgress };
import { APP_NAME, keychainName } from "../lib/brand";
import { useStore } from "../lib/store";
import { Banner, ms, Row, Seg, Sparkline, Toggle } from "../components/ui";

export function presetInfo(gpus: Snapshot["gpus"]) {
  const gpu = gpus.find((g) => g.vendor === "NVIDIA") ?? gpus[0];
  return [
    {
      id: "private" as Preset,
      title: "Private",
      sub: "everything on this computer",
      privacy: "Audio and text never leave this computer",
      speed: "About 1–2 s after you release" + (gpu ? ` (${gpu.name.replace(/NVIDIA |Generation |Laptop GPU/g, "").trim()})` : ""),
      cost: "Free · Dettino sets up Ollama for cleanup",
    },
    {
      id: "fast" as Preset,
      title: "Cloud",
      sub: "cloud hearing, cloud cleanup",
      privacy: "Audio goes to your transcription provider; text to your cleanup provider",
      speed: "Under 1 s on a good connection",
      cost: "≈ $0.006 per minute of speech + cleanup tokens",
    },
    {
      id: "balanced" as Preset,
      title: "Balanced",
      sub: "local hearing, cloud cleanup",
      privacy: "Audio stays here unless on-device transcription fails; the text goes to the cloud",
      speed: "About 1 s",
      cost: "Cleanup tokens only (cents per day)",
    },
  ];
}

/** Cloud mode: which cloud model cleans up. Transcription is OpenAI either way. */
export function CloudCleanupPick() {
  const { snap, update } = useStore();
  const s = snap.settings;
  if (s.preset !== "fast") return null;
  const options = [
    { value: "anthropic", label: "Anthropic Claude" },
    { value: "openai-chat", label: "OpenAI" },
    { value: "perplexity", label: "Perplexity" },
  ].filter((o) => s.providers.some((p) => p.id === o.value));
  const first = s.llm_chain[0] ?? "";
  const desc =
    first === "openai-chat"
      ? "Uses the same OpenAI key as transcription."
      : "Transcription stays with OpenAI, which needs its own key. The others follow as fallbacks.";
  return (
    <Row title="Clean up with" desc={desc}>
      <Seg
        label="Clean up with"
        value={first}
        options={options}
        onChange={(v) => update((x) => { x.llm_chain = [v, ...x.llm_chain.filter((id) => id !== v)]; })}
      />
    </Row>
  );
}

export function PresetCards(p: { value: Preset; onChange: (p: Preset) => void; gpus: Snapshot["gpus"]; recommended?: Preset }) {
  return (
    <div className="preset-cards" role="radiogroup" aria-label="Mode">
      {presetInfo(p.gpus).map((x) => (
        <button key={x.id} role="radio" aria-checked={p.value === x.id} className={`preset ${p.value === x.id ? "on" : ""}`} onClick={() => p.onChange(x.id)}>
          <h4>
            {x.title}
            {p.value === x.id && <Check size={15} color="var(--accent)" />}
          </h4>
          <span className="small muted hstack">
            {x.sub}
            {p.recommended === x.id && <span className="chip info">Recommended for this computer</span>}
          </span>
          <div className="tradeoff">
            <Lock /> <span>{x.privacy}</span>
            <Zap /> <span>{x.speed}</span>
            <Wallet /> <span>{x.cost}</span>
          </div>
        </button>
      ))}
    </div>
  );
}

function statusChip(p: ProviderConfig, snap: Snapshot) {
  const st = snap.stats[p.id];
  if (needsKey(p.type) && !snap.keys[p.id]) return <span className="chip warn">Needs key</span>;
  if ((p.type === "parakeet_local" || p.type === "whisper_local") && !snap.ready[p.id]) return <span className="chip warn">Model not downloaded</span>;
  if (st?.last_error && st.last_error_ts && Date.now() - st.last_error_ts < 10 * 60_000 && st.recent_ms.length === 0)
    return <span className="chip err">Error · {st.last_error}</span>;
  if (st?.last_error && st.last_error_ts && Date.now() - st.last_error_ts < 10 * 60_000)
    return <span className="chip warn">Failed {Math.max(1, Math.round((Date.now() - st.last_error_ts) / 60000))} min ago · {st.last_error}</span>;
  return <span className="chip ok">Ready</span>;
}

export function ModelRow({ m, prog, busy, onDownload, onDelete }: { m: ModelStatus; prog?: DownloadProgress; busy: boolean; onDownload: () => void; onDelete?: () => void }) {
  const pct = prog && prog.total ? Math.round((prog.done / prog.total) * 100) : 0;
  return (
    <div className="row">
      <div className="grow">
        <div className="title hstack">
          {m.name}
          {m.recommended && <span className="chip info">Recommended</span>}
          {m.english_only && <span className="chip">English</span>}
        </div>
        <div className="desc">{m.description} · {formatBytes(m.bytes)} · {m.license}</div>
        {busy && prog && (
          <div style={{ marginTop: 8 }}>
            <div className="progress"><i style={{ width: `${pct}%` }} /></div>
            <div className="small muted" style={{ marginTop: 4 }}>
              {prog.stage === "installing" ? "Installing…" : `${formatBytes(prog.done)} of ${formatBytes(prog.total)}`}
            </div>
          </div>
        )}
      </div>
      {m.installed ? (
        <div className="hstack">
          <span className="chip ok"><Check /> Installed</span>
          {onDelete && <button className="btn small ghost" onClick={onDelete} aria-label={`Delete ${m.name}`}><Trash2 /></button>}
        </div>
      ) : (
        <button className="btn small" onClick={onDownload} disabled={busy}>
          <Download /> {busy ? "Downloading…" : "Download"}
        </button>
      )}
    </div>
  );
}

function ProviderEditor({ p, onClose }: { p: ProviderConfig; onClose: () => void }) {
  const { snap, update, setSnap, toast } = useStore();
  const [key, setKey] = useState("");
  const edit = (fn: (x: ProviderConfig) => void) =>
    update((s) => {
      const x = s.providers.find((q) => q.id === p.id);
      if (x) fn(x);
    });
  const showBase = p.type !== "parakeet_local" && p.type !== "whisper_local";
  return (
    <div className="card-pad stack" style={{ background: "var(--surface-2)", borderTop: "1px solid var(--border)" }}>
      {needsKey(p.type) && (
        <div className="field">
          <label htmlFor={`key-${p.id}`}>API key</label>
          <div className="hstack">
            <input
              id={`key-${p.id}`}
              className="input"
              style={{ flex: 1 }}
              type="password"
              autoComplete="off"
              placeholder={snap.keys[p.id] ? "•••••••• saved — paste a new key to replace" : "Paste your key"}
              value={key}
              onChange={(e) => setKey(e.target.value)}
            />
            <button
              className="btn"
              disabled={!key.trim()}
              onClick={async () => {
                try {
                  setSnap(await api.setProviderKey(p.id, key));
                  setKey("");
                  toast("Key saved in the system keychain");
                } catch (e) {
                  toast(String(e));
                }
              }}
            >
              Save
            </button>
            {snap.keys[p.id] && (
              <button className="btn ghost danger" onClick={async () => setSnap(await api.setProviderKey(p.id, ""))}>Remove</button>
            )}
          </div>
          <span className="small muted">Stored in the {keychainName(snap.platform)}, never in settings files or logs.</span>
        </div>
      )}
      <div className="grid2">
        {showBase && (
          <div className="field">
            <label htmlFor={`${p.id}-base`}>Server address</label>
            <input id={`${p.id}-base`} className="input" value={p.base_url ?? ""} onChange={(e) => edit((x) => (x.base_url = e.target.value))} />
          </div>
        )}
        {p.type === "perplexity_agent" ? (
          <div className="field" style={{ gridColumn: "span 2" }}>
            <label htmlFor={`${p.id}-models`}>Models, in fallback order (up to 5)</label>
            <input
              id={`${p.id}-models`}
              className="input"
              value={p.models.join(", ")}
              onChange={(e) => edit((x) => (x.models = e.target.value.split(",").map((m) => m.trim()).filter(Boolean).slice(0, 5)))}
            />
            <span className="small muted">Perplexity routes to these models in order, e.g. anthropic/claude-haiku-5-5, openai/gpt-5.5, google/gemini-3.5-flash, perplexity/sonar.</span>
          </div>
        ) : p.type === "ollama" ? null : p.type === "parakeet_local" ? (
          <div className="field">
            <label htmlFor={`${p.id}-model`}>Model</label>
            <select id={`${p.id}-model`} className="select" value={p.model} onChange={(e) => edit((x) => (x.model = e.target.value))}>
              <option value="parakeet-tdt-0.6b-v2-int8">Parakeet TDT 0.6B v2 (English)</option>
              <option value="parakeet-tdt-0.6b-v3-int8">Parakeet TDT 0.6B v3 (multilingual)</option>
            </select>
          </div>
        ) : p.type === "whisper_local" ? (
          <div className="field">
            <label htmlFor={`${p.id}-model`}>Model</label>
            <select id={`${p.id}-model`} className="select" value={p.model} onChange={(e) => edit((x) => (x.model = e.target.value))}>
              <option value="whisper-turbo">Whisper Large v3 Turbo</option>
              <option value="whisper-small.en">Whisper Small (English)</option>
            </select>
          </div>
        ) : (
          <div className="field">
            <label htmlFor={`${p.id}-model`}>Model</label>
            <input id={`${p.id}-model`} className="input" value={p.model} onChange={(e) => edit((x) => (x.model = e.target.value))} />
          </div>
        )}
      </div>
      {p.type === "ollama" && <OllamaSetup base={p.base_url ?? "http://localhost:11434"} compact />}
      {p.type === "ollama" && (
        <label className="hstack small">
          <Toggle on={snap.settings.start_ollama} onChange={(v) => update((x) => (x.start_ollama = v))} label={`Start Ollama with ${APP_NAME}`} />
          Start Ollama in the background when {APP_NAME} starts, or when it stops answering
        </label>
      )}
      {(p.type === "openai_compat_asr" || p.type === "openai_compat_chat") && (
        <div className="hstack" style={{ gap: 24 }}>
          {p.type === "openai_compat_asr" && (
            <label className="hstack small"><Toggle on={p.accepts_prompt} onChange={(v) => edit((x) => (x.accepts_prompt = v))} label="Server accepts a prompt" /> Server accepts a prompt</label>
          )}
          <label className="hstack small"><Toggle on={p.treat_as_local} onChange={(v) => edit((x) => (x.treat_as_local = v))} label="Runs on this computer or my network" /> Runs on this computer or my own network</label>
        </div>
      )}
      <div className="hstack" style={{ justifyContent: "flex-end" }}>
        <button className="btn small" onClick={onClose}>Done</button>
      </div>
    </div>
  );
}

function ProviderCard({ p, rank, chain, kind }: { p: ProviderConfig; rank: number | null; chain: string[]; kind: "asr" | "llm" }) {
  const { snap, update } = useStore();
  const [open, setOpen] = useState(false);
  const [test, setTest] = useState<TestResult | "running" | null>(null);
  const st = snap.stats[p.id];
  const local = isLocalType(p.type) || p.treat_as_local;
  const field = kind === "asr" ? "asr_chain" : "llm_chain";
  const move = (dir: -1 | 1) =>
    update((s) => {
      const c = s[field];
      const i = c.indexOf(p.id);
      const j = i + dir;
      if (i < 0 || j < 0 || j >= c.length) return;
      [c[i], c[j]] = [c[j], c[i]];
      s.preset = "custom";
    });
  const toggleInChain = () =>
    update((s) => {
      const c = s[field];
      if (c.includes(p.id)) s[field] = c.filter((x) => x !== p.id);
      else c.push(p.id);
      s.preset = "custom";
    });
  return (
    <div>
      <div className="pcard">
        <span className={`rank ${rank === 0 ? "first" : ""}`}>{rank === null ? "–" : rank + 1}</span>
        <div className="grow" style={{ flex: 1, minWidth: 0 }}>
          <div className="hstack">
            <b style={{ fontWeight: 600 }}>{p.display_name}</b>
            <span className="chip">{local ? <><HardDrive /> On this computer</> : <><Cloud /> Cloud</>}</span>
            {rank !== null && statusChip(p, snap)}
          </div>
          <div className="small muted" style={{ marginTop: 2 }}>
            {p.type === "perplexity_agent" ? p.models.join(" → ") : p.model}
            {st && st.p50_ms != null && ` · typical ${ms(st.p50_ms)}, slowest ${ms(st.p95_ms)}`}
          </div>
          {test && test !== "running" && (
            <div className={`small ${test.ok ? "" : ""}`} style={{ marginTop: 6, color: test.ok ? "var(--success)" : "var(--danger)" }}>
              {test.ok ? <><Check size={13} /> "{test.text}" · {ms(test.ms)}</> : <><X size={13} /> {test.error}</>}
            </div>
          )}
          {test === "running" && kind === "asr" && <div className="small" style={{ marginTop: 6, color: "var(--accent)" }}>Say anything for three seconds…</div>}
        </div>
        {st && <Sparkline values={st.recent_ms} />}
        {rank !== null && (
          <>
            <button className="btn small" disabled={test === "running"} onClick={async () => { setTest("running"); setTest(await api.testProvider(p.id)); }}>
              {test === "running" ? "Testing…" : "Test"}
            </button>
            <button className="btn small ghost" aria-label="Move up" disabled={rank === 0} onClick={() => move(-1)}><ArrowUp /></button>
            <button className="btn small ghost" aria-label="Move down" disabled={rank === chain.length - 1} onClick={() => move(1)}><ArrowDown /></button>
          </>
        )}
        <button className="btn small ghost" aria-label={rank === null ? "Add to chain" : "Remove from chain"} onClick={toggleInChain} title={rank === null ? "Use this provider" : "Stop using this provider"}>
          {rank === null ? <Plus /> : <X />}
        </button>
        <button className="btn small ghost" aria-expanded={open} aria-label="Configure" onClick={() => setOpen(!open)}>
          {open ? <ChevronDown /> : <ChevronRight />}
        </button>
      </div>
      {open && <ProviderEditor p={p} onClose={() => setOpen(false)} />}
    </div>
  );
}

function ChainCard({ kind }: { kind: "asr" | "llm" }) {
  const { snap } = useStore();
  const s = snap.settings;
  const chain = kind === "asr" ? s.asr_chain : s.llm_chain;
  const all = s.providers.filter((p) => kindOf(p.type) === kind);
  const inChain = chain.map((id) => all.find((p) => p.id === id)).filter(Boolean) as ProviderConfig[];
  const others = all.filter((p) => !chain.includes(p.id));
  const privateCloud = s.preset === "private" && inChain.some((p) => !isLocalType(p.type) && !p.treat_as_local);
  return (
    <div className="card">
      <div className="card-head">
        <h3>{kind === "asr" ? `Transcription — how ${APP_NAME} hears you` : "Cleanup — how the text is polished"}</h3>
      </div>
      {inChain.length === 0 && (
        <div className="row"><span className="muted">Nothing in use. Add a provider below{kind === "llm" ? " — or keep cleanup off; dictations are then pasted as heard." : "."}</span></div>
      )}
      {inChain.map((p, i) => <ProviderCard key={p.id} p={p} rank={i} chain={chain} kind={kind} />)}
      <div className="row small muted" style={{ minHeight: 36 }}>
        {(() => {
          const ready = inChain.filter((p) => snap.ready[p.id]);
          const skipped = inChain.filter((p) => !snap.ready[p.id]);
          const skipNote = skipped.length ? ` ${skipped.map((p) => p.display_name).join(" and ")} ${skipped.length === 1 ? "is" : "are"} skipped until set up.` : "";
          if (ready.length > 1) return `If ${ready[0].display_name} fails or is slow, ${ready[1].display_name} takes over automatically.${skipNote}`;
          if (ready.length === 1)
            return (kind === "llm" ? "If cleanup fails, your words are pasted as heard (with punctuation) — never lost." : "Set up a second provider to get automatic fallback.") + skipNote;
          return kind === "llm" ? "No cleanup provider is ready — dictations are pasted as heard." + skipNote : "No transcription provider is ready yet." + skipNote;
        })()}
      </div>
      {privateCloud && <div className="card-pad"><Banner kind="warn">A cloud provider is in this chain, so this is no longer fully private.</Banner></div>}
      {others.length > 0 && (
        <>
          <div className="row small" style={{ minHeight: 32, background: "var(--surface-2)", fontWeight: 600, color: "var(--text-2)" }}>Not in use</div>
          {others.map((p) => <ProviderCard key={p.id} p={p} rank={null} chain={chain} kind={kind} />)}
        </>
      )}
    </div>
  );
}

export function Providers() {
  const { snap, setSnap, update, toast } = useStore();
  const prog = useModelProgress();
  const [busy, setBusy] = useState<string | null>(null);
  const s = snap.settings;
  const models = snap.models.filter((m) => m.family !== "vad");
  const vad = snap.models.find((m) => m.family === "vad");
  const download = async (id: string) => {
    setBusy(id);
    try {
      setSnap(await api.modelDownload(id));
      toast("Model installed");
    } catch (e) {
      toast(String(e));
    } finally {
      setBusy(null);
    }
  };
  return (
    <div className="page">
      <div className="page-head">
        <h1>Providers</h1>
        <p>Choose who hears you and who polishes the text. Mix local and cloud freely; the first that answers wins.</p>
      </div>
      <PresetCards value={s.preset} onChange={async (p) => setSnap(await api.setPreset(p))} gpus={snap.gpus} />
      {s.preset === "custom" && <p className="small muted">Custom pairing — pick a mode above to reset both chains.</p>}
      {s.preset === "fast" && <div className="card"><CloudCleanupPick /></div>}

      <ChainCard kind="asr" />
      <ChainCard kind="llm" />

      <div className="card">
        <div className="card-head">
          <h3>Models on this computer</h3>
          <span className="sub">
            {snap.gpus.length ? snap.gpus.map((g) => `${g.name}${g.vram_mb ? ` (${Math.round(g.vram_mb / 1024)} GB)` : ""}`).join(", ") : "No GPU detected"}
            {snap.ram_mb > 0 ? ` · ${Math.round(snap.ram_mb / 1024)} GB RAM` : ""} · {snap.cpu_threads} CPU threads
          </span>
        </div>
        {vad && <ModelRow m={vad} prog={prog[vad.id]} busy={busy === vad.id} onDownload={() => download(vad.id)} />}
        {models.map((m) => (
          <ModelRow key={m.id} m={m} prog={prog[m.id]} busy={busy === m.id} onDownload={() => download(m.id)} onDelete={async () => setSnap(await api.modelDelete(m.id))} />
        ))}
      </div>

      <div className="card">
        <div className="card-head"><h3>Fallback timing</h3></div>
        <div className="row">
          <div className="grow">
            <div className="title">Start the next provider if no answer after</div>
            <div className="desc">Automatic uses 1.5 × each provider's recent slowest time (at least 1.2 s).</div>
          </div>
          <select
            className="select"
            aria-label="Start the next provider after"
            value={s.hedge_ms ?? "auto"}
            onChange={(e) => update((x) => (x.hedge_ms = e.target.value === "auto" ? null : Number(e.target.value)))}
          >
            <option value="auto">Automatic</option>
            {[800, 1200, 2000, 3000, 5000].map((v) => <option key={v} value={v}>{v / 1000} s</option>)}
          </select>
        </div>
        <div className="row">
          <div className="grow">
            <div className="title">Wait for a model on this computer up to</div>
            <div className="desc">Automatic waits about 1.5 × the model's measured speed (up to 15 s) and shows a notice on the pill while it waits.</div>
          </div>
          <select
            className="select"
            aria-label="Wait for a model on this computer up to"
            value={s.cleanup_wait_ms ?? "auto"}
            onChange={(e) => update((x) => (x.cleanup_wait_ms = e.target.value === "auto" ? null : Number(e.target.value)))}
          >
            <option value="auto">Automatic</option>
            {[3000, 5000, 8000, 12000, 20000].map((v) => <option key={v} value={v}>{v / 1000} s</option>)}
          </select>
        </div>
      </div>
    </div>
  );
}
