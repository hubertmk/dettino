// The core loop: hold Ctrl+Space, talk, let go, and the cleaned-up text lands at the cursor.
import { AbsoluteFill } from "remotion";
import { LinePill } from "../components/LinePill";
import { Caption, Cur, Dictation, Keycap, Ln, Plus, Rise, Said, useIn, Window } from "../components/ui";
import { C, FONT } from "../theme";

const WORDS: Said[] = [
  ["so", ""], ["um", ""], ["can", "Can"], ["we"], ["move"], ["the"], ["review"], ["to"], ["thursday", "Thursday"],
  ["ten thirty", "at 10:30?"], ["martin", "Martin"], ["cant", "can’t"], ["make"], ["wednesday", "Wednesday."],
];
const DOWN = 30;
const START = 40;
const EVERY = 7;
const UP = 160;
const TIDY = 172;
const PASTED = TIDY + WORDS.length * 2 + 6;

export const DICTATE_FRAMES = 300;

export function Dictate() {
  const down = Math.max(0, Math.min(1, useIn(DOWN, 14, 260) - useIn(UP, 14, 260)));
  return (
    <AbsoluteFill style={{ background: C.desk }}>
      <div style={{ position: "absolute", left: 140, top: 230, width: 640 }}>
        <Rise at={0}><Caption size={100}>Hold a key.</Caption></Rise>
        <Rise at={DOWN + 8}><Caption size={100}>Talk.</Caption></Rise>
        <Rise at={UP}><Caption size={100}>Let go.</Caption></Rise>
        <Rise at={PASTED}>
          <Caption size={100} style={{ color: C.accent }}>It’s typed.<Cur /></Caption>
        </Rise>
      </div>
      <div style={{ position: "absolute", left: 140, top: 760, display: "flex", alignItems: "center" }}>
        <Keycap label="Ctrl" down={down} width={170} />
        <Plus />
        <Keycap label="Space" down={down} width={330} />
      </div>
      <div style={{ position: "absolute", left: 880, top: 150 }}>
        <Window app="mail" name="Mail" ctx="To: Lena" width={900} height={560}>
          <div style={{ fontFamily: FONT, fontSize: 36, color: C.ink, marginBottom: 22 }}>Hi Lena,</div>
          <Dictation words={WORDS} start={START} every={EVERY} tidy={TIDY} size={36} />
          <div style={{ position: "absolute", bottom: 0, left: 0, right: 0 }}>
            <Ln w="34%" />
          </div>
        </Window>
      </div>
      <div style={{ position: "absolute", left: 880 + 450 - 720, top: 620 }}>
        <LinePill
          cues={[
            { at: DOWN + 2, phase: "listening" },
            { at: UP, phase: "thinking" },
            { at: PASTED, phase: "flash" },
            { at: PASTED + 50, phase: "idle" },
          ]}
          scale={4}
        />
      </div>
    </AbsoluteFill>
  );
}
