# Spoken lists: "first…, second…, third…" becomes a list

Date: 2026-11-27. Status: proposed.

## Intent

When someone dictates an enumeration, Dettino should type a list, one item per line
with a number or a bullet, instead of a flat sentence that happens to contain the
words "first", "second" and "third". The result should look like what the speaker
would have typed by hand. Wispr Flow does this ("use numbers or sequence words such
as one, two or first, second to make a list automatically"; on platforms without
rich text it falls back to `- ` and `1.` lines), and it is the behaviour the user
expects from Dettino.

Today the opposite happens on purpose: the cleanup system prompt says "plain text
only, no markdown" and names only comma, period, new line and new paragraph as
spoken formatting, and the local rules know no list words at all.

## Goals

- "First, check the logs. Second, restart the service. Third, tell the team" is
  typed as a numbered list of three items.
- "Bullet point apples, bullet point pears" is typed as a bulleted list.
- A sentence before the first item ("Three things:") stays as its own line above the list.
- Works at every cleanup level, including Off, and whenever the AI is unavailable,
  times out or is rejected by the guard. Works with on-device models (Gemma 4 E2B
  through Ollama) as well as cloud models.
- Never fires on ordinary prose: "the first time I saw it" and "second thoughts"
  stay as they are.
- Can be switched off in one place.

## Non-goals

- Nested lists, check boxes, tables or headings.
- Editing an existing list on screen (that is the command path, unchanged).
- Rich-text (`<ul>`, `<ol>`) pasting. It is designed for below as a follow-up,
  not part of this change.

## Anti-goals

- A list where none was meant. A false positive is worse than a miss, because it
  reshapes the whole dictation and the user has to undo it by hand.
- Dropping or changing words. Only the sequence marker ("first", "bullet point")
  and its trailing comma or colon disappear; every other word survives, so the
  guard's word-count and precision checks still hold.
- Lists in terminals. Terminal targets get no formatting of any kind, as today.

## Constraints

- Local rules are the last link of every chain and all of cleanup Off
  (`crates/cleanup/src/rules.rs`), so the deterministic detector must live there.
- The cleanup system prompt is static so providers can cache it; one wording serves
  every provider and level (`crates/cleanup/src/prompt.rs`).
- The guard (`crates/cleanup/src/guard.rs`) rejects replies that look like
  markdown; list markers must not trip it.
- The output is pasted as plain text everywhere, with an HTML flavour on the
  clipboard whose only structure is `<br>` for newlines.

## Approaches considered

1. **Prompt only.** Teach the AI about lists and nothing else. Cheapest, but no list
   at cleanup Off, for dictations under 4 words, when the model is slow or offline,
   or when the guard rejects the reply. Small local models also follow such
   instructions inconsistently. Rejected by the user: they want both.
2. **Detector before the AI.** A local pass rewrites the transcript into list
   lines, then the AI cleans each line. The model sees the structure and keeps it,
   but any detector mistake is baked in before the model can judge it, and the
   transcript the guard compares against would no longer be what was said.
3. **Prompt plus detector after the AI (chosen).** The prompt tells the AI it may
   write list lines and when. A deterministic detector in the local rules turns
   unambiguous spoken enumerations into list lines. It runs on every output path:
   on the raw transcript for Off, short and fallback paths, and on the AI's text
   after the guard accepted it. It is idempotent, so a reply that is already a list
   passes through unchanged. The AI handles the fuzzy cases ("one… two… three" in
   prose); the detector guarantees the unambiguous ones.

## Design

### Detection (new module `crates/cleanup/src/lists.rs`)

A marker is recognised only at a clause start: the start of the text, or right after
`.`, `!`, `?`, `;`, `:`, `,` or a line break, optionally preceded by "and", "then" or
"and then" (the joiners belong to no item).
Matching is whole-word and case-insensitive, reusing `words::replace_word_ci`-style
boundaries.

| Family | Spoken forms | Output | Rule |
|---|---|---|---|
| Ordinals | first, second, third … tenth; firstly, secondly, thirdly; "first of all", "first off"; last, lastly, finally close a list | `1. ` … | Must begin with first/firstly and run in order; "third" without "second" is not a list. The idioms "first and foremost" and "first things first" are not markers. |
| Numbered | number one, number two …; point one, point two …; a digit with an attached period as ASR writes it (`1.` `2.`) | `1. ` … | Must begin with one and run in order. "number N"/"point N" need a pause after the number (`,` `:` `.`), so "number one priority" stays prose. Digit markers count anywhere, not only at clause starts, because ASR rarely punctuates before them. |
| Bullets | bullet point, bullet, new bullet, next bullet, next point, new point | `- ` | Any count of two or more. |

- Bare cardinals ("one, two, three") are left to the AI: in local rules they are
  too often ordinary speech ("one more thing").
- A list needs at least two items.
- The marker and a directly following comma or colon are removed. Nothing else
  is removed.
