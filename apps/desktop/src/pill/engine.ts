// Motion model of the pill: one line that is the idle mark, the meter, the
// thinking indicator and the success mark. Pure numbers, no DOM, so every value
// here is unit-tested. Decided on 2026-10-18 after three rounds of prototypes:
// Drift while thinking, 56 px while talking, 2.5 px stroke, 24 px idle, accent
// while listening.

export const CFG = {
  /** Full lengths in CSS px. */
  idleLen: 24,
  talkLen: 56,
  thinkLen: 44,
  flashLen: 40,
  strokeIdle: 2,
  strokeOpen: 2.5,
  /** The line rises this much when it wakes. */
  lift: 2,
  /** Text that sits on the line extends it by this much on each side. */
  textPadIdle: 4,
  textPadOpen: 6,
  /** Ripple emitter: one wavelet per `emitEvery` s of speech, travelling outward at `speed` px/s. */
  emitEvery: 0.07,
  speed: 58,
  /** Flash success: the stroke swells by `flashSwell` px for `flashMs`. */
  flashSwell: 1.2,
  flashMs: 160,
} as const;

/** Stiffness / damping ratio pairs. Entry overshoots once (≈3 %), exit never does. */
export const SPRING = {
  in: [300, 0.72],
  out: [380, 1],
  hover: [420, 0.85],
  /** Return to idle: slightly underdamped so the line comes in 1 px short and relaxes out. */
  settle: [300, 0.82],
  think: [220, 1],
  message: [300, 0.8],
} as const satisfies Record<string, readonly [number, number]>;

export class Spring {
  x: number;
  v = 0;
  t: number;
  k = 300;
  z = 1;
  constructor(x: number) {
    this.x = x;
    this.t = x;
  }
  to(t: number, kz?: readonly [number, number]) {
    this.t = t;
    if (kz) [this.k, this.z] = kz;
    return this;
  }
  /** Advance by `dt` seconds. `snap` jumps straight to the target (reduced motion). */
  step(dt: number, snap = false) {
    if (snap) {
      this.x = this.t;
      this.v = 0;
      return this.x;
    }
    const c = 2 * this.z * Math.sqrt(this.k);
    const n = 4;
    const h = dt / n;
    for (let i = 0; i < n; i++) {
      this.v += (this.k * (this.t - this.x) - c * this.v) * h;
      this.x += this.v * h;
    }
    if (Math.abs(this.x - this.t) < 0.02 && Math.abs(this.v) < 0.4) {
      this.x = this.t;
      this.v = 0;
    }
    return this.x;
  }
  get settled() {
    return this.x === this.t && this.v === 0;
  }
}

export type Phase = "idle" | "hover" | "listening" | "thinking" | "flash" | "check" | "message";

/** Half-length the line springs to in a phase. `textHalf` is half the width of text sitting on it, 0 for none. */
export function lineTarget(phase: Phase, textHalf = 0): { L: number; kz: readonly [number, number] } {
  const pad = phase === "idle" || phase === "hover" ? CFG.textPadIdle : CFG.textPadOpen;
  const withText = (len: number) => Math.max(len / 2, textHalf > 0 ? textHalf + pad : 0);
  switch (phase) {
    case "idle":
      return { L: CFG.idleLen / 2, kz: SPRING.settle };
    case "hover":
      return { L: withText(CFG.idleLen), kz: SPRING.hover };
    case "listening":
      return { L: withText(CFG.talkLen), kz: SPRING.in };
    case "thinking":
      return { L: withText(CFG.thinkLen), kz: SPRING.think };
    case "flash":
      return { L: withText(CFG.flashLen), kz: SPRING.message };
    case "check":
      return { L: Math.max(CHECK_LEN / 2, textHalf > 0 ? textHalf + pad : 0), kz: SPRING.message };
    case "message":
      return { L: withText(CFG.talkLen), kz: SPRING.message };
  }
}

// ---- meter -------------------------------------------------------------

export interface Wavelet {
  born: number;
  amp: number;
}

