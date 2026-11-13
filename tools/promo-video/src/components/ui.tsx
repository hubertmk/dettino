// Shared pieces: keycaps, a generic app window, headline captions, and the dictated words.
import React from "react";
import { interpolate, spring, useCurrentFrame, useVideoConfig } from "remotion";
import { C, FONT, MONO } from "../theme";

const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;

/** 0 → 1 spring that starts at `at` (frames). */
export function useIn(at: number, damping = 18, stiffness = 140) {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return spring({ frame: frame - at, fps, config: { damping, stiffness } });
}

/** Fade + rise in at `at`, optional fade out at `out`. */
export function Rise({ at, out, dy = 24, children, style }: { at: number; out?: number; dy?: number; children: React.ReactNode; style?: React.CSSProperties }) {
  const frame = useCurrentFrame();
  const p = useIn(at);
  const o = out === undefined ? 1 : interpolate(frame, [out, out + 10], [1, 0], clamp);
  return <div style={{ opacity: Math.min(p, 1) * o, transform: `translateY(${(1 - p) * dy}px)`, ...style }}>{children}</div>;
}

export function Caption({ children, size = 88, style }: { children: React.ReactNode; size?: number; style?: React.CSSProperties }) {
  return (
    <div style={{ fontFamily: FONT, fontWeight: 650, fontSize: size, lineHeight: 1.06, letterSpacing: "-0.035em", color: C.ink, ...style }}>
      {children}
    </div>
  );
}

export function Sub({ children, style }: { children: React.ReactNode; style?: React.CSSProperties }) {
  return <div style={{ fontFamily: FONT, fontSize: 34, lineHeight: 1.4, color: C.muted, letterSpacing: "-0.01em", ...style }}>{children}</div>;
}

/** The homepage's accent caret after a headline. */
export function Cur({ blink = true }: { blink?: boolean }) {
  const frame = useCurrentFrame();
  const on = !blink || Math.floor(frame / 16) % 2 === 0;
  return (
    <span style={{ display: "inline-block", width: "0.07em", height: "0.8em", borderRadius: 2, background: C.accent, marginLeft: "0.08em", verticalAlign: "-0.03em", opacity: on ? 1 : 0 }} />
  );
}

/** A keycap that presses down while `down` is 1. */
export function Keycap({ label, down, width = 150, size = 1 }: { label: string; down: number; width?: number; size?: number }) {
  const h = 110 * size;
  const travel = 8 * size;
  return (
    <div style={{ position: "relative", width: width * size, height: h + travel }}>
      <div style={{ position: "absolute", inset: `${travel}px 0 0 0`, borderRadius: 18 * size, background: "#C9C9D2" }} />
      <div
        style={{
          position: "absolute", left: 0, right: 0, top: down * travel, height: h, borderRadius: 18 * size,
          background: down > 0.5 ? mixHex(C.sheet, C.face, down) : C.sheet,
          boxShadow: `inset 0 0 0 ${1.5 * size}px ${down > 0.02 ? `rgba(47,95,219,${0.55 * down})` : C.edge}`,
          display: "flex", alignItems: "center", justifyContent: "center",
          fontFamily: MONO, fontWeight: 500, fontSize: 34 * size, color: down > 0.5 ? C.accent : C.ink,
        }}
      >
        {label}
      </div>
    </div>
  );
}

function mixHex(a: string, b: string, t: number) {
  const p = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const [x, y] = [p(a), p(b)];
  return `rgb(${x.map((v, i) => Math.round(v + (y[i] - v) * t)).join(",")})`;
}

export function Plus({ size = 1 }: { size?: number }) {
  return <div style={{ fontFamily: FONT, fontSize: 40 * size, color: C.muted, padding: `0 ${14 * size}px`, paddingBottom: 8 * size }}>+</div>;
}

const ICONS: Record<string, React.ReactNode> = {
  mail: <><rect x="3" y="5" width="18" height="14" rx="2" /><path d="m3 7 9 6 9-6" /></>,
  chat: <path d="M21 12a8 8 0 0 1-11.6 7.1L4 20l1-4.6A8 8 0 1 1 21 12Z" />,
  code: <><path d="m8 8-4 4 4 4" /><path d="m16 8 4 4-4 4" /></>,
  notes: <><path d="M6 3h9l4 4v14H6z" /><path d="M9 12h7M9 16h5" /></>,
  lock: <><rect x="4" y="11" width="16" height="10" rx="2" /><path d="M8 11V7a4 4 0 0 1 8 0v4" /></>,
  zap: <path d="M13 2 4 14h7l-1 8 9-12h-7z" />,
  cloud: <path d="M7 18a5 5 0 1 1 1-9.9A6 6 0 0 1 19.5 10 4 4 0 0 1 18 18z" />,
  half: <><circle cx="12" cy="12" r="9" /><path d="M12 3a9 9 0 0 1 0 18z" fill="currentColor" /></>,
  book: <><path d="M4 5a2 2 0 0 1 2-2h14v16H6a2 2 0 0 0-2 2z" /><path d="M4 19V5" /></>,
  scissors: <><circle cx="6" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><path d="M20 4 8.1 15.9M14.5 14.5 20 20M8.1 8.1 12 12" /></>,
  sliders: <><path d="M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0" /><circle cx="16" cy="6" r="2" /><circle cx="10" cy="12" r="2" /><circle cx="18" cy="18" r="2" /></>,
  wand: <><path d="m15 4 5 5L9 20H4v-5z" /><path d="m12 7 5 5" /></>,
  mic: <><rect x="9" y="3" width="6" height="11" rx="3" /><path d="M5 11a7 7 0 0 0 14 0M12 18v3" /></>,
  history: <><path d="M3 12a9 9 0 1 0 3-6.7L3 8" /><path d="M3 3v5h5M12 7v5l3 2" /></>,
};

