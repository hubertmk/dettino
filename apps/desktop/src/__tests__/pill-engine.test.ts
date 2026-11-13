import { describe, expect, it } from "vitest";
import { CFG, CHECK, CHECK_LEN, N, SPRING, Spring, breath, driftY, emit, lineTarget, prune, rippleY, win } from "../pill/engine";

/** Run a spring at 120 Hz for `seconds`, returning the peak overshoot past the target. */
function run(s: Spring, seconds: number) {
  let over = 0;
  for (let i = 0; i < seconds * 120; i++) {
    s.step(1 / 120);
    over = Math.max(over, s.x - s.t);
  }
  return over;
}

describe("Spring", () => {
  it("entry overshoots once by a few percent and settles within half a second", () => {
    const s = new Spring(12).to(28, SPRING.in);
    const over = run(s, 0.5);
    expect(over).toBeGreaterThan(0.1);
    expect(over).toBeLessThan((28 - 12) * 0.06);
    expect(s.settled).toBe(true);
  });
  it("exit never overshoots", () => {
    const s = new Spring(28).to(12, SPRING.out);
    let under = 0;
    for (let i = 0; i < 60; i++) {
      s.step(1 / 120);
      under = Math.min(under, s.x - 12);
    }
    expect(under).toBe(0);
    expect(s.settled).toBe(true);
  });
  it("snaps under reduced motion", () => {
    const s = new Spring(0).to(10);
    s.step(0.016, true);
    expect(s.x).toBe(10);
    expect(s.settled).toBe(true);
  });
});

describe("lineTarget", () => {
  it("uses the decided lengths", () => {
    expect(lineTarget("idle").L * 2).toBe(CFG.idleLen);
    expect(lineTarget("listening").L * 2).toBe(CFG.talkLen);
    expect(lineTarget("thinking").L * 2).toBe(CFG.thinkLen);
    expect(lineTarget("flash").L * 2).toBe(CFG.flashLen);
  });
  it("lets text on the line win when it is wider", () => {
    expect(lineTarget("listening", 10).L).toBe(CFG.talkLen / 2);
    expect(lineTarget("listening", 80).L).toBe(80 + CFG.textPadOpen);
    expect(lineTarget("hover", 40).L).toBe(40 + CFG.textPadIdle);
  });
  it("shortens to the check's arc length before bending, so the fold does not stretch", () => {
    expect(lineTarget("check").L * 2).toBeCloseTo(CHECK_LEN, 5);
    expect(CHECK_LEN).toBeGreaterThan(20);
    expect(CHECK_LEN).toBeLessThan(21);
    expect(CHECK).toHaveLength(N + 1);
  });
});

describe("meter", () => {
  it("emits one wavelet per 70 ms of speech and none in silence", () => {
    const wl: ReturnType<typeof prune> = [];
    let last = -1;
    last = emit(wl, 0, 0.5, last);
    last = emit(wl, 0.03, 0.5, last);
    last = emit(wl, 0.08, 0.5, last);
    emit(wl, 0.2, 0.01, last);
    expect(wl.map((w) => w.born)).toEqual([0, 0.08]);
  });
  it("ripples travel outward from the centre and fade", () => {
    const wl = [{ born: 0, amp: 1 }];
    const early = Math.abs(rippleY(wl, 0, 0.05, 28));
    const far = Math.abs(rippleY(wl, 20, 0.05, 28));
    expect(early).toBeGreaterThan(far);
    expect(Math.abs(rippleY(wl, 0, 1.5, 28))).toBeLessThan(0.05);
    expect(prune(wl, 1.7)).toEqual([]);
  });
  it("pins the ends flat and breathes only after 700 ms of silence", () => {
    expect(win(0)).toBeLessThan(0.001);
    expect(win(0.5)).toBe(1);
    expect(breath(0.5)).toBe(0);
    expect(Math.abs(breath(1.1))).toBeGreaterThan(0.1);
  });
});

describe("thinking (Drift)", () => {
  it("never lies flat: the raised line is at least 0.7 px up at the centre", () => {
    for (let tt = 0; tt < 3; tt += 0.1) {
      expect(driftY(0, 0.5, 22, tt, 1)).toBeLessThanOrEqual(-0.7);
    }
  });
  it("carries a hump of about 4 px that moves", () => {
    const peak = (tt: number) => {
      let best = 0;
      for (let i = 0; i <= 44; i++) {
        const x = -22 + i;
        best = Math.min(best, driftY(x, (x + 22) / 44, 22, tt, 1));
      }
      return best;
    };
    expect(peak(0)).toBeLessThan(-4);
    const at = (tt: number) => {
      let bx = 0, best = 0;
      for (let i = 0; i <= 44; i++) {
        const x = -22 + i;
        const y = driftY(x, (x + 22) / 44, 22, tt, 1);
        if (y < best) { best = y; bx = x; }
      }
      return bx;
    };
    expect(at(0.75)).toBeGreaterThan(at(0) + 5);
  });
});
