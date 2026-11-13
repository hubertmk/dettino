// The same gesture in three more apps, back to back.
import { AbsoluteFill } from "remotion";
import { LinePill } from "../components/LinePill";
import { Caption, Dictation, Ln, Rise, Said, Window } from "../components/ui";
import { C } from "../theme";

const APPS: { app: string; name: string; ctx: string; mono?: boolean; pre: string[]; words: Said[] }[] = [
  {
    app: "chat", name: "Chat", ctx: "Martin", pre: ["62%", "40%"],
    words: [["uh", ""], ["the", "The"], ["drawings"], ["look"], ["like", ""], ["really"], ["good", "good,"], ["thanks", "thanks!"]],
  },
  {
    app: "code", name: "Editor", ctx: "retry.ts", mono: true, pre: ["52%", "70%"],
    words: [["comment", "//"], ["retry", "Retry"], ["on"], ["four two nine", "429,"], ["at"], ["most"], ["three", "3"], ["times", "times."]],
  },
  {
    app: "notes", name: "Notes", ctx: "Today", pre: ["58%"],
    words: [["okay", ""], ["so", ""], ["order", "Order"], ["two kilos", "2 kg"], ["of"], ["petg", "PETG"], ["before"], ["the"], ["fourteenth", "14th."]],
  },
];

export const ANYAPP_FRAMES = 200;

export function AnyApp() {
  return (
    <AbsoluteFill style={{ background: C.desk }}>
      <Rise at={0} style={{ position: "absolute", left: 140, top: 120 }}>
        <Caption size={92}>Works in any app.</Caption>
      </Rise>
      <Rise at={10} style={{ position: "absolute", left: 140, top: 230 }}>
        <Caption size={92} style={{ color: C.muted }}>Wherever your cursor is.</Caption>
      </Rise>
      {APPS.map((a, i) => {
        const base = 26 + i * 40;
        const start = base + 4;
        const every = 3;
        const up = start + a.words.length * every + 6;
        const tidy = up + 10;
        const pasted = tidy + a.words.length * 2 + 6;
        const left = 140 + i * 560;
        return (
          <Rise key={a.app} at={base - 14} style={{ position: "absolute", left, top: 400 }}>
            <Window app={a.app} name={a.name} ctx={a.ctx} width={520} height={400} scale={0.85}>
              {a.pre.map((w, j) => <Ln key={j} w={w} k={0.85} />)}
              <Dictation words={a.words} start={start} every={every} tidy={tidy} size={30} mono={a.mono} caret={false} />
            </Window>
            <div style={{ position: "absolute", left: 260 - 360, top: 380 }}>
              <LinePill
                scale={2}
                cues={[
                  { at: base, phase: "listening" },
                  { at: up, phase: "thinking" },
                  { at: pasted, phase: "flash" },
                  { at: pasted + 40, phase: "idle" },
                ]}
              />
            </div>
          </Rise>
        );
      })}
    </AbsoluteFill>
  );
}
