import { useEffect, useState } from "react";
import { Check, ExternalLink } from "lucide-react";
import { api, PermissionKind, Permissions } from "../lib/api";
import { APP_NAME } from "../lib/brand";

/** macOS permissions, polled once a second so the chips flip as soon as a switch is turned on. */
export function usePermissions(active = true): Permissions | null {
  const [p, setP] = useState<Permissions | null>(null);
  useEffect(() => {
    if (!active) return;
    let alive = true;
    const poll = () =>
      api
        .permissions()
        .then((v) => alive && setP(v))
        .catch(() => {});
    poll();
    const t = window.setInterval(poll, 1000);
    return () => {
      alive = false;
      window.clearInterval(t);
    };
  }, [active]);
  return p;
}

/** True when something dictation needs is still missing. */
export function permissionsMissing(p: Permissions | null) {
  return !!p && (!p.accessibility || p.microphone === "denied" || p.microphone === "restricted");
}

interface RowSpec {
  kind: PermissionKind;
  title: string;
  desc: string;
  ok: boolean;
  /** Not strictly needed: shown, but never as a problem. */
  optional?: boolean;
  /** The system prompt can still be shown (microphone only). */
  promptable?: boolean;
}

/** Microphone, Accessibility and Input Monitoring rows with live status and a way to fix each. */
export function PermissionRows() {
  const p = usePermissions();
  if (!p) return null;
  const rows: RowSpec[] = [
    {
      kind: "microphone",
      title: "Microphone",
      desc: "To hear you while you hold the shortcut, and never otherwise.",
      ok: p.microphone === "granted",
      promptable: p.microphone === "undetermined",
    },
    {
      kind: "accessibility",
      title: "Accessibility",
      desc: "To notice your shortcut, paste the text where your cursor is and read the field you dictate into.",
      ok: p.accessibility,
    },
    {
      kind: "input_monitoring",
      title: "Input Monitoring",
      desc: "Some macOS versions ask for this as well before a shortcut works in every app.",
      ok: p.input_monitoring,
      optional: true,
    },
  ];
  const fix = async (r: RowSpec) => {
    // Asking first adds the app to the list in System Settings, so it only needs switching on.
    await api.requestPermission(r.kind);
    if (!r.promptable) await api.openPermissionSettings(r.kind);
  };
  return (
    <div className="stack">
      {rows.map((r) => (
        <div key={r.kind} className="hstack" style={{ justifyContent: "space-between", alignItems: "flex-start", gap: 16 }}>
          <div className="stack" style={{ gap: 2 }}>
            <b>{r.title}</b>
            <span className="small muted">{r.desc}</span>
          </div>
          <div className="hstack" style={{ flexShrink: 0 }}>
            {r.ok ? (
              <span className="chip ok"><Check /> Allowed</span>
            ) : (
              <>
                <span className={`chip ${r.optional ? "info" : "warn"}`}>{r.optional ? "Optional" : "Needed"}</span>
                <button type="button" className="btn small" onClick={() => fix(r)}>
                  {r.promptable ? "Allow" : <>Open System Settings <ExternalLink size={11} /></>}
                </button>
              </>
            )}
          </div>
        </div>
      ))}
      <p className="small muted">
        {APP_NAME} never reads your keys except to match your shortcut, and the code is open so you can check. If a switch is on but
        the chip still says “Needed”, turn it off and on again: macOS ties the permission to the exact app build.
      </p>
    </div>
  );
}
