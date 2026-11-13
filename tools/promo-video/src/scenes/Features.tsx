// The everyday extras, as a grid of tiles.
import { AbsoluteFill } from "remotion";
import { Caption, Icon, Rise } from "../components/ui";
import { C, FONT } from "../theme";

const TILES = [
  ["book", "Dictionary", "Names and terms spelled right"],
  ["notes", "Snippets", "Say a cue, get the whole block"],
  ["sliders", "App rules", "A different tone for each app"],
  ["wand", "Commands", "Select text, say what to change"],
  ["mic", "Hands-free", "Talk without holding a key"],
  ["history", "History", "Every dictation, encrypted"],
];

export const FEATURES_FRAMES = 165;

export function Features() {
  return (
    <AbsoluteFill style={{ background: C.desk }}>
      <Rise at={0} style={{ position: "absolute", left: 140, top: 120 }}>
        <Caption size={92}>Made to fit how you write.</Caption>
      </Rise>
      <div style={{ position: "absolute", left: 140, top: 310, width: 1640, display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 36 }}>
        {TILES.map(([icon, title, text], i) => (
          <Rise key={title} at={14 + i * 6} dy={30}>
            <div style={{ height: 220, boxSizing: "border-box", padding: "36px 40px", borderRadius: 22, background: C.sheet, boxShadow: `0 0 0 1px ${C.edge}, 0 24px 50px -32px rgba(0,0,0,.3)`, fontFamily: FONT }}>
              <Icon name={icon} size={42} color={C.accent} />
              <div style={{ fontSize: 44, fontWeight: 650, letterSpacing: "-0.03em", color: C.ink, marginTop: 22 }}>{title}</div>
              <div style={{ fontSize: 29, color: C.muted, marginTop: 8 }}>{text}</div>
            </div>
          </Rise>
        ))}
      </div>
    </AbsoluteFill>
  );
}
