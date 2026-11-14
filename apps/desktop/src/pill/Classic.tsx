// Classic pill: a grey idle dot that grows into a rounded card with glyph, waveform and
// label, with an icon tray above it on hover. The original look, kept as a style.
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as I from "./icons";
import { APP_NAME } from "../lib/brand";
import { act, IDLE, mmss, PillAction, PillState, useNow } from "./state";

const WEIGHTS = [0.45, 0.7, 0.9, 1, 0.9, 0.7, 0.45];

const REDUCED = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : null;

export function ClassicPill() {
  const [st, setSt] = useState<PillState>(IDLE);
  // `&tray=1` opens the hover tray in the mock build, for review screenshots.
  const [hover, setHover] = useState(import.meta.env.DEV && new URLSearchParams(location.search).has("tray"));
  const stageRef = useRef<HTMLDivElement>(null);
  const pillRef = useRef<HTMLDivElement>(null);
  const trayRef = useRef<HTMLDivElement>(null);
  const innerRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState<number | undefined>();
  const level = useRef({ target: 0, value: 0 });
  const leaveTimer = useRef<number | undefined>(undefined);

  // state feed
  useEffect(() => {
    invoke<PillState>("pill_ready").then(setSt).catch(() => {});
    const un1 = listen<PillState>("pill", (e) => setSt(e.payload));
    const un2 = listen<boolean>("pill:hover", (e) => {
      if (e.payload) {
        window.clearTimeout(leaveTimer.current);
      }
    });
    const ch = new Channel<{ level: number; speech: boolean }>();
    ch.onmessage = (f) => {
      level.current.target = f.level;
    };
    invoke("subscribe_levels", { channel: ch }).catch(() => {});
    return () => {
      un1.then((u) => u());
      un2.then((u) => u());
    };
  }, []);

  // waveform: EMA in rAF, written to a CSS variable (no React re-render per frame).
  // Under "reduce motion" the bars step a few times a second with no breathing.
  const [reduced, setReduced] = useState(REDUCED?.matches ?? false);
  useEffect(() => {
    const on = () => setReduced(REDUCED?.matches ?? false);
    REDUCED?.addEventListener("change", on);
    return () => REDUCED?.removeEventListener("change", on);
  }, []);
  useEffect(() => {
    const write = (breath: number) => {
      const l = level.current;
      stageRef.current?.style.setProperty("--level", String(Math.max(0, l.value)));
      stageRef.current?.style.setProperty("--b", String(l.value < 0.05 ? 0 : breath));
    };
    if (reduced) {
      const t = window.setInterval(() => {
        level.current.value = level.current.target;
        write(0);
      }, 250);
      return () => window.clearInterval(t);
    }
    let raf = 0;
    const tick = (t: number) => {
      const l = level.current;
      l.value = l.value + (l.target - l.value) * 0.35;
      write(Math.sin(t / 127) * 0.08);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [reduced]);

  const active = st.kind !== "idle";
  const showTray = hover && st.kind !== "listening";

  // measure content so the width animates
  useLayoutEffect(() => {
    if (!innerRef.current) {
      setWidth(undefined);
      return;
    }
    setWidth(Math.min(320, Math.max(112, innerRef.current.scrollWidth + 2)));
  });

  // report the interactive rectangle (pill + tray, or just the dot) for click-through
  useLayoutEffect(() => {
    const els = [pillRef.current, showTray ? trayRef.current : null].filter(Boolean) as HTMLElement[];
    if (!els.length) return;
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const el of els) {
      const r = el.getBoundingClientRect();
      x0 = Math.min(x0, r.left); y0 = Math.min(y0, r.top);
      x1 = Math.max(x1, r.right); y1 = Math.max(y1, r.bottom);
    }
    invoke("pill_rect", { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }).catch(() => {});
  });

  const now = useNow(active && (st.kind === "thinking" || st.hands_free), st.hands_free ? 500 : 300);
  const undoNow = useNow(showTray && !!st.undo_until_ms && st.undo_until_ms > Date.now(), 1000);
  const undoLeft = st.undo_until_ms ? Math.max(0, st.undo_until_ms - undoNow) : 0;

  // drag after 6 px, click otherwise
  const down = useRef<{ x: number; y: number } | null>(null);
  const onMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;
    down.current = { x: e.screenX, y: e.screenY };
  };
  const onMouseMove = (e: React.MouseEvent) => {
    const d = down.current;
    if (d && (Math.abs(e.screenX - d.x) > 6 || Math.abs(e.screenY - d.y) > 6)) {
      down.current = null;
      getCurrentWindow().startDragging().catch(() => {});
    }
  };
  const onMouseUp = () => {
    if (down.current) {
      down.current = null;
      if (st.badge === "paused") act("pause");
      else setHover((h) => !h);
    }
  };

  const enter = useCallback(() => {
    window.clearTimeout(leaveTimer.current);
    leaveTimer.current = window.setTimeout(() => setHover(true), 250);
  }, []);
  const leave = useCallback(() => {
    window.clearTimeout(leaveTimer.current);
    leaveTimer.current = window.setTimeout(() => setHover(false), 400);
  }, []);

  return (
    <div className="stage classic" ref={stageRef} onMouseLeave={leave}>
      {showTray && (
        <div className="tray" ref={trayRef} onMouseEnter={enter}>
          <TrayButton tip={st.badge === "paused" ? "Resume dictation" : "Pause dictation"} onClick={() => act("pause")}>
            {st.badge === "paused" ? <I.Play /> : <I.Pause />}
          </TrayButton>
          <TrayButton tip="Undo AI edit" disabled={undoLeft <= 0} onClick={() => act("undo")}>
            <I.Undo />
            {undoLeft > 0 && <UndoRing frac={undoLeft / 60000} />}
          </TrayButton>
          <TrayButton tip="Paste last transcript" onClick={() => act("paste_last")}>
            <I.Clip />
          </TrayButton>
          <TrayButton tip="Settings" onClick={() => act("settings")}>
            <I.Gear />
          </TrayButton>
        </div>
      )}
      {!active ? (
        <div className="dot-wrap" ref={pillRef} onMouseEnter={enter} onMouseDown={onMouseDown} onMouseMove={onMouseMove} onMouseUp={onMouseUp}
          title={st.badge_label ?? `${APP_NAME} — hover for options`} role="img" aria-label={st.badge_label ?? `${APP_NAME} is ready`}>
          {st.badge === "paused" ? (
            // A text chip, not a ⏸ glyph: a pause symbol reads as "something is playing".
            <div className={`paused-chip ${hover ? "hot" : ""}`}>
              <I.MicOff />
              <span>Paused</span>
            </div>
          ) : (
            <div className={`dot ${hover ? "hot" : ""} ${st.badge ? "badge-" + st.badge : ""}`}>
              {st.badge === "no_mic" && <I.MicOff />}
              {st.badge === "hotkeys_off" && <I.Keyboard />}
            </div>
          )}
        </div>
      ) : (
        <div
          ref={pillRef}
          className={`pill ${st.hands_free ? "handsfree" : ""} ${st.quiet ? "quiet" : ""}`}
          style={{ width }}
          onMouseEnter={enter}
          onMouseDown={onMouseDown}
          onMouseMove={onMouseMove}
          onMouseUp={() => (down.current = null)}
        >
          <div className="pill-inner" ref={innerRef} role="status" aria-live="polite">
            <Content st={st} now={now} />
          </div>
        </div>
      )}
    </div>
  );
}

