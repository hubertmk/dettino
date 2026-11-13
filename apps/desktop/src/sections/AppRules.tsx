import { useState } from "react";
import { AppWindow, Plus, Trash2 } from "lucide-react";
import { api, AppRule, isMac } from "../lib/api";
import { APP_NAME } from "../lib/brand";
import { useStore } from "../lib/store";
import { Empty, Toggle } from "../components/ui";

const blank = (app: string): AppRule => ({ app, cleanup: null, context: null, never_dictate: false, insert_mode: null, paste_delay_ms: null, paste_chord: null });

export function AppRules() {
  const { snap, update } = useStore();
  const rules = snap.settings.app_rules;
  const [app, setApp] = useState("");
  const [notFound, setNotFound] = useState<string | null>(null);
  const add = async () => {
    let a = app.trim().toLowerCase();
    if (!a) return;
    if (isMac() && !a.includes(".")) {
      // Rules match bundle identifiers on macOS; people type app names.
      const id = await api.resolveApp(app.trim());
      if (!id) {
        setNotFound(app.trim());
        return;
      }
      a = id;
    } else if (!isMac() && !a.includes(".")) {
      a = `${a}.exe`;
    }
    setNotFound(null);
    update((x) => {
      if (!x.app_rules.some((r) => r.app === a)) x.app_rules.unshift(blank(a));
    });
    setApp("");
  };
  const set = (i: number, fn: (r: AppRule) => void) => update((x) => fn(x.app_rules[i]));
  // Rules for the other OS stay in settings but aren't shown here.
  const hidden = (r: AppRule) => (snap.platform === "windows" && r.app.startsWith("com.")) || (snap.platform === "macos" && r.app.endsWith(".exe"));
  return (
    <div className="page">
      <div className="page-head">
        <h1>App rules</h1>
        <p>Different behaviour per app. Terminals get no cleanup (it could change your commands); password managers are never dictated into.</p>
      </div>
      <div className="card">
        <div className="card-pad hstack">
          <input
            className="input"
            style={{ flex: 1 }}
            aria-label={isMac() ? "App name" : "App executable"}
            placeholder={isMac() ? "App name, e.g. Slack" : "App executable, e.g. slack.exe"}
            value={app}
            onChange={(e) => setApp(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && add()}
          />
          <button className="btn primary" onClick={add} disabled={!app.trim()}><Plus /> Add app</button>
        </div>
        {notFound && <p className="card-pad small muted" role="alert">No app named “{notFound}” in the Applications folders. Type its bundle identifier instead, e.g. com.tinyspeck.slackmacgap.</p>}
        {rules.every(hidden) ? (
          <Empty icon={<AppWindow />} title="No app rules yet">
            Every app uses your default cleanup and context settings. Add an app to change how {APP_NAME} behaves in it.
          </Empty>
        ) : (
        <table className="simple">
          <thead>
            <tr>
              <th>App</th>
              <th>Cleanup</th>
              <th>Screen context</th>
              <th>Insert by</th>
              <th>Never dictate</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {rules.map((r, i) => hidden(r) ? null : (
              <tr key={r.app}>
                <td className="mono">{r.app}</td>
                <td>
                  <select className="select" aria-label={`Cleanup in ${r.app}`} value={r.cleanup ?? "default"} onChange={(e) => set(i, (x) => (x.cleanup = e.target.value === "default" ? null : (e.target.value as AppRule["cleanup"])))}>
                    <option value="default">Default</option>
                    <option value="off">Off</option>
                    <option value="light">Light</option>
                    <option value="medium">Medium</option>
                  </select>
                </td>
                <td>
                  <select className="select" aria-label={`Screen context in ${r.app}`} value={r.context === null ? "default" : r.context ? "on" : "off"} onChange={(e) => set(i, (x) => (x.context = e.target.value === "default" ? null : e.target.value === "on"))}>
                    <option value="default">Default</option>
                    <option value="on">On</option>
                    <option value="off">Off</option>
                  </select>
                </td>
                <td>
                  <select
                    className="select"
                    aria-label={`Insert text in ${r.app} by`}
                    value={r.insert_mode === "type" ? "type" : r.paste_chord ?? "ctrl+v"}
                    onChange={(e) =>
                      set(i, (x) => {
                        if (e.target.value === "type") {
                          x.insert_mode = "type";
                          x.paste_chord = null;
                        } else {
                          x.insert_mode = null;
                          x.paste_chord = e.target.value === "ctrl+v" ? null : e.target.value;
                        }
                      })
                    }
                  >
                    <option value="ctrl+v">{isMac() ? "Paste (⌘V)" : "Paste (Ctrl+V)"}</option>
                    <option value="ctrl+shift+v">{isMac() ? "Paste and Match Style (⌥⇧⌘V)" : "Paste (Ctrl+Shift+V)"}</option>
                    {!isMac() && <option value="shift+insert">Paste (Shift+Insert)</option>}
                    <option value="type">Type keystrokes</option>
                  </select>
                </td>
                <td><Toggle label={`Never dictate in ${r.app}`} on={r.never_dictate} onChange={(v) => set(i, (x) => (x.never_dictate = v))} /></td>
                <td><button className="btn small ghost" aria-label={`Remove rule for ${r.app}`} onClick={() => update((x) => x.app_rules.splice(i, 1))}><Trash2 /></button></td>
              </tr>
            ))}
          </tbody>
        </table>
        )}
      </div>
      <p className="small muted">
        "Type keystrokes" works through remote desktops and in consoles that refuse paste. In apps marked "Never dictate", a hold of the shortcut shows the pill with an "Allow 5 min" button instead.
      </p>
    </div>
  );
}