export function Icon({ name, size = 28, color = "currentColor", width = 2 }: { name: string; size?: number; color?: string; width?: number }) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth={width} strokeLinecap="round" strokeLinejoin="round" style={{ flex: "none" }}>
      {ICONS[name]}
    </svg>
  );
}

/** Generic app window: header with the app's icon and name on the left, context on the right. */
export function Window({ app, name, ctx, width, height, scale = 1, children }: {
  app: string; name: string; ctx: React.ReactNode; width: number; height: number; scale?: number; children: React.ReactNode;
}) {
  const k = scale;
  return (
    <div
      style={{
        width, height, background: C.sheet, borderRadius: 14 * k, padding: `${26 * k}px ${32 * k}px`, boxSizing: "border-box",
        boxShadow: "0 2px 4px rgba(0,0,0,.06), 0 30px 60px -30px rgba(0,0,0,.35)", display: "flex", flexDirection: "column",
        fontFamily: FONT,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", paddingBottom: 18 * k, marginBottom: 24 * k, borderBottom: `1px solid ${C.line}`, color: C.muted, fontSize: 22 * k }}>
        <span style={{ display: "flex", alignItems: "center", gap: 10 * k, fontWeight: 600 }}>
          <Icon name={app} size={24 * k} />
          {name}
        </span>
        <span>{ctx}</span>
      </div>
      <div style={{ flex: 1, position: "relative" }}>{children}</div>
    </div>
  );
}

/** A grey placeholder line, like the homepage's `.ln`. */
export function Ln({ w, k = 1 }: { w: string; k?: number }) {
  return <i style={{ display: "block", height: 14 * k, width: w, background: C.line, borderRadius: 7 * k, marginBottom: 18 * k }} />;
}

export function Caret({ k = 1 }: { k?: number }) {
  const frame = useCurrentFrame();
  return (
    <span style={{ display: "inline-block", width: 3 * k, height: "1.1em", background: C.accent, marginLeft: 3 * k, verticalAlign: "-0.2em", opacity: Math.floor(frame / 16) % 2 === 0 ? 1 : 0 }} />
  );
}

/** One spoken word and what cleanup turns it into ('' drops it). `clean` undefined keeps it as is. */
export type Said = [raw: string, clean?: string];

/**
 * Words that arrive as said (muted, from `start`, one every `every` frames) and, from `tidy`,
 * drop their filler and snap into the cleaned sentence, in ink.
 */
export function Dictation({ words, start, every, tidy, size, caret = true, mono = false }: {
  words: Said[]; start: number; every: number; tidy: number; size: number; caret?: boolean; mono?: boolean;
}) {
  const frame = useCurrentFrame();
  const done = frame >= tidy + words.length * 2 + 8;
  return (
    <div style={{ fontFamily: mono ? MONO : FONT, fontSize: size, lineHeight: 1.45, letterSpacing: mono ? 0 : "-0.01em", color: C.ink }}>
      {words.map(([raw, clean], i) => {
        const shownAt = start + i * every;
        const appear = interpolate(frame, [shownAt, shownAt + 5], [0, 1], clamp);
        const t = interpolate(frame, [tidy + i * 2, tidy + i * 2 + 8], [0, 1], clamp);
        const drop = clean === "";
        const text = t > 0.5 && clean ? clean : raw;
        const maxW = drop ? interpolate(t, [0, 1], [(raw.length + 1) * 0.7, 0]) : undefined;
        return (
          <span
            key={i}
            style={{
              display: frame < shownAt ? "none" : "inline-block", verticalAlign: "top", whiteSpace: "pre", overflow: drop ? "hidden" : undefined, maxWidth: maxW === undefined ? undefined : `${maxW}em`,
              opacity: appear * (drop ? 1 - t : 1), transform: `translateY(${(1 - appear) * 0.3}em)`,
              color: t > 0.5 ? C.ink : C.muted, filter: clean && t > 0.25 && t < 0.75 ? "blur(1.5px)" : undefined,
            }}
          >
            {text}
            {drop && t >= 1 ? "" : " "}
          </span>
        );
      })}
      {caret && (frame < start || done) && <Caret />}
    </div>
  );
}