- Text before the first marker is kept as a lead-in line above the list.
- An item runs to the next marker; the last item runs to the end of the text.
- Each item starts with a capital letter and loses a trailing period or comma
  (a `?` or `!` stays). No period is appended; the `trailing_period` option is
  suppressed for list text.
- If the text already contains two or more lines starting with `- `, `* `, `• `
  or `N. `, the detector returns it unchanged (idempotence).

Worked examples (level Off or fallback):

```
first check the logs comma second restart the service comma third tell the team
→ 1. Check the logs
  2. Restart the service
  3. Tell the team

three things. first, the budget. second, hiring. and finally, the offsite
→ Three things.
  1. The budget
  2. Hiring
  3. The offsite

bullet point apples bullet point pears
→ - Apples
  - Pears

the first time I saw it I had second thoughts
→ The first time I saw it I had second thoughts.      (no list: markers not at clause starts)

second we leave third we eat
→ Second we leave third we eat.                          (no list: no "first")
```

### Local rules (`rules.rs`)

- `RuleOptions` gains `lists: bool`. The pipeline sets it from the new setting and
  forces it off for the terminal category.
- `apply` runs the detector after `spoken_punctuation` and before `tidy`. Line
  breaks already make `capitalize_sentences` capitalise the next word, and `tidy`
  works line by line, so nothing else changes. `trailing_period` is skipped when
  the result is a list.

### Prompt (`prompt.rs`)

Add one rule to `CLEANUP_SYSTEM` and relax the plain-text line:

- "Lists: when the speaker enumerates items with sequence words (first/second/third,
  one/two/three, number one, bullet point, next point), write each item on its own
  line: numbered items as `1. `, `2. `, bullets as `- `. Drop the spoken marker and
  keep the speaker's words. A sentence spoken before the first item stays as its
  own line. Do not make a list when the sequence word is part of ordinary prose."
- "Plain text only: no quotes, no markdown except the list markers above, no
  preface or explanation."

The system prompt stays static, so caching is unaffected. The user block does not
change.

### Guard (`guard.rs`)

No new rejection. Confirm with tests that `1. …` and `- …` lines pass
`looks_like_a_reply`, the word-count ratio and the precision check (digits already
count as known words). Add a `Reply` test that `**` inside a list is still rejected.

### Pipeline (`apps/desktop/src-tauri/src/pipeline.rs`)

- `rule_opts` reads `s.format_lists` and the terminal flag into `RuleOptions.lists`.
- After `check_cleanup` accepts the AI text, run the detector on it (same options).
  The guard compares the model's reply with the transcript; the detector runs after,
  so it can never cause a rejection.
- If the final text is a list and `before` is non-empty and does not end with a
  line break, prepend `\n` so the list starts on its own line. This sits next to
  the existing `leading_space` decision.
- `undo_text` stays the raw transcript with default rules, as today.

### Setting and UI

- `Settings.format_lists: bool`, default `true`, `#[serde(default)]` so old
  settings files load (`crates/core/src/config.rs`, mirrored in
  `apps/desktop/src/lib/api.ts`).
- A toggle in Settings → Shortcuts & cleanup, in the Cleanup card
  (`apps/desktop/src/sections/Dictation.tsx`): "Spoken lists. Turn 'first…,
  second…, third…' and 'bullet point…' into a list, one item per line." Global only;
  no per-app rule in this change, terminals are excluded automatically.
- `docs/setup-guide.md` tip line gains the list words next to "comma" and
  "new paragraph".

### Follow-up, out of scope here: rich-text lists

Both clipboard writers already offer an HTML flavour next to the plain text
(`crates/platform/src/mac/clipboard.rs`, `crates/platform/src/win/clipboard.rs`,
both through `escape_html` in `crates/platform/src/lib.rs`). A later change can
render list lines as `<ol>`/`<ul>` in that flavour so Slack, Gmail, Word and Notion
receive real list items while code editors and plain fields keep `1. ` / `- `. It
needs testing against each target app and is kept separate.

## Decisions left to the builder

- Exact word boundary handling for multi-word markers ("bullet point" vs "bullet")
  and whether "point" alone is too risky (recommendation: require "next point" or
  "new point", never bare "point").
- Whether the lead-in line gets a colon when it ends mid-sentence (recommendation:
  leave the speaker's punctuation; `trailing_period` already adds a period when
  it is the whole text, and the lead-in is not).
- Test fixtures beyond the worked examples above.

## Verification

- `cargo test -p app-cleanup` for the detector, rules, prompt and guard tests.
- `cargo test` in `apps/desktop/src-tauri` for the pipeline newline-before-list test.
- `pnpm test` and `pnpm typecheck` in `apps/desktop` for the settings type and toggle.
- Manual: dictate the worked examples into a plain text editor with cleanup Off,
  Light and Medium, and once with Ollama stopped (fallback path). Dictate "the first
  time I saw it" and confirm no list. Dictate into a terminal and confirm no list.
