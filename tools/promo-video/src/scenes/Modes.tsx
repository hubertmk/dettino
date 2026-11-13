// Private, Cloud, Balanced: the onboarding's three modes, with the fallback chain underneath.
import { AbsoluteFill, interpolate, useCurrentFrame } from "remotion";
import { Caption, Icon, Rise, Sub } from "../components/ui";
import { C, FONT } from "../theme";

const MODES = [
  {
    icon: "lock", title: "Private", sub: "everything on this computer",
    rows: [["lock", "Audio and text never leave this computer"], ["zap", "Free. Runs on local models"]],
  },
  {
    icon: "cloud", title: "Cloud", sub: "cloud hearing, cloud cleanup",
    rows: [["cloud", "Your own keys: OpenAI, Anthropic and more"], ["zap", "Under 1 s on a good connection"]],
  },
  {
    icon: "half", title: "Balanced", sub: "local hearing, cloud cleanup",
    rows: [["lock", "Audio stays here; only text goes out"], ["zap", "About 1 s"]],
  },
];

export const MODES_FRAMES = 280;
const PICK = 150;

export function Modes() {
  const frame = useCurrentFrame();
  const pick = interpolate(frame, [PICK, PICK + 12], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <AbsoluteFill style={{ background: C.desk }}>
      <Rise at={0} style={{ position: "absolute", left: 140, top: 120 }}>
        <Caption size={92}>You choose who hears you.</Caption>
      </Rise>
      <div style={{ position: "absolute", left: 140, top: 300, display: "flex", gap: 40 }}>
        {MODES.map((m, i) => {
          const on = i === 0 ? pick : 0;
          return (
            <Rise key={m.title} at={18 + i * 10} dy={40}>
              <div
                style={{
                  width: 520, height: 470, boxSizing: "border-box", padding: "40px 40px", borderRadius: 22, background: C.sheet,
                  boxShadow: `0 0 0 ${1 + 2 * on}px ${on > 0 ? `rgba(47,95,219,${0.3 + 0.7 * on})` : C.edge}, 0 30px 60px -34px rgba(0,0,0,.35)`,
                  fontFamily: FONT, transform: `translateY(${-10 * on}px)`,
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: 16, color: C.ink }}>
                  <Icon name={m.icon} size={40} color={C.accent} />
                  <span style={{ fontSize: 50, fontWeight: 650, letterSpacing: "-0.03em" }}>{m.title}</span>
                  {i === 0 && (
                    <span style={{ marginLeft: "auto", opacity: on, fontSize: 22, fontWeight: 600, color: C.accent, background: C.face, borderRadius: 999, padding: "6px 16px" }}>
                      Default
                    </span>
                  )}
                </div>
                <Sub style={{ fontSize: 28, marginTop: 10, marginBottom: 40 }}>{m.sub}</Sub>
                {m.rows.map(([ic, text]) => (
                  <div key={text} style={{ display: "flex", gap: 18, alignItems: "flex-start", fontSize: 30, lineHeight: 1.35, color: C.ink, marginBottom: 26 }}>
                    <span style={{ marginTop: 4 }}><Icon name={ic} size={30} color={C.muted} /></span>
                    {text}
                  </div>
                ))}
              </div>
            </Rise>
          );
        })}
      </div>
      <Rise at={PICK + 30} style={{ position: "absolute", left: 140, top: 850 }}>
        <Sub style={{ color: C.ink, fontSize: 38 }}>A provider is down? The next one takes over, automatically.</Sub>
      </Rise>
    </AbsoluteFill>
  );
}
