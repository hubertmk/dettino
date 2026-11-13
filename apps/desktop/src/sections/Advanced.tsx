import { useState } from "react";
import { Download, FolderOpen, Upload } from "lucide-react";
import { api, isMac } from "../lib/api";
import { APP_NAME, osName } from "../lib/brand";
import { useStore } from "../lib/store";
import { Row, Toggle } from "../components/ui";
import { PermissionRows } from "../components/Permissions";
import { autostartProblem } from "../lib/readiness";

export function Advanced() {
  const { snap, update, setSnap, toast } = useStore();
  const s = snap.settings;
  const [importText, setImportText] = useState<string | null>(null);
  return (
    <div className="page">
      <div className="page-head">
        <h1>Advanced</h1>
        <p>{isMac() ? "Permissions, clipboard" : "Clipboard"} behaviour, startup, logs and settings files.</p>
      </div>
      {isMac() && (
        <div className="card card-pad">
          <PermissionRows />
        </div>
      )}
      <div className="card">
        <Row title="Keep dictation on the clipboard" desc="Off: your previous clipboard is restored after pasting (only if you didn't copy something meanwhile).">
          <Toggle label="Keep dictation on clipboard" on={s.keep_on_clipboard} onChange={(v) => update((x) => (x.keep_on_clipboard = v))} />
        </Row>
        <Row
          title={`Start with ${osName(snap.platform)}`}
          desc={autostartProblem(snap.autostart, snap.platform) ?? `${APP_NAME} lives in the ${isMac() ? "menu bar" : "tray"}; closing this window keeps dictation running.`}
        >
          <Toggle label={`Start with ${osName(snap.platform)}`} on={s.launch_at_login} onChange={(v) => update((x) => (x.launch_at_login = v))} />
        </Row>
      </div>
      <div className="card">
        <Row title="Logs" desc="Text-free by default: they contain timings and errors, never what you said.">
          <button className="btn small" onClick={() => api.openDataFolder("logs")}><FolderOpen /> Open logs</button>
        </Row>
        <Row title="Models folder" desc={snap.data_dir + (isMac() ? "/models" : "\\models")}>
          <button className="btn small" onClick={() => api.openDataFolder("models")}><FolderOpen /> Open</button>
        </Row>
        <Row title="Export settings" desc="A JSON file with every setting. API keys are never included.">
          <button
            className="btn small"
            onClick={async () => {
              const json = await api.exportSettings();
              await api.copyText(json);
              toast("Settings copied to the clipboard as JSON");
            }}
          >
            <Download /> Copy as JSON
          </button>
        </Row>
        <Row title="Import settings" desc="Paste an exported JSON.">
          <button className="btn small" onClick={() => setImportText("")}><Upload /> Import…</button>
        </Row>
        {importText !== null && (
          <div className="card-pad stack" style={{ borderTop: "1px solid var(--border)" }}>
            <textarea className="textarea mono" rows={8} placeholder="Paste settings JSON" value={importText} onChange={(e) => setImportText(e.target.value)} />
            <div className="hstack" style={{ justifyContent: "flex-end" }}>
              <button className="btn small" onClick={() => setImportText(null)}>Cancel</button>
              <button
                className="btn small primary"
                disabled={!importText.trim()}
                onClick={async () => {
                  try {
                    setSnap(await api.importSettings(importText));
                    setImportText(null);
                    toast("Settings imported");
                  } catch (e) {
                    toast(String(e));
                  }
                }}
              >
                Import
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
