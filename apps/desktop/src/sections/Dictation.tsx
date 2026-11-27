import { useEffect, useState } from "react";
import { Play } from "lucide-react";
import { Accent, api, ChordSpec, chordLabel, CleanupLevel, Hotkeys, isMac, MODS, PillStyle, pillWord, SoundStyle, SuccessAnimation } from "../lib/api";
import { APP_NAME, osName } from "../lib/brand";
import { ACCENTS } from "../lib/theme";
import { useStore } from "../lib/store";
import { Banner, KeyRecorder, Row, Seg, Swatches, Toggle } from "../components/ui";

const CHORDS: { key: keyof Hotkeys; title: string; desc: string }[] = [
  { key: "push_to_talk", title: "Hold to talk", desc: "Hold, speak, release. A quick tap still reaches the app (e.g. autocomplete in VS Code)." },
  { key: "hands_free", title: "Hands-free", desc: "Press once to start. Space finishes, Esc cancels." },
  { key: "command", title: "Edit selection by voice", desc: "Select text, hold, say how to change it (\"make this more formal\")." },
  { key: "undo_ai", title: "Undo AI edit", desc: "Swap the cleaned text for exactly what you said — within 60 seconds." },
  { key: "paste_last", title: "Paste last transcript", desc: "Paste the last dictation again, wherever the cursor is." },
];

const EXAMPLES: Record<CleanupLevel, { said: string; typed: string }> = {
  off: { said: "um so let's meet at two actually three", typed: "Um so let's meet at two actually three." },
  light: { said: "um so let's meet at two actually three", typed: "Let's meet at three." },
  medium: { said: "so the build is like broken i think because of the cache we should clear it", typed: "The build is broken, probably because of the cache. We should clear it." },
};

const STYLE_DESC: Record<PillStyle, string> = {
  classic: "A small grey dot that grows into a rounded pill with a microphone and sound bars while you talk. Hover it for icon buttons.",
  line: "A single short line. It stretches and ripples while you talk, glides while the AI works, and the words sit right on it.",
};

const SOUND_STYLES: { value: SoundStyle; label: string; desc: string }[] = [
  { value: "classic", label: "Classic", desc: "Two short, clean beeps." },
  { value: "soft_pluck", label: "Soft pluck", desc: "The Classic notes, softened: they overlap and fade out." },
  { value: "mallet", label: "Mallet", desc: "Lower and warmer, like a small marimba." },
  { value: "glass", label: "Glass", desc: "Higher and bell-like, with a longer ring." },
  { value: "warm_pop", label: "Warm pop", desc: "Low, round and short, like a soft pop." },
];

const SUCCESS_DESC: Record<SuccessAnimation, string> = {
  flash: "The line turns green and swells for a moment. Nothing to read.",
  check: "The line bends into a check mark, then straightens.",
  check_word: "The check mark with the word “Pasted” above it.",
};

function conflictNote(c: ChordSpec): string | null {
  const ctrlSpace = c.mods === MODS.CTRL && c.key === 0x20;
  if (ctrlSpace && isMac())
    return "macOS uses Control + Space to switch input sources when you have more than one. A tap still switches; holding it dictates.";
  if (ctrlSpace) return "Also used for suggestions in VS Code and by some input-language switchers — those still work with a tap.";
  const label = chordLabel(c);
  if (label === "Ctrl + Alt + V" || label === "Ctrl + Alt + Z") return null;
  if (c.mods === 0 && c.key === 0x14) return "Caps Lock will no longer toggle capitals.";
  if (c.mods === MODS.WIN) return isMac() ? "macOS and most apps use many Command shortcuts; this one may be taken." : "Windows uses many Win+key shortcuts; this one may be taken.";
  if (c.mods === 0 && c.key === 0xff) return "Set “Press 🌐 key to” to “Do nothing” in System Settings → Keyboard, or the emoji picker opens too.";
  return null;
}

/** Languages Whisper and OpenAI transcription accept, as ISO 639-1 codes. */
const LANGUAGES: [string, string][] = [
  ["en", "English"], ["de", "German"], ["fr", "French"], ["es", "Spanish"], ["it", "Italian"], ["pt", "Portuguese"],
  ["nl", "Dutch"], ["pl", "Polish"], ["cs", "Czech"], ["sv", "Swedish"], ["da", "Danish"], ["nb", "Norwegian"], ["fi", "Finnish"],
  ["tr", "Turkish"], ["uk", "Ukrainian"], ["ru", "Russian"], ["ja", "Japanese"], ["zh", "Chinese"], ["ko", "Korean"],
];

