import { useState } from "react";
import { BookA, Check, Plus, Trash2, X } from "lucide-react";
import { api, Suggestion } from "../lib/api";
import { APP_NAME } from "../lib/brand";
import { useStore } from "../lib/store";
import { Empty, Row, Toggle } from "../components/ui";

export function Dictionary({ suggestions, setSuggestions }: { suggestions: Suggestion[]; setSuggestions: (s: Suggestion[]) => void }) {
  const { snap, update, setSnap } = useStore();
  const s = snap.settings;
  const [term, setTerm] = useState("");
  const [from, setFrom] = useState("");
  const [bulk, setBulk] = useState<string | null>(null);
  const add = () => {
    if (!term.trim()) return;
    update((x) => {
      if (!x.dictionary.some((d) => d.term.toLowerCase() === term.trim().toLowerCase())) {
        x.dictionary.unshift({ term: term.trim(), replace_from: from.trim() || null, source: "manual", enabled: true });
      }
    });
    setTerm("");
    setFrom("");
  };
  const resolve = async (sg: Suggestion, accept: boolean) => {
    setSnap(await api.suggestionResolve(sg.to, accept));
    setSuggestions(suggestions.filter((x) => x.to !== sg.to));
  };
  return (
    <div className="page">
      <div className="page-head">
        <h1>Dictionary</h1>
        <p>Names, products and terms {APP_NAME} should always spell your way. They are passed to the transcription and cleanup as hints.</p>
      </div>

      {suggestions.length > 0 && (
        <div className="card">
          <div className="card-head"><h3>Suggested from your corrections</h3><span className="sub">Nothing is added without you</span></div>
          {suggestions.map((sg) => (
            <div className="row" key={sg.to}>
              <div className="grow">
                <b>{sg.to}</b> <span className="muted small">— you changed "{sg.from}"</span>
              </div>
              <button className="btn small" onClick={() => resolve(sg, true)}><Check /> Add</button>
              <button className="btn small ghost" onClick={() => resolve(sg, false)} aria-label="Dismiss"><X /></button>
            </div>
          ))}
        </div>
      )}

      <div className="card">
        <div className="card-pad hstack">
          <input className="input" style={{ flex: 2 }} aria-label="Word or name" placeholder="Word or name, e.g. Lindqvist" value={term} onChange={(e) => setTerm(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
          <input className="input" style={{ flex: 2 }} aria-label="Often heard as" placeholder="Often heard as (optional), e.g. lind quest" value={from} onChange={(e) => setFrom(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
          <button className="btn primary" onClick={add} disabled={!term.trim()}><Plus /> Add</button>
        </div>
        {s.dictionary.length === 0 ? (
          <Empty icon={<BookA />} title={`Teach ${APP_NAME} your words`}>
            Names, products, acronyms. When you fix a word after dictating, {APP_NAME} suggests adding it here — nothing is added without you.
          </Empty>
        ) : (
          <table className="simple">
            <thead>
              <tr><th>Term</th><th>Often heard as</th><th>Added by</th><th style={{ width: 90 }}>On</th><th style={{ width: 40 }} /></tr>
            </thead>
            <tbody>
              {s.dictionary.map((d, i) => (
                <tr key={i}>
                  <td><input className="input" value={d.term} onChange={(e) => update((x) => (x.dictionary[i].term = e.target.value))} /></td>
                  <td><input className="input" aria-label={`${d.term} is often heard as`} value={d.replace_from ?? ""} placeholder="—" onChange={(e) => update((x) => (x.dictionary[i].replace_from = e.target.value || null))} /></td>
                  <td className="small muted">{d.source === "auto" ? "Suggestion" : "You"}</td>
                  <td><Toggle label={`Use ${d.term}`} on={d.enabled} onChange={(v) => update((x) => (x.dictionary[i].enabled = v))} /></td>
                  <td><button className="btn small ghost" aria-label={`Delete ${d.term}`} onClick={() => update((x) => x.dictionary.splice(i, 1))}><Trash2 /></button></td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {s.dictionary.length > 500 && <div className="row small" style={{ color: "var(--warning)" }}>Very large dictionaries can make recognition substitute names. Consider removing terms you no longer use.</div>}
      </div>

      <div className="card">
        <Row title="Learn from my corrections" desc={`8 seconds after a paste, ${APP_NAME} looks at how you edited the text and suggests names and terms.`}>
          <Toggle label="Learn from corrections" on={s.learn_from_corrections} onChange={(v) => update((x) => (x.learn_from_corrections = v))} />
        </Row>
        <Row title="Import or export" desc="One term per line; optionally term, heard-as separated by a comma.">
          <div className="hstack">
            <button className="btn small" onClick={() => setBulk(s.dictionary.map((d) => (d.replace_from ? `${d.term}, ${d.replace_from}` : d.term)).join("\n"))}>Export / edit as text</button>
          </div>
        </Row>
        {bulk !== null && (
          <div className="card-pad stack" style={{ borderTop: "1px solid var(--border)" }}>
            <textarea className="textarea mono" rows={8} value={bulk} onChange={(e) => setBulk(e.target.value)} />
            <div className="hstack" style={{ justifyContent: "flex-end" }}>
              <button className="btn small" onClick={() => setBulk(null)}>Cancel</button>
              <button
                className="btn small primary"
                onClick={() => {
                  const lines = bulk.split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
                  update((x) => {
                    x.dictionary = lines.map((l) => {
                      const [t, f] = l.split(",").map((p) => p.trim());
                      const prev = x.dictionary.find((d) => d.term === t);
                      return { term: t, replace_from: f || null, source: prev?.source ?? "manual", enabled: prev?.enabled ?? true };
                    });
                  });
                  setBulk(null);
                }}
              >
                Replace dictionary
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
