// Mark, name, the homepage headline, and where to find it.
import { AbsoluteFill } from "remotion";
import { Mark } from "../components/Mark";
import { Caption, Cur, Rise, Sub, useIn } from "../components/ui";
import { C } from "../theme";

export const END_FRAMES = 150;

export function End() {
  const p = useIn(4, 12, 160);
  return (
    <AbsoluteFill style={{ background: C.desk, alignItems: "center", justifyContent: "center", textAlign: "center" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 36, transform: `scale(${0.8 + 0.2 * p})`, opacity: Math.min(1, p) }}>
        <Mark size={180} />
        <Caption size={150} style={{ letterSpacing: "-0.045em" }}>Dettino</Caption>
      </div>
      <Rise at={26} style={{ marginTop: 56 }}>
        <Caption size={64} style={{ color: C.muted, fontWeight: 500 }}>Hold a key. Talk. It’s typed<Cur /></Caption>
      </Rise>
      <Rise at={50} style={{ marginTop: 40 }}>
        <Sub style={{ color: C.ink }}>Coming soon for Windows and Mac · <b style={{ fontWeight: 600 }}>dettino.com</b></Sub>
      </Rise>
    </AbsoluteFill>
  );
}
