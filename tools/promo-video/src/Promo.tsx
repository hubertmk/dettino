import { linearTiming, TransitionSeries } from "@remotion/transitions";
import { fade } from "@remotion/transitions/fade";
import { AnyApp, ANYAPP_FRAMES } from "./scenes/AnyApp";
import { Dictate, DICTATE_FRAMES } from "./scenes/Dictate";
import { End, END_FRAMES } from "./scenes/End";
import { Features, FEATURES_FRAMES } from "./scenes/Features";
import { Hook } from "./scenes/Hook";
import { Modes, MODES_FRAMES } from "./scenes/Modes";
import { Speed, SPEED_FRAMES } from "./scenes/Speed";

const CUT = 12;
const SCENES = [
  [Hook, 120],
  [Dictate, DICTATE_FRAMES],
  [AnyApp, ANYAPP_FRAMES],
  [Modes, MODES_FRAMES],
  [Speed, SPEED_FRAMES],
  [Features, FEATURES_FRAMES],
  [End, END_FRAMES],
] as const;

export const PROMO_FRAMES = SCENES.reduce((n, [, d]) => n + d, 0) - CUT * (SCENES.length - 1);

/** Frame each scene starts at in the full video, for stills. */
export const SCENE_STARTS = SCENES.map((_, i) => SCENES.slice(0, i).reduce((n, [, d]) => n + d - CUT, 0));

export function Promo() {
  return (
    <TransitionSeries>
      {SCENES.flatMap(([Scene, d], i) => [
        ...(i ? [<TransitionSeries.Transition key={`t${i}`} presentation={fade()} timing={linearTiming({ durationInFrames: CUT })} />] : []),
        <TransitionSeries.Sequence key={i} durationInFrames={d}>
          <Scene />
        </TransitionSeries.Sequence>,
      ])}
    </TransitionSeries>
  );
}
