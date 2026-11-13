// "One line" pill (concept G.5, decided 2026-10-18): a single line drawn on a canvas that is
// the idle mark, the meter, the thinking indicator and the success mark. Words sit above it.
import { useCallback, useEffect, useRef, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { APP_NAME } from "../lib/brand";
import { ACCENT_APPLIED } from "../lib/theme";
import {
  CFG, CHECK, N, Phase, Spring, Wavelet, archY, breath, driftGlintX, driftY, emit, fadeColor, lineTarget, prune, rippleY, win,
} from "./engine";
import { act, IDLE, mmss, PillAction, PillState, useNow } from "./state";

/** Window size in CSS px; must match `win_size(PillStyle::Line)` in pill.rs. */
const W = 360;
const H = 80;
/** The line's resting baseline, from the top. Text sits above it, the taskbar is below. */
const BASE_Y = H - 18;

const REDUCED = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : null;
const DARK = typeof matchMedia === "function" ? matchMedia("(prefers-color-scheme: dark)") : null;

type RGB = [number, number, number];
function parseColor(s: string): RGB | null {
  s = s.trim();
  const hex = /^#([0-9a-f]{6})$/i.exec(s);
  if (hex) {
    const n = parseInt(hex[1], 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }
  const m = /^rgba?\(([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)/.exec(s);
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}
const rgba = (c: number[], a: number) => `rgba(${c[0] | 0},${c[1] | 0},${c[2] | 0},${a})`;
const mix = (a: number[], b: number[], t: number) => a.map((v, i) => v + (b[i] - v) * t);

interface Palette {
  ink: RGB;
  accent: RGB;
  success: RGB;
  warning: RGB;
  danger: RGB;
  /** The line colour for an error: red, or ink when the accent is itself red and a red line means "listening". */
  errorLine: RGB;
  dark: boolean;
}
function hue([r, g, b]: number[]) {
  const M = Math.max(r, g, b), m = Math.min(r, g, b), d = M - m;
  if (!d) return 0;
  const h = M === r ? ((g - b) / d) % 6 : M === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return (h * 60 + 360) % 360;
}
function readPalette(): Palette {
  const cs = getComputedStyle(document.documentElement);
  const get = (name: string, fallback: RGB) => parseColor(cs.getPropertyValue(name)) ?? fallback;
  const ink = get("--text", [27, 27, 31]);
  const accent = get("--accent", [47, 95, 219]);
  const danger = get("--danger", [180, 35, 47]);
  const gap = Math.abs(hue(accent) - hue(danger)) % 360;
  const saturated = Math.max(...accent) - Math.min(...accent) > 40;
  return {
    ink,
    accent,
    success: get("--success", [23, 120, 75]),
    warning: get("--warning", [143, 94, 18]),
    danger,
    errorLine: saturated && Math.min(gap, 360 - gap) < 30 ? ink : danger,
    dark: DARK?.matches ?? false,
  };
}

/** Which motion the line is in. Everything with words on it that is not listening or thinking is a "message". */
function phaseOf(st: PillState, hover: boolean): Phase {
  switch (st.kind) {
    case "idle":
      return hover ? "hover" : "idle";
    case "listening":
      return "listening";
    case "thinking":
    case "starting":
      // Getting ready moves like thinking: the line lifts and the hump drifts under the words.
      return "thinking";
    case "pasted":
      return st.success === "check" || st.success === "check_word" ? "check" : "flash";
    default:
      return "message";
  }
}

interface Engine {
  L: Spring;
  sw: Spring;
  op: Spring;
  th: Spring;
  m: Spring;
  col: number[] | null;
  wl: Wavelet[];
  lastEmit: number;
  lv: number;
  lvTarget: number;
  lvSampled: number;
  quietSince: number;
  thinkT: number;
  last: number;
  rect: string;
  hit: string;
  running: boolean;
}

export function LinePill() {
  const [st, setSt] = useState<PillState>(IDLE);
  // `&tray=1` opens the hover state in the mock build, for review screenshots.
  const [hover, setHover] = useState(import.meta.env.DEV && new URLSearchParams(location.search).has("tray"));
  const [reduced, setReduced] = useState(REDUCED?.matches ?? false);
  const stRef = useRef(st);
  const hoverRef = useRef(hover);
  const reducedRef = useRef(reduced);
  stRef.current = st;
  hoverRef.current = hover;
  reducedRef.current = reduced;

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const labRef = useRef<HTMLDivElement>(null);
  const trayRef = useRef<HTMLDivElement>(null);
  const hitRef = useRef<HTMLDivElement>(null);
  const pal = useRef<Palette | null>(null);
  const eng = useRef<Engine>({
    L: new Spring(CFG.idleLen / 2), sw: new Spring(CFG.strokeIdle), op: new Spring(0), th: new Spring(0), m: new Spring(0),
    col: null, wl: [], lastEmit: 0, lv: 0, lvTarget: 0, lvSampled: 0, quietSince: 0, thinkT: 0, last: 0, rect: "", hit: "", running: false,
  });
  const leaveTimer = useRef<number | undefined>(undefined);
  const raf = useRef(0);

  // ---- drawing -------------------------------------------------------------

  const draw = useCallback((t: number) => {
    const cv = canvasRef.current;
    const P = pal.current;
    if (!cv || !P) return;
    const e = eng.current;
    const s = stRef.current;
    const phase = phaseOf(s, hoverRef.current);
    const snap = reducedRef.current;
    const dpr = window.devicePixelRatio || 1;
    if (cv.width !== Math.round(W * dpr)) {
      cv.width = Math.round(W * dpr);
      cv.height = Math.round(H * dpr);
    }
    const ctx = cv.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, W, H);

    const listening = phase === "listening";
    const paused = s.kind === "idle" && s.badge === "paused";
    const cx = W / 2;
    const cy = BASE_Y - CFG.lift * e.op.x;
    const L = Math.max(1, e.L.x);
    const th = e.th.x;
    const m = Math.max(0, Math.min(1, e.m.x));
    const tt = t - e.thinkT;
    if (e.lv > 0.04) e.quietSince = t;
    const br = listening && !snap ? breath(t - e.quietSince) : 0;

    const pts: [number, number][] = new Array(N + 1);
    for (let i = 0; i <= N; i++) {
      const u = i / N;
      const x = -L + 2 * L * u;
      const w = win(u, 4);
      let y = 0;
      if (e.wl.length) y += rippleY(e.wl, x, t, L, phase === "thinking" ? 6 : 0) * w;
      if (br) y += br * w;
      if (th > 0.001) y += snap ? archY(u, th) : driftY(x, u, L, tt, th);
      const bx = cx + x;
      const by = cy + y;
      const k = CHECK[i];
      pts[i] = [bx + (cx + k[0] - bx) * m, by + (cy + k[1] - by) * m];
    }

    const path = () => {
      ctx.beginPath();
      let pen = false;
      for (let i = 0; i <= N; i++) {
        // Paused: the line is drawn in two halves with a 6 px gap.
        if (paused && Math.abs(pts[i][0] - cx) < 3) {
          pen = false;
          continue;
        }
        if (pen) ctx.lineTo(pts[i][0], pts[i][1]);
        else ctx.moveTo(pts[i][0], pts[i][1]);
        pen = true;
      }
    };

    const open = phase !== "idle";
    const alpha = paused ? 0.5 : open ? 1 : 0.75;
    const sw = e.sw.x;
    const col = e.col ?? P.ink;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    // halo of the opposite tone, so the line survives any wallpaper
    ctx.save();
    ctx.shadowColor = P.dark ? "rgba(0,0,0,.6)" : "rgba(255,255,255,.9)";
    ctx.shadowBlur = 2.5;
    path();
    ctx.lineWidth = sw + 0.5;
    ctx.strokeStyle = P.dark ? "rgba(0,0,0,.35)" : "rgba(255,255,255,.55)";
    ctx.stroke();
    ctx.restore();
    // the line, with a drop shadow once it has lifted
    ctx.save();
    const op = e.op.x;
    if (op > 0.01) {
      ctx.shadowColor = `rgba(0,0,0,${(0.22 * op).toFixed(3)})`;
      ctx.shadowBlur = 4 * op;
      ctx.shadowOffsetY = 2 * op;
    }
    path();
    ctx.lineWidth = sw;
    ctx.strokeStyle = rgba(col, alpha);
    ctx.stroke();
    ctx.restore();
    // glint: rides the hump while thinking, crosses the idle line while a model downloads
    const downloading = s.kind === "idle" && s.badge === "downloading";
    if (!snap && ((th > 0.02 && phase === "thinking") || downloading)) {
      let gx: number;
      let ga: number;
      if (downloading) {
        const ph = (t / 1.6) % 1;
        const ease = ph * ph * (3 - 2 * ph);
        gx = cx - L + 2 * L * ease;
        ga = Math.sin(Math.PI * ph) * 0.8;
      } else {
        gx = cx + driftGlintX(L, tt);
        ga = 0.55 * th;
      }
      const gw = Math.max(10, L * 0.3);
      const gr = ctx.createLinearGradient(gx - gw, 0, gx + gw, 0);
      const hi = mix(col, [255, 255, 255], P.dark ? 0.55 : 0.7);
      gr.addColorStop(0, rgba(hi, 0));
      gr.addColorStop(0.5, rgba(hi, ga));
      gr.addColorStop(1, rgba(hi, 0));
      path();
      ctx.lineWidth = sw;
      ctx.strokeStyle = gr;
      ctx.stroke();
    }

    // interactive rectangle: the line plus whatever text sits on it
    let x0 = cx - L - 8, y0 = cy - 10, x1 = cx + L + 8, y1 = cy + 8;
    for (const el of [labRef.current, trayRef.current]) {
      if (!el || !el.offsetWidth) continue;
      const r = el.getBoundingClientRect();
      x0 = Math.min(x0, r.left - 4); y0 = Math.min(y0, r.top - 4);
      x1 = Math.max(x1, r.right + 4); y1 = Math.max(y1, r.bottom + 4);
    }
    const rect = `${x0 | 0},${y0 | 0},${(x1 - x0) | 0},${(y1 - y0) | 0}`;
    if (rect !== e.rect) {
      e.rect = rect;
      invoke("pill_rect", { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }).catch(() => {});
    }
    const hit = hitRef.current;
    const hb = `${(cx - L - 8) | 0},${(cy - 10) | 0},${(2 * L + 16) | 0}`;
    if (hit && hb !== e.hit) {
      e.hit = hb;
      hit.style.left = `${cx - L - 8}px`;
      hit.style.top = `${cy - 10}px`;
      hit.style.width = `${2 * L + 16}px`;
      hit.style.height = "18px";
    }
  }, []);

  const tick = useCallback((nowMs: number) => {
    const e = eng.current;
    const s = stRef.current;
    const snap = reducedRef.current;
    const dt = Math.min(0.05, e.last ? (nowMs - e.last) / 1000 : 0.016);
    e.last = nowMs;
    const t = nowMs / 1000;
    const phase = phaseOf(s, hoverRef.current);
    const listening = phase === "listening";

    // microphone level: smoothed, or sampled four times a second under reduced motion
    if (snap) {
      if (nowMs - e.lvSampled > 250) {
        e.lv = e.lvTarget;
        e.lvSampled = nowMs;
      }
    } else {
      e.lv += (e.lvTarget - e.lv) * (1 - Math.exp(-dt * 16));
    }
    if (!listening) e.lvTarget = 0;

    // targets
    const lab = labRef.current;
    const textHalf = lab && lab.offsetWidth && phase !== "check" && phase !== "flash" ? lab.offsetWidth / 2 : 0;
    const tg = lineTarget(phase, textHalf);
    e.L.to(tg.L, tg.kz);
    const open = phase !== "idle";
    e.op.to(open ? 1 : 0, [300, 0.9]);
    const since = Date.now() - s.since_ms;
    const swell = phase === "flash" && since < CFG.flashMs ? CFG.flashSwell : 0;
    e.sw.to(open ? CFG.strokeOpen + swell : CFG.strokeIdle, swell ? [260, 0.7] : [300, 0.9]);
    e.th.to(phase === "thinking" ? 1 : 0, phase === "thinking" ? [400, 1] : [120, 1]);
    e.m.to(phase === "check" ? 1 : 0, [170, 0.78]);
    for (const sp of [e.L, e.op, e.sw, e.th, e.m]) sp.step(dt, snap);

    // meter
    if (listening) e.lastEmit = emit(e.wl, t, e.lv, e.lastEmit);
    if (e.wl.length) e.wl = prune(e.wl, t);

    // colour
    const P = pal.current;
    if (P) {
      const b = s.badge;
      const target =
        s.kind === "pasted" || s.kind === "ready" ? P.success
        // An error always has red words and a red dot; the line is red too unless the accent
        // is red-ish (Olivetti red, or a red OS accent), where red already means "listening".
        : s.kind === "error" ? P.errorLine
        : s.kind === "listening" && s.quiet ? P.warning
        : s.kind === "listening" ? P.accent
        : s.kind === "thinking" || s.kind === "starting" ? mix(P.accent, P.ink, 0.25)
        : s.kind === "idle" && (b === "no_mic" || b === "hotkeys_off" || b === "cleanup_off") ? P.warning
        : s.kind === "idle" && b === "error" ? mix(P.danger, P.ink, 0.4)
        : s.kind === "idle" && b === "downloading" ? P.accent
        : P.ink;
      e.col = fadeColor(e.col, target, dt, 11, snap);
    }

    draw(t);

    const busy =
      phase !== "idle" || e.wl.length > 0 || s.badge === "downloading" ||
      ![e.L, e.op, e.sw, e.th, e.m].every((sp) => sp.settled) ||
      (e.col && P && e.col.some((v, i) => Math.abs(v - (P.ink as number[])[i]) > 0.5));
    if (busy) raf.current = requestAnimationFrame(tick);
    else {
      e.running = false;
      e.last = 0;
    }
  }, [draw]);

  const wake = useCallback(() => {
    const e = eng.current;
    if (e.running) return;
    e.running = true;
    raf.current = requestAnimationFrame(tick);
  }, [tick]);

  // ---- feeds ---------------------------------------------------------------

  useEffect(() => {
    pal.current = readPalette();
    const onTheme = () => {
      pal.current = readPalette();
      eng.current.col = null;
      wake();
    };
    DARK?.addEventListener("change", onTheme);
    const onReduced = () => setReduced(REDUCED?.matches ?? false);
    REDUCED?.addEventListener("change", onReduced);
    // The accent is applied asynchronously at start-up and again when the setting changes.
    window.addEventListener(ACCENT_APPLIED, onTheme);
    return () => {
      DARK?.removeEventListener("change", onTheme);
      REDUCED?.removeEventListener("change", onReduced);
      window.removeEventListener(ACCENT_APPLIED, onTheme);
    };
  }, [wake]);

  useEffect(() => {
    invoke<PillState>("pill_ready").then(setSt).catch(() => {});
    const un1 = listen<PillState>("pill", (e) => setSt(e.payload));
    const un2 = listen<boolean>("pill:hover", (e) => {
      if (e.payload) window.clearTimeout(leaveTimer.current);
    });
    const ch = new Channel<{ level: number; speech: boolean }>();
    ch.onmessage = (f) => {
      eng.current.lvTarget = f.level;
      wake();
    };
    invoke("subscribe_levels", { channel: ch }).catch(() => {});
    return () => {
      un1.then((u) => u());
      un2.then((u) => u());
      cancelAnimationFrame(raf.current);
    };
  }, [wake]);

  // any state or hover change starts the loop; it stops itself once everything has settled
  const prevKind = useRef("idle");
  useEffect(() => {
    // The hump keeps gliding when only the label changes mid-thinking ("Working…", "Still brewing…").
    const drifting = (k: string) => k === "thinking" || k === "starting";
    if (drifting(st.kind) && !drifting(prevKind.current)) eng.current.thinkT = performance.now() / 1000;
    prevKind.current = st.kind;
    if (st.kind !== "listening") eng.current.lvTarget = 0;
    wake();
  }, [st, hover, reduced, wake]);

  // ---- interaction -----------------------------------------------------------

  const active = st.kind !== "idle";
  const now = useNow(active && (st.kind === "thinking" || st.kind === "starting" || st.hands_free), st.hands_free ? 500 : 300);
  const undoNow = useNow(hover && !active && !!st.undo_until_ms && st.undo_until_ms > Date.now(), 1000);
  const undoLeft = st.undo_until_ms ? Math.max(0, st.undo_until_ms - undoNow) : 0;

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
    if (!down.current) return;
    down.current = null;
    if (active) return;
    if (st.badge === "paused") act("pause");
    else setHover((h) => !h);
  };
  const enter = useCallback(() => {
    window.clearTimeout(leaveTimer.current);
    if (stRef.current.kind !== "idle") return;
    leaveTimer.current = window.setTimeout(() => setHover(true), 250);
  }, []);
  const leave = useCallback(() => {
    window.clearTimeout(leaveTimer.current);
    leaveTimer.current = window.setTimeout(() => setHover(false), 300);
  }, []);
  useEffect(() => {
    if (active) setHover(false);
  }, [active]);

  const showTray = hover && !active;
  const label = labelFor(st, hover, now);
  const title = st.badge_label ?? `${APP_NAME} — hover for options`;

  return (
    <div className="stage line" onMouseLeave={leave}>
      <canvas ref={canvasRef} style={{ width: W, height: H }} aria-hidden="true" />
      {showTray && (
        <div className="tray" ref={trayRef} onMouseEnter={enter}>
          <button onClick={() => act("pause")}>{st.badge === "paused" ? "Resume" : "Pause"}</button>
          <i />
          <button disabled={undoLeft <= 0} onClick={() => act("undo")} aria-label="Undo AI edit">
            Undo{undoLeft > 0 && <span className="tn"> · {Math.ceil(undoLeft / 1000)}s</span>}
          </button>
          <i />
          <button onClick={() => act("paste_last")}>Paste last</button>
          <i />
          <button onClick={() => act("settings")}>Settings</button>
        </div>
      )}
      {label && (
        <div key={label.key} className={`lab ${label.tone ?? ""}`} ref={labRef} role="status" aria-live="polite" onMouseEnter={enter}>
          {label.node}
        </div>
      )}
      <div
        className="hit"
        ref={hitRef}
        role="img"
        aria-label={st.badge_label ?? (active ? st.label ?? `${APP_NAME} is ${st.kind}` : `${APP_NAME} is ready`)}
        title={active ? undefined : title}
        onMouseEnter={enter}
        onMouseDown={onMouseDown}
        onMouseMove={onMouseMove}
        onMouseUp={onMouseUp}
      />
    </div>
  );
}

function Keys({ chord }: { chord: string }) {
  return (
    <>
      {chord.split(" + ").map((k, i) => (
        <kbd key={i}>{k}</kbd>
      ))}
    </>
  );
}

function Action({ a }: { a: PillAction }) {
  return (
    <button className="action" onClick={() => act(a.id)}>
      {a.label}
    </button>
  );
}

interface Label {
  key: string;
  node: React.ReactNode;
  tone?: "warning" | "danger";
}

/** The words that sit on the line in a state, or null for a bare line. */
function labelFor(st: PillState, hover: boolean, now: number): Label | null {
  const elapsed = now - st.since_ms;
  switch (st.kind) {
    case "idle": {
      if (!hover) return null;
      if (st.badge === "paused") return { key: "paused", node: <><span>Paused</span><button className="action" onClick={() => act("pause")}>Resume</button></> };
      if (st.badge && st.badge_label) return { key: st.badge, node: <span>{st.badge_label}</span>, tone: st.badge === "error" ? "danger" : st.badge === "downloading" ? undefined : "warning" };
      return { key: "hint", node: <span className="hint">Hold <Keys chord={st.hint} /></span> };
    }
    case "listening": {
      if (st.quiet && st.label) return { key: "quiet", node: <><i className="wdot" /><span>{st.label}</span></>, tone: "warning" };
      if (st.hands_free) {
        const warning = st.label?.startsWith("1 minute");
        return {
          key: "hf",
          node: (
            <>
              <b className="tn">{mmss(elapsed)}</b>
              {(warning || elapsed < 3500) && st.label && <><span className="sep">·</span><span className="sub">{warning ? st.label : "Space to finish"}</span></>}
              {st.action && <Action a={st.action} />}
            </>
          ),
        };
      }
      if (st.mode === "command" && st.label) return { key: "cmd", node: <span className="sub">{st.label}</span> };
      return null;
    }
    case "thinking": {
      const text = st.label ?? (elapsed > 2500 ? "Still working…" : elapsed > 1200 ? "Working…" : null);
      return text ? { key: "think", node: <span className="sub">{text}</span> } : null;
    }
    case "starting":
      return st.label ? { key: `start:${st.label}`, node: <span className="sub">{st.label}</span> } : null;
    case "ready":
      return { key: "ready", node: <><span>{st.label ?? "Pronto!"}</span><span className="hint">Hold <Keys chord={st.hint} /></span></> };
    case "pasted":
      if (st.success !== "check_word") return null;
      return { key: "pasted", node: <><span>{st.label ?? "Pasted"}</span>{st.detail && <span className="sub">{st.detail}</span>}</> };
    case "error":
      return { key: `err:${st.label}`, node: <><i className="wdot" /><span title={st.detail ?? undefined}>{st.label}</span>{st.action && <Action a={st.action} />}</>, tone: "danger" };
    default:
      // pasted_raw, held, nothing, cancelled, blocked, notice
      return { key: `${st.kind}:${st.label}`, node: <><span title={st.detail ?? undefined}>{st.label}</span>{st.action && <Action a={st.action} />}</> };
  }
}
