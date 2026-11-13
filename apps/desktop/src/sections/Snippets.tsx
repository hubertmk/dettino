import { Plus, Sparkles, Trash2 } from "lucide-react";
import { useStore } from "../lib/store";
import { Empty, Toggle } from "../components/ui";

const EXAMPLES = [
  { trigger: "my email", body: "you@example.com", enabled: true },
  { trigger: "today's date", body: "{date:%B %-d, %Y}", enabled: true },
  { trigger: "sign off", body: "Best regards,\nYour Name", enabled: true },
];

export function Snippets() {
  const { snap, update } = useStore();
  const list = snap.settings.snippets;
  return (
    <div className="page">
      <div className="page-head">
        <h1>Snippets</h1>
        <p>Say a phrase, paste a block. If you say only the trigger, exactly the snippet is pasted; inside a sentence it is replaced inline.</p>
      </div>
      <div className="card">
        {list.length === 0 ? (
          <Empty
            icon={<Sparkles />}
            title="Say a phrase, paste a block"
            action={
              <div className="hstack">
                <button className="btn" onClick={() => update((x) => x.snippets.push({ trigger: "", body: "", enabled: true }))}><Plus /> Create snippet</button>
                <button className="btn ghost" onClick={() => update((x) => x.snippets.push(...EXAMPLES))}>Add examples</button>
              </div>
            }
          >
            "My address" → your address. Variables like {"{date}"} fill in automatically.
          </Empty>
        ) : (
          <>
            {list.map((sn, i) => (
              <div className="row" key={i} style={{ alignItems: "flex-start" }}>
                <div className="stack grow" style={{ gap: 6 }}>
                  <input className="input" aria-label="Trigger phrase" placeholder="Trigger phrase, e.g. my address" value={sn.trigger} onChange={(e) => update((x) => (x.snippets[i].trigger = e.target.value))} />
                  <textarea className="textarea" rows={2} aria-label="Text to paste" placeholder="Text to paste" value={sn.body} onChange={(e) => update((x) => (x.snippets[i].body = e.target.value))} />
                </div>
                <Toggle label={`Use ${sn.trigger}`} on={sn.enabled} onChange={(v) => update((x) => (x.snippets[i].enabled = v))} />
                <button className="btn small ghost" aria-label="Delete snippet" onClick={() => update((x) => x.snippets.splice(i, 1))}><Trash2 /></button>
              </div>
            ))}
            <div className="row">
              <button className="btn small" onClick={() => update((x) => x.snippets.push({ trigger: "", body: "", enabled: true }))}><Plus /> Add snippet</button>
            </div>
          </>
        )}
      </div>
      <div className="card card-pad small muted">
        Variables: <span className="mono">{"{date}"}</span>, <span className="mono">{"{time}"}</span>, <span className="mono">{"{date:%d.%m.%Y}"}</span> (any strftime format),{" "}
        <span className="mono">{"{clipboard}"}</span>, <span className="mono">{"{selection}"}</span>, <span className="mono">{"{app}"}</span>.
      </div>
    </div>
  );
}