export function Dictation() {
  const { snap, update } = useStore();
  const s = snap.settings;
  const [instruction, setInstruction] = useState(s.standing_instruction);
  useEffect(() => setInstruction(s.standing_instruction), [s.standing_instruction]);
  const conflicts = (() => {
    const seen = new Map<string, string>();
    const out: string[] = [];
    for (const c of CHORDS) {
      const l = chordLabel(s.hotkeys[c.key]);
      if (seen.has(l)) out.push(`${seen.get(l)} and ${c.title} use the same shortcut.`);
      seen.set(l, c.title);
    }
    return out;
  })();
  const ex = EXAMPLES[s.cleanup_level];
  return (
    <div className="page">
      <div className="page-head">
        <h1>Shortcuts &amp; cleanup</h1>
        <p>Shortcuts, how much the AI may change, and how hands-free works.</p>
      </div>

      {snap.hotkeys_error && <Banner kind="err">Shortcuts are unavailable: {snap.hotkeys_error}</Banner>}
      {conflicts.map((c) => <Banner key={c} kind="warn">{c}</Banner>)}

      <div className="card">
        <div className="card-head"><h3>Shortcuts</h3><span className="sub">Click a shortcut to change it. Any key works, including a single modifier like {isMac() ? "Right Option" : "Right Ctrl"}.</span></div>
        {CHORDS.map((c) => {
          const note = conflictNote(s.hotkeys[c.key]);
          return (
            <Row key={c.key} title={c.title} desc={<>{c.desc}{note && <><br /><span style={{ color: "var(--warning)" }}>{note}</span></>}</>}>
              <KeyRecorder label={c.title} value={s.hotkeys[c.key]} onChange={(v) => update((x) => (x.hotkeys[c.key] = v))} />
            </Row>
          );
        })}
        <Row title="Hold threshold" desc="Shorter than this is a tap and goes to the app. Speech always counts as a hold.">
          <div className="hstack">
            <input type="range" min={150} max={400} step={10} value={s.tap_max_ms} onChange={(e) => update((x) => (x.tap_max_ms = Number(e.target.value)))} aria-label="Hold threshold" />
            <span className="small" style={{ width: 52 }}>{s.tap_max_ms} ms</span>
          </div>
        </Row>
        <Row title="Double-tap hold-to-talk for hands-free" desc="Off by default so a double tap never replays twice to your app.">
          <Toggle label="Double-tap for hands-free" on={s.double_tap_handsfree} onChange={(v) => update((x) => (x.double_tap_handsfree = v))} />
        </Row>
      </div>

      <div className="card">
        <div className="card-head"><h3>Cleanup</h3><span className="sub">How much the AI may change what you said</span></div>
        <div className="card-pad stack">
          <Seg<CleanupLevel>
            label="Cleanup level"
            value={s.cleanup_level}
            onChange={(v) => update((x) => (x.cleanup_level = v))}
            options={[{ value: "off", label: "Off" }, { value: "light", label: "Light" }, { value: "medium", label: "Medium" }]}
          />
          <div className="grid2">
            <div className="stack" style={{ gap: 4 }}>
              <span className="small muted">You say</span>
              <span className="muted" style={{ fontStyle: "italic" }}>“{ex.said}”</span>
            </div>
            <div className="stack" style={{ gap: 4 }}>
              <span className="small muted">You get</span>
              <span>{ex.typed}</span>
            </div>
          </div>
          <p className="small muted">
            If the cleanup changes too much or adds words you didn't say, {APP_NAME} pastes what you said instead and tells you. The raw version is always in History.
          </p>
        </div>
        <div className="row" style={{ alignItems: "flex-start" }}>
          <div className="grow stack" style={{ gap: 6 }}>
            <div className="title">Standing instruction</div>
            <textarea
              className="textarea"
              rows={2}
              maxLength={400}
              aria-label="Standing instruction"
              placeholder='e.g. "Use British spelling. Never add greetings."'
              value={instruction}
              onChange={(e) => setInstruction(e.target.value)}
              onBlur={() => update((x) => (x.standing_instruction = instruction))}
            />
            <span className="small muted">Sent with every cleanup. {400 - instruction.length} characters left.</span>
          </div>
        </div>
        <Row title="Spoken lists" desc="“First…, second…, third…” and “bullet point…” become a list, one item per line. Never in terminals.">
          <Toggle label="Spoken lists" on={s.format_lists} onChange={(v) => update((x) => (x.format_lists = v))} />
        </Row>
      </div>

      <div className="card">
        <div className="card-head"><h3>Language</h3><span className="sub">What you speak</span></div>
        <Row
          title="Spoken language"
          desc="Whisper and cloud transcription follow this; Auto lets them detect it. Parakeet, the default on-device model, understands English only."
        >
          <select className="select" aria-label="Spoken language" value={s.language ?? ""} onChange={(e) => update((x) => (x.language = e.target.value || null))}>
            <option value="">Auto-detect</option>
            {LANGUAGES.map(([code, name]) => <option key={code} value={code}>{name}</option>)}
          </select>
        </Row>
      </div>

      <div className="card">
        <div className="card-head"><h3>Hands-free</h3></div>
        <Row title="Time limit" desc="A minute before the end the pill reminds you; at the limit it finishes and pastes — never discards.">
          <select className="select" aria-label="Hands-free time limit" value={s.handsfree_max_min} onChange={(e) => update((x) => (x.handsfree_max_min = Number(e.target.value)))}>
            {[5, 10, 20, 30, 60].map((m) => <option key={m} value={m}>{m} minutes</option>)}
          </select>
        </Row>
      </div>

      <div className="card">
        <div className="card-head"><h3>Feedback</h3></div>
        <Row title="Sounds" desc="Two quiet blips when listening starts and stops.">
          <div className="hstack">
            <input type="range" min={0} max={1} step={0.05} value={s.sound_volume} disabled={!s.sounds} onChange={(e) => update((x) => (x.sound_volume = Number(e.target.value)))} aria-label="Sound volume" />
            <Toggle label="Sounds" on={s.sounds} onChange={(v) => update((x) => (x.sounds = v))} />
          </div>
        </Row>
        <Row title="Sound style" desc={SOUND_STYLES.find((o) => o.value === s.sound_style)?.desc ?? ""}>
          <div className="hstack">
            <select
              className="select"
              aria-label="Sound style"
              value={s.sound_style}
              disabled={!s.sounds}
              onChange={(e) => {
                const v = e.target.value as SoundStyle;
                update((x) => (x.sound_style = v));
                api.previewSound(v);
              }}
            >
              {SOUND_STYLES.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
            </select>
            <button className="btn small" disabled={!s.sounds} onClick={() => api.previewSound(s.sound_style)}><Play /> Play</button>
          </div>
        </Row>
        <Row title="Pill style" desc={STYLE_DESC[s.pill_style]}>
          <Seg<PillStyle>
            label="Pill style"
            value={s.pill_style}
            onChange={(v) => update((x) => (x.pill_style = v))}
            options={[{ value: "classic", label: "Classic" }, { value: "line", label: "One line" }]}
          />
        </Row>
        <Row
          title="Accent colour"
          desc={`${s.accent === "surf" ? `Buttons and toggles. The ${pillWord(s.pill_style)} listens in blue and glows seafoam after a paste.` : `Buttons, toggles and the ${pillWord(s.pill_style)} while it listens.`} ${
            s.accent === "system" ? `Following your ${osName(snap.platform)} accent.` : `Now: ${ACCENTS.find((a) => a.id === s.accent)?.name}.`
          }`}
        >
          <Swatches<Accent>
            label="Accent colour"
            value={s.accent}
            onChange={(v) => update((x) => (x.accent = v))}
            options={[
              ...ACCENTS.map((a) => ({ value: a.id, label: a.name, light: a.fill ?? a.light, dark: a.dark })),
              { value: "system" as const, label: `System (${osName(snap.platform)} accent)` },
            ]}
          />
        </Row>
        {s.pill_style === "line" && (
          <Row title="After a paste" desc={SUCCESS_DESC[s.success_animation]}>
            <Seg<SuccessAnimation>
              label="Success animation"
              value={s.success_animation}
              onChange={(v) => update((x) => (x.success_animation = v))}
              options={[{ value: "flash", label: "Flash" }, { value: "check", label: "Check" }, { value: "check_word", label: "Check + Pasted" }]}
            />
          </Row>
        )}
        <Row title="Pill position" desc="Drag the pill anywhere; it remembers the spot for each display.">
          <button className="btn small" onClick={() => api.pillResetPosition()}>Reset position</button>
        </Row>
      </div>
    </div>
  );
}