function TrayButton(p: { tip: string; disabled?: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button aria-label={p.tip} disabled={p.disabled} onClick={p.onClick}>
      {p.children}
      <span className="tip">{p.tip}</span>
    </button>
  );
}

function UndoRing({ frac }: { frac: number }) {
  const c = 2 * Math.PI * 11;
  return (
    <svg className="ring" viewBox="0 0 24 24">
      <circle cx="12" cy="12" r="11" fill="none" stroke="var(--accent)" strokeWidth="1.5" strokeDasharray={`${c * frac} ${c}`} transform="rotate(-90 12 12)" />
    </svg>
  );
}

function Wave() {
  return (
    <span className="wave" aria-hidden>
      {WEIGHTS.map((w, i) => (
        <i key={i} style={{ ["--w" as string]: w } as React.CSSProperties} />
      ))}
    </span>
  );
}

function Action({ a, danger }: { a: PillAction; danger?: boolean }) {
  return (
    <button className={`action ${danger ? "danger" : ""}`} onClick={() => act(a.id)}>
      {a.label}
    </button>
  );
}

function Content({ st, now }: { st: PillState; now: number }) {
  const elapsed = now - st.since_ms;
  switch (st.kind) {
    case "listening": {
      const glyph = st.mode === "command" ? <I.Wand /> : <I.Mic />;
      return (
        <>
          <span className={`glyph ${st.quiet ? "warning" : "voice"}`}>{glyph}</span>
          <Wave />
          {st.hands_free && <span className="label">{mmss(elapsed)}</span>}
          {(st.quiet || (st.hands_free && elapsed < 3500) || st.mode === "command" || (st.label && st.label.startsWith("1 minute"))) && st.label && (
            <span className="label">{st.label}</span>
          )}
          {st.hands_free && st.action && <Action a={st.action} />}
        </>
      );
    }
    case "starting":
    case "thinking": {
      const label = st.label ?? (elapsed > 2500 ? "Still working…" : elapsed > 1200 ? "Working…" : null);
      return (
        <>
          <span className="dots" aria-hidden>
            <i />
            <i />
            <i />
          </span>
          {label && <span className="label">{label}</span>}
        </>
      );
    }
    case "pasted":
      return (
        <>
          <span className="glyph success">
            <I.Check />
          </span>
          <span className="label">{st.label ?? "Pasted"}</span>
          {st.detail && <span className="detail">{st.detail}</span>}
        </>
      );
    case "ready":
      return (
        <>
          <span className="glyph success">
            <I.Check />
          </span>
          <span className="label">{st.label ?? "Pronto!"}</span>
          <span className="detail">Hold {st.hint}</span>
        </>
      );
    case "pasted_raw":
      return (
        <>
          <span className="glyph muted">
            <I.Check />
          </span>
          <span className="label">{st.label}</span>
          {st.action && <Action a={st.action} />}
        </>
      );
    case "held":
      return (
        <>
          <span className="glyph warning">
            <I.Inbox />
          </span>
          <span className="label" title={st.detail ?? undefined}>{st.label}</span>
          {st.action && <Action a={st.action} />}
        </>
      );
    case "nothing":
      return (
        <>
          <span className="glyph muted">
            <I.MicOff />
          </span>
          <span className="label">{st.label}</span>
        </>
      );
    case "cancelled":
      return (
        <>
          <span className="glyph muted">
            <I.Undo />
          </span>
          <span className="label">{st.label}</span>
          {st.action && <Action a={st.action} />}
        </>
      );
    case "error":
      return (
        <>
          <span className="glyph danger">
            <I.Alert />
          </span>
          <span className="label" title={st.detail ?? undefined}>{st.label}</span>
          {st.action && <Action a={st.action} danger />}
        </>
      );
    case "blocked":
      return (
        <>
          <span className="glyph muted">
            <I.Shield />
          </span>
          <span className="label">{st.label}</span>
          {st.action && <Action a={st.action} />}
        </>
      );
    default:
      return (
        <>
          <span className="glyph muted">
            <I.Info />
          </span>
          <span className="label">{st.label}</span>
          {st.action && <Action a={st.action} />}
        </>
      );
  }
}