/** Emit a wavelet at the centre every `emitEvery` s while there is speech. Returns the time of the last emission. */
export function emit(wl: Wavelet[], now: number, level: number, lastEmit: number): number {
  if (level <= 0.06 || now - lastEmit < CFG.emitEvery) return lastEmit;
  wl.push({ born: now, amp: level });
  if (wl.length > 32) wl.shift();
  return now;
}

/** Drop wavelets that have faded. */
export function prune(wl: Wavelet[], now: number) {
  return wl.filter((w) => now - w.born < 1.6);
}

/** Vertical offset of the ripple field at `x` (px from centre) for a line of half-length `L`. */
export function rippleY(wl: readonly Wavelet[], x: number, now: number, L: number, decayBoost = 0): number {
  const ax = Math.abs(x);
  const k = Math.min(1, L / 60);
  const tau = 0.3 + 0.25 * k;
  const ampPx = 8 * (0.6 + 0.4 * k);
  let y = 0;
  for (const w of wl) {
    const age = now - w.born;
    if (age < 0) continue;
    const d = CFG.speed * age;
    const g = Math.exp(-(((ax - d) / 8) ** 2));
    if (g < 0.01 && ax > d) continue;
    y += w.amp * ampPx * Math.exp(-age / tau) * Math.exp(-decayBoost * age) * g * Math.cos((ax - d) * 0.5);
  }
  return y;
}

/** Cosine window that pins the ends of the line flat. `u` runs 0..1 along the line. */
export function win(u: number, power = 4) {
  return Math.cos((u - 0.5) * Math.PI) ** power;
}

/** Thinking: a 4.2 px hump gliding end to end on a line raised 0.7 px. `tt` = seconds in thinking, `th` = ramp 0..1. */
export function driftY(x: number, u: number, L: number, tt: number, th: number): number {
  const dx = L * 0.6 * Math.sin(2 * Math.PI * 0.33 * tt);
  return -(4.2 * Math.exp(-(((x - dx) / (L * 0.3)) ** 2)) + 0.7) * win(u, 0.5) * th;
}

/** Where the glint sits on the hump. */
export function driftGlintX(L: number, tt: number) {
  return L * 0.6 * Math.sin(2 * Math.PI * 0.33 * tt);
}

/** Static arch used under reduced motion so thinking is still distinguishable from idle. */
export function archY(u: number, th: number) {
  return -2 * th * Math.cos((u - 0.5) * Math.PI) ** 1.2;
}

/** Slow ±0.8 px breath after `quietFor` seconds of silence while listening. */
export function breath(quietFor: number) {
  return quietFor > 0.7 ? 0.8 * Math.sin((quietFor - 0.7) * 2 * Math.PI * 0.6) : 0;
}

// ---- the check -----------------------------------------------------------

export const N = 72;
const A: [number, number] = [-6.5, 0.5];
const B: [number, number] = [-2, 5];
const C: [number, number] = [7.5, -5.5];
const L1 = Math.hypot(B[0] - A[0], B[1] - A[1]);
const L2 = Math.hypot(C[0] - B[0], C[1] - B[1]);
/** Arc length of the check. The line shortens to this before it bends, so it folds without stretching. */
export const CHECK_LEN = L1 + L2;
/** The check sampled by arc length into N+1 points, relative to the centre. */
export const CHECK: [number, number][] = Array.from({ length: N + 1 }, (_, i) => {
  const d = (i / N) * CHECK_LEN;
  if (d <= L1) {
    const f = d / L1;
    return [A[0] + (B[0] - A[0]) * f, A[1] + (B[1] - A[1]) * f];
  }
  const f = (d - L1) / L2;
  return [B[0] + (C[0] - B[0]) * f, B[1] + (C[1] - B[1]) * f];
});

/** Per-channel colour fade: `speed` per second, exact under reduced motion. */
export function fadeColor(cur: number[] | null, target: number[], dt: number, speed: number, snap = false): number[] {
  if (!cur || snap) return target.slice();
  const a = 1 - Math.exp(-dt * speed);
  return cur.map((v, i) => v + (target[i] - v) * a);
}
