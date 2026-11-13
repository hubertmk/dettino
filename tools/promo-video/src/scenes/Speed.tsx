// The two measured latencies from the README, counting up.
import { AbsoluteFill, Easing, interpolate, useCurrentFrame } from "remotion";
import { Caption, Rise, Sub } from "../components/ui";
import { C, FONT } from "../theme";

const STATS = [
  { ms: 230, at: 20, label: "to transcribe 7 s of speech, on the CPU" },
  { ms: 240, at: 38, label: "to clean it up with a local model" },
];

export const SPEED_FRAMES = 150;

export function Speed() {
  const frame = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: C.desk }}>
      <Rise at={0} style={{ position: "absolute", left: 140, top: 120 }}>
        <Caption size={92}>Quick, even fully offline.</Caption>
      </Rise>
      <div style={{ position: "absolute", left: 140, top: 340, display: "flex", gap: 100 }}>
        {STATS.map((st) => {
          const n = interpolate(frame, [st.at, st.at + 36], [0, st.ms], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: Easing.out(Easing.cubic) });
          return (
            <Rise key={st.ms} at={st.at - 6} style={{ width: 760 }}>
              <div style={{ fontFamily: FONT, fontWeight: 650, fontSize: 220, letterSpacing: "-0.05em", color: C.accent, lineHeight: 1, fontVariantNumeric: "tabular-nums" }}>
                {Math.round(n)}
                <span style={{ fontSize: 90, letterSpacing: "-0.02em", marginLeft: 16 }}>ms</span>
              </div>
              <Sub style={{ marginTop: 26, fontSize: 38, color: C.ink }}>{st.label}</Sub>
            </Rise>
          );
        })}
      </div>
      <Rise at={80} style={{ position: "absolute", left: 140, top: 900 }}>
        <Sub style={{ fontSize: 26 }}>Median, warm, on the development laptop (RTX PRO 5000). Your times depend on your hardware.</Sub>
      </Rise>
    </AbsoluteFill>
  );
}
