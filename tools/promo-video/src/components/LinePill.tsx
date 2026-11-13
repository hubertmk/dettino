// The app's One line pill, driven by the frame instead of requestAnimationFrame. The motion model is
// the app's own engine.ts; this file replays it from the start of the scene on every frame so any
// frame renders the same, whichever order Remotion renders them in.
import { useLayoutEffect, useRef } from "react";
import { useCurrentFrame, useVideoConfig } from "remotion";
import {
  CFG, N, Phase, Spring, Wavelet, breath, driftGlintX, driftY, emit, fadeColor, lineTarget, prune, rippleY, win,
} from "../../../../apps/desktop/src/pill/engine";

type RGB = [number, number, number];
const INK: RGB = [27, 27, 31];
const ACCENT: RGB = [47, 95, 219];
const SUCCESS: RGB = [23, 120, 75];
const rgba = (c: number[], a: number) => `rgba(${c[0] | 0},${c[1] | 0},${c[2] | 0},${a})`;
const mix = (a: number[], b: number[], t: number) => a.map((v, i) => v + (b[i] - v) * t);

/** Pill window size in CSS px, as in Line.tsx. */
const W = 360;
const H = 80;
const BASE_Y = H - 18;
const DT = 1 / 120;

export interface Cue {
  /** Frame (relative to the sequence) the phase starts at. */
  at: number;
  phase: Phase;
}

/** A plausible speech level: syllable bursts with short gaps, 0..1. */
export function speechLevel(t: number) {
  const syl = Math.max(0, Math.sin(t * 2 * Math.PI * 3.1) * 0.6 + Math.sin(t * 2 * Math.PI * 1.3 + 1) * 0.5);
  const words = 0.55 + 0.45 * Math.sin(t * 2 * Math.PI * 0.45 + 0.4);
  return Math.min(1, syl * words * 0.9 + 0.08);
}

interface Sim {
  L: Spring; sw: Spring; op: Spring; th: Spring;
  col: number[]; wl: Wavelet[]; lastEmit: number; lv: number; quietSince: number; thinkT: number; phase: Phase;
}

function simulate(T: number, fps: number, cues: Cue[]): Sim {
  const e: Sim = {
    L: new Spring(CFG.idleLen / 2), sw: new Spring(CFG.strokeIdle), op: new Spring(0), th: new Spring(0),
    col: INK.slice(), wl: [], lastEmit: -1, lv: 0, quietSince: 0, thinkT: 0, phase: "idle",
  };
  let since = 0;
  const steps = Math.round(T / DT);
  for (let i = 0; i <= steps; i++) {
    const t = i * DT;
    const cue = [...cues].reverse().find((c) => c.at / fps <= t);
    const phase = cue?.phase ?? "idle";
    if (phase !== e.phase) {
      if (phase === "thinking") e.thinkT = t;
      since = t;
      e.phase = phase;
    }
    const listening = phase === "listening";
    e.lv += ((listening ? speechLevel(t) : 0) - e.lv) * (1 - Math.exp(-DT * 16));
    const tg = lineTarget(phase);
    e.L.to(tg.L, tg.kz);
    const open = phase !== "idle";
    e.op.to(open ? 1 : 0, [300, 0.9]);
    const swell = phase === "flash" && t - since < CFG.flashMs / 1000 ? CFG.flashSwell : 0;
    e.sw.to(open ? CFG.strokeOpen + swell : CFG.strokeIdle, swell ? [260, 0.7] : [300, 0.9]);
    e.th.to(phase === "thinking" ? 1 : 0, phase === "thinking" ? [400, 1] : [120, 1]);
    for (const sp of [e.L, e.op, e.sw, e.th]) sp.step(DT);
    if (listening) e.lastEmit = emit(e.wl, t, e.lv, e.lastEmit);
    if (e.wl.length) e.wl = prune(e.wl, t);
    if (e.lv > 0.04) e.quietSince = t;
    const target = phase === "flash" ? SUCCESS : listening ? ACCENT : phase === "thinking" ? mix(ACCENT, INK, 0.25) : INK;
    e.col = fadeColor(e.col, target, DT, 11);
  }
  return e;
}

export function LinePill({ cues, scale = 3 }: { cues: Cue[]; scale?: number }) {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const ref = useRef<HTMLCanvasElement>(null);

  useLayoutEffect(() => {
    const cv = ref.current;
    if (!cv) return;
    const T = frame / fps;
    const e = simulate(T, fps, cues);
    const ctx = cv.getContext("2d")!;
    ctx.setTransform(scale, 0, 0, scale, 0, 0);
    ctx.clearRect(0, 0, W, H);

    const phase = e.phase;
    const cx = W / 2;
    const cy = BASE_Y - CFG.lift * e.op.x;
    const L = Math.max(1, e.L.x);
    const th = e.th.x;
    const tt = T - e.thinkT;
    const br = phase === "listening" ? breath(T - e.quietSince) : 0;
    const pts: [number, number][] = [];
    for (let i = 0; i <= N; i++) {
      const u = i / N;
      const x = -L + 2 * L * u;
      const w = win(u, 4);
      let y = 0;
      if (e.wl.length) y += rippleY(e.wl, x, T, L, phase === "thinking" ? 6 : 0) * w;
      if (br) y += br * w;
      if (th > 0.001) y += driftY(x, u, L, tt, th);
      pts.push([cx + x, cy + y]);
    }
    const path = () => {
      ctx.beginPath();
      pts.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
    };
    const open = phase !== "idle";
    const sw = e.sw.x;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.save();
    const op = e.op.x;
    if (op > 0.01) {
      ctx.shadowColor = `rgba(0,0,0,${(0.22 * op).toFixed(3)})`;
      ctx.shadowBlur = 4 * op * scale;
      ctx.shadowOffsetY = 2 * op * scale;
    }
    path();
    ctx.lineWidth = sw;
    ctx.strokeStyle = rgba(e.col, open ? 1 : 0.75);
    ctx.stroke();
    ctx.restore();
    if (phase === "thinking" && th > 0.02) {
      const gx = cx + driftGlintX(L, tt);
      const gw = Math.max(10, L * 0.3);
      const gr = ctx.createLinearGradient(gx - gw, 0, gx + gw, 0);
      const hi = mix(e.col, [255, 255, 255], 0.7);
      gr.addColorStop(0, rgba(hi, 0));
      gr.addColorStop(0.5, rgba(hi, 0.55 * th));
      gr.addColorStop(1, rgba(hi, 0));
      path();
      ctx.lineWidth = sw;
      ctx.strokeStyle = gr;
      ctx.stroke();
    }
  }, [frame, fps, cues, scale]);

  return <canvas ref={ref} width={W * scale} height={H * scale} style={{ width: W * scale, height: H * scale, display: "block" }} />;
}
