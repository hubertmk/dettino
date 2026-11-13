import { AbsoluteFill } from "remotion";
import { Caption, Cur, Rise } from "../components/ui";
import { C } from "../theme";

export function Hook() {
  return (
    <AbsoluteFill style={{ background: C.desk, justifyContent: "center", padding: "0 180px" }}>
      <Rise at={6}>
        <Caption size={92}>You speak about 150 words a minute.</Caption>
      </Rise>
      <Rise at={48} style={{ marginTop: 28 }}>
        <Caption size={92} style={{ color: C.muted }}>
          You type about <span style={{ color: C.accent }}>40</span>.<Cur />
        </Caption>
      </Rise>
    </AbsoluteFill>
  );
}
