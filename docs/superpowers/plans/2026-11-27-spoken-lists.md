# Spoken Lists Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dictated enumerations ("first…, second…, third…", "bullet point…") are typed as a list, one item per line, at every cleanup level and on every fallback path.

**Architecture:** A deterministic detector in the cleanup crate's local rules turns unambiguous spoken enumerations into `1. ` / `- ` lines and is idempotent. The static cleanup system prompt additionally tells the AI it may write such lines. The pipeline runs the detector on the raw transcript (Off, short, fallback) and on the AI's text after the guard accepted it, and starts a list on its own line. One global setting switches it off; terminals never get it.

**Tech Stack:** Rust 2021 workspace (`app-cleanup`, `app-core`, `dettino_lib`), Tauri 2, React 19 + TypeScript, vitest.

**Spec:** `docs/superpowers/specs/2026-11-27-spoken-lists-design.md`

## Global Constraints

- Only the spoken marker and a directly following comma or colon are removed; every other word survives (the guard's word-count and precision checks depend on it).
- A list needs at least two items. Ordinal and "number N" lists must start at one and run in order.
- Markers count only at a clause start: start of text, or after `.` `!` `?` `;` `:` `,` or a line break, optionally preceded by "and" or "then".
- Output markers are exactly `1. ` (digit, period, space) and `- ` (hyphen, space). No other markdown.
- `CLEANUP_SYSTEM` stays a `const &str` (cacheable); only its wording changes.
- Terminal category (`app_platform::app_category(..) == "terminal"`) never gets list formatting.
- Setting name is `format_lists`, default `true`, in both Rust and TypeScript.
- Commit messages follow the repo style: imperative sentence, no prefix (see `git log`).

## Review Focus

1. ASR writes a digit with a period ("1. check the logs 2. restart") instead of words: the detector must treat `1.`/`2.` at clause starts as numbered markers, or at least leave the text intact (Task 1 test `digit_markers_from_asr`).
2. A marker word spoken at a clause start in prose ("Second, I disagree.") with no "first" anywhere: must not become a list (Task 1 test `no_list_without_first`).
3. A list dictated into a field whose text before the caret ends mid-line ("Notes:"): the list has to start on a new line, not glued to "Notes:" (Task 2 test `a_list_starts_on_its_own_line`).
4. The AI already returned a list and the detector runs again: no double markers (Task 1 test `already_a_list_is_unchanged`).
5. Dictionary term or identifier inside an item ("second, rename user_count"): must survive the marker removal and still pass the guard (Task 3 test `a_list_reply_passes_the_guard`).

---

### Task 1: List detector

**Files:**
- Create: `crates/cleanup/src/lists.rs`
- Modify: `crates/cleanup/src/lib.rs` (add `pub mod lists;`)

**Interfaces:**
- Consumes: `crate::words::replace_word_ci` boundary semantics (word chars are alphanumeric or `_`).
- Produces:
  - `pub fn format(text: &str) -> Option<String>`: `Some(list)` when the text is a spoken enumeration, `None` otherwise. The returned string is lead-in line (if any) followed by one line per item, each starting with `1. `/`2. `… or `- `, items capitalised and stripped of a trailing `.` or `,` (`?`/`!` stay), no period appended, no trailing newline.
  - `pub fn is_list(text: &str) -> bool`: true when two or more lines start with `- `, `* `, `• ` or `<digits>. `.

- [ ] **Step 1: Write the failing tests** in `crates/cleanup/src/lists.rs` under `#[cfg(test)]`:

```rust
#[test]
fn ordinals_make_a_numbered_list() {
    assert_eq!(
        format("first check the logs, second restart the service, third tell the team").as_deref(),
        Some("1. Check the logs\n2. Restart the service\n3. Tell the team")
    );
    assert_eq!(
        format("firstly, the budget. secondly, hiring. and finally, the offsite").as_deref(),
        Some("1. The budget\n2. Hiring\n3. The offsite")
    );
}

#[test]
fn a_lead_in_sentence_stays_above_the_list() {
    assert_eq!(
        format("three things. first, the budget. second, hiring").as_deref(),
        Some("Three things.\n1. The budget\n2. Hiring")
    );
}

#[test]
fn number_n_makes_a_numbered_list() {
    assert_eq!(format("number one apples, number two pears").as_deref(), Some("1. Apples\n2. Pears"));
    assert_eq!(format("point one apples. point two pears").as_deref(), Some("1. Apples\n2. Pears"));
}

#[test]
fn bullet_commands_make_a_bulleted_list() {
    assert_eq!(format("bullet point apples bullet point pears").as_deref(), Some("- Apples\n- Pears"));
    assert_eq!(format("shopping. bullet apples, next bullet pears, next point milk").as_deref(), Some("Shopping.\n- Apples\n- Pears\n- Milk"));
}

#[test]
fn digit_markers_from_asr() {
    assert_eq!(format("1. check the logs 2. restart").as_deref(), Some("1. Check the logs\n2. Restart"));
}

#[test]
fn ordinary_prose_is_not_a_list() {
    assert_eq!(format("the first time I saw it I had second thoughts"), None);
    assert_eq!(format("we won first place and then second place"), None);
    assert_eq!(format("one more thing, two if you count the dog"), None); // bare cardinals are the AI's job
    assert_eq!(format("bullet point apples"), None); // one item is not a list
}

#[test]
fn no_list_without_first() {
    assert_eq!(format("second we leave third we eat"), None);
    assert_eq!(format("second, I disagree. third, so does she"), None);
}

#[test]
fn out_of_order_markers_are_not_a_list() {
    assert_eq!(format("first we leave third we eat"), None);
}

#[test]
fn already_a_list_is_unchanged() {
    assert!(is_list("1. Apples\n2. Pears"));
    assert!(is_list("- Apples\n- Pears"));
    assert!(!is_list("1. Apples"));
    assert!(!is_list("Apples and pears"));
    assert_eq!(format("1. Apples\n2. Pears"), None);
}

#[test]
fn items_lose_a_trailing_period_but_keep_a_question_mark() {
    assert_eq!(format("first, is it ready? second, ship it.").as_deref(), Some("1. Is it ready?\n2. Ship it"));
}

#[test]
fn markers_mid_clause_do_not_count() {
    // "first" after "the" is not a clause start even though "second" is.
    assert_eq!(format("take the first exit. second, turn left"), None);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p app-cleanup lists`
Expected: compile error, `lists` module not found.

- [ ] **Step 3: Implement `lists.rs`**

Signatures from the Interfaces block. Approach the signatures do not fix:

- Tokenise the text into (word, byte range) while remembering, for each word, whether it sits at a clause start: it is the first word, or the previous non-space character is one of `. ! ? ; : ,` or `\n`, or the previous word is "and"/"then" and *that* word sits at a clause start.
- Marker tables (lower-case): ordinals `first…tenth` plus `firstly, secondly, thirdly`, with `last, lastly, finally` accepted as "next in sequence" only once a list is open; `number <one…ten>` and `point <one…ten>` as two-word markers; bullets `bullet point`, `bullet`, `new bullet`, `next bullet`, `next point`, `new point`; digits `\d+.` where the period is attached and a space follows. Digit markers are the one family that counts at any position, not only at a clause start (ASR writes "1. check the logs 2. restart" without punctuation before the 2).
- Scan clause-start words in order. Collect candidate markers of one family only (the family of the first marker found). For numbered families require values 1, 2, 3… in order; stop collecting at the first break. Fewer than two markers → `None`.
- Lead-in = text before the first marker, trimmed. Item i = text between marker i (plus a following `,` or `:` and spaces) and marker i+1, trimmed of surrounding spaces and a trailing `,` or `.` (keep `?` and `!`). Capitalise the first alphabetic char of each item (reuse the pattern in `rules::capitalize_sentences`).
- `is_list` first: if true, return `None`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p app-cleanup lists`
Expected: all tests in `lists::tests` PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/cleanup/src/lists.rs crates/cleanup/src/lib.rs
git commit -m "Detect spoken enumerations and format them as lists"
```

---

### Task 2: Local rules apply the detector

**Files:**
- Modify: `crates/cleanup/src/rules.rs` (`RuleOptions`, `apply`, new `starts_own_line`)

**Interfaces:**
- Consumes: `lists::format`, `lists::is_list` (Task 1).
- Produces:
  - `RuleOptions.lists: bool` (default `true`).
  - `rules::apply` returns list text when `opt.lists` and the detector matches; `trailing_period` is not applied to list text.
  - `pub fn starts_own_line(before: Option<&str>, text: &str) -> bool`: true when `text` is a list (`lists::is_list`) and `before` is non-empty and does not end with `\n` or `\r` (after trimming spaces and tabs). Mirrors `leading_space`.

- [ ] **Step 1: Write the failing tests** in `rules.rs` `mod tests`:

```rust
#[test]
fn spoken_lists_become_lines() {
    let o = RuleOptions::default();
    assert_eq!(
        apply("first check the logs comma second restart the service comma third tell the team", o),
        "1. Check the logs\n2. Restart the service\n3. Tell the team"
    );
    assert_eq!(apply("three things. first, the budget. second, hiring", o), "Three things.\n1. The budget\n2. Hiring");
    assert_eq!(apply("bullet point apples bullet point pears", o), "- Apples\n- Pears");
    // no period appended to the last item
    assert!(!apply("first apples second pears", o).ends_with('.'));
}

#[test]
fn lists_can_be_switched_off() {
    let o = RuleOptions { lists: false, ..Default::default() };
    assert_eq!(apply("first apples second pears", o), "First apples second pears.");
}

#[test]
fn prose_with_ordinals_keeps_the_old_rules() {
    let o = RuleOptions::default();
    assert_eq!(apply("the first time I saw it I had second thoughts", o), "The first time I saw it I had second thoughts.");
}

#[test]
fn a_list_starts_on_its_own_line() {
    assert!(starts_own_line(Some("Notes:"), "1. Apples\n2. Pears"));
    assert!(starts_own_line(Some("Notes: "), "- Apples\n- Pears"));
    assert!(!starts_own_line(Some("Notes:\n"), "1. Apples\n2. Pears"));
    assert!(!starts_own_line(None, "1. Apples\n2. Pears"));
    assert!(!starts_own_line(Some("Notes:"), "Apples and pears."));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p app-cleanup rules`
Expected: compile errors for `lists` field and `starts_own_line`.

- [ ] **Step 3: Implement**

- Add `pub lists: bool` to `RuleOptions` with doc comment "Turn spoken enumerations (first…, second…; bullet point…) into list lines." and `lists: true` in `Default`.
- In `apply`, after `spoken_punctuation` (and filler removal), when `opt.lists`, replace `t` with `lists::format(&t)` if it returns `Some`, and remember `is_list = true`. Keep the existing `tidy_with` / `capitalize_sentences` steps (they are line-aware). Skip the `trailing_period` push when `is_list`.
- Add `starts_own_line` next to `leading_space`.
- Note: the `continues_sentence` lower-casing of the first char must not touch a list; guard it with `!is_list`.

- [ ] **Step 4: Run the whole crate's tests**

Run: `cargo test -p app-cleanup`
Expected: PASS, including the pre-existing `spoken_commands`, `off_mode_rules`, `continuation` tests.

- [ ] **Step 5: Commit**

```bash
git add crates/cleanup/src/rules.rs
git commit -m "Apply spoken-list formatting in the local rules"
```

---

### Task 3: Prompt teaches the AI, guard admits lists

**Files:**
- Modify: `crates/cleanup/src/prompt.rs:20-35` (`CLEANUP_SYSTEM`)
- Modify: `crates/cleanup/src/guard.rs` (tests only unless a test fails)

**Interfaces:**
- Produces: unchanged function signatures; `CLEANUP_SYSTEM` contains the exact copy below.

- [ ] **Step 1: Write the failing tests**

In `prompt.rs` `mod tests`:

```rust
#[test]
fn the_system_prompt_allows_lists() {
    assert!(CLEANUP_SYSTEM.contains("Lists:"));
    assert!(CLEANUP_SYSTEM.contains("no markdown except the list markers above"));
    assert!(!CLEANUP_SYSTEM.contains("no markdown, no preface"));
}
```

In `guard.rs` `mod tests`:

```rust
#[test]
fn a_list_reply_passes_the_guard() {
    let raw = "first check the logs second restart the service third tell the team";
    assert!(check_cleanup(raw, "1. Check the logs\n2. Restart the service\n3. Tell the team", Light, &[]).is_ok());
    let raw = "bullet point apples bullet point pears bullet point milk";
    assert!(check_cleanup(raw, "- Apples\n- Pears\n- Milk", Light, &[]).is_ok());
    // identifiers and dictionary terms inside items still have to survive
    let raw = "first rename user_count second ask Lindqvist";
    assert!(check_cleanup(raw, "1. Rename user_count\n2. Ask Lindqvist", Light, &["Lindqvist".into()]).is_ok());
    assert_eq!(
        check_cleanup(raw, "1. Rename user count\n2. Ask Lindqvist", Light, &["Lindqvist".into()]),
        Err(Rejection::DroppedTerm("user_count".into()))
    );
}

#[test]
fn bold_inside_a_list_is_still_a_reply() {
    assert_eq!(check_cleanup("first apples second pears third milk", "- **Apples**\n- Pears\n- Milk", Light, &[]), Err(Rejection::Reply));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p app-cleanup the_system_prompt_allows_lists`
Expected: FAIL on the `contains("Lists:")` assertion. The guard tests are expected to PASS already; if one fails, fix `check_cleanup` minimally (the ratio and precision rules treat digits as known words, so no change is anticipated).

- [ ] **Step 3: Edit `CLEANUP_SYSTEM`**

Replace the line `- Plain text only: no quotes around the result, no markdown, no preface or explanation.` with these two lines, verbatim:

```
- Lists: when the speaker enumerates items with sequence words (first/second/third, one/two/three, number one, bullet point, next point), write each item on its own line: numbered items as "1. ", "2. ", bullets as "- ". Drop the spoken marker and keep the speaker's words. A sentence spoken before the first item stays as its own line. Do not make a list when the sequence word is part of ordinary prose.
- Plain text only: no quotes around the result, no markdown except the list markers above, no preface or explanation.
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p app-cleanup`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/cleanup/src/prompt.rs crates/cleanup/src/guard.rs
git commit -m "Let the cleanup model write spoken enumerations as lists"
```

---

### Task 4: The `format_lists` setting and its toggle

**Files:**
- Modify: `crates/core/src/config.rs:376-377` (field next to `cleanup_level`), `:427` (default), test `settings_roundtrip_and_missing_fields` at `:631`
- Modify: `apps/desktop/src/lib/api.ts:87-88` (Settings type)
- Modify: `apps/desktop/src/dev/defaults.json:119-120`
- Modify: `apps/desktop/src/sections/Dictation.tsx:146` (after the Standing instruction row, inside the Cleanup card)
- Modify: `docs/setup-guide.md:171`

**Interfaces:**
- Produces: `Settings.format_lists: bool` (Rust), `format_lists: boolean` (TS). Task 5 reads `s.format_lists`.

- [ ] **Step 1: Write the failing test** in `config.rs`, extend `settings_roundtrip_and_missing_fields`:

```rust
assert!(Settings::default().format_lists, "spoken lists are on by default");
let old: Settings = serde_json::from_str(r#"{"version":1}"#).unwrap();
assert!(old.format_lists, "a settings file from before the field loads with it on");
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p app-core settings_roundtrip_and_missing_fields`
Expected: compile error, no field `format_lists`.

- [ ] **Step 3: Add the field**

- `config.rs`: `/// Turn "first…, second…" and "bullet point…" into a list, one item per line.` then `pub format_lists: bool,` after `standing_instruction`; `format_lists: true,` in `Default`. The struct already has `#[serde(default)]`, so missing fields take the `Default` value.
- `api.ts`: `format_lists: boolean;` after `standing_instruction: string;`.
- `defaults.json`: `"format_lists": true,` after `"standing_instruction": "",`.
- `Dictation.tsx`: after the Standing instruction `<div className="row">`, still inside the Cleanup card, add:

```tsx
<Row title="Spoken lists" desc="“First…, second…, third…” and “bullet point…” become a list, one item per line. Never in terminals.">
  <Toggle label="Spoken lists" on={s.format_lists} onChange={(v) => update((x) => (x.format_lists = v))} />
</Row>
```

- `docs/setup-guide.md:171`: change `Say punctuation if you like ("comma", "period", "new paragraph")` to `Say punctuation if you like ("comma", "period", "new paragraph"), and say "first…, second…, third…" or "bullet point…" to get a list`.

- [ ] **Step 4: Run the checks**

Run: `cargo test -p app-core` → PASS.
Run (in `apps/desktop`): `pnpm typecheck && pnpm test` → no type errors, vitest PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/config.rs apps/desktop/src/lib/api.ts apps/desktop/src/dev/defaults.json apps/desktop/src/sections/Dictation.tsx docs/setup-guide.md
git commit -m "Add a setting that turns spoken enumerations into lists"
```

---

### Task 5: Pipeline wiring

**Files:**
- Modify: `apps/desktop/src-tauri/src/pipeline.rs:283-292` (`rule_opts`), `:390-398` (after `check_cleanup`), `:648-650` (before `leading_space`), `:663` (`undo_text`), tests at `:1152+`

**Interfaces:**
- Consumes: `RuleOptions.lists`, `rules::apply`, `rules::starts_own_line`, `app_cleanup::lists::format` (Tasks 1–2); `Settings.format_lists` (Task 4).

- [ ] **Step 1: Write the failing tests** in `pipeline.rs` `mod tests`:

```rust
#[test]
fn list_formatting_follows_the_setting_and_skips_terminals() {
    let mut s = Settings::default();
    assert!(rule_opts(&s, "slack.exe", None, false).lists);
    assert!(!rule_opts(&s, "WindowsTerminal.exe", None, false).lists);
    assert!(!rule_opts(&s, "com.apple.terminal", None, false).lists);
    s.format_lists = false;
    assert!(!rule_opts(&s, "slack.exe", None, false).lists);
}

#[test]
fn accepted_ai_text_still_gets_list_structure() {
    let o = rule_opts(&Settings::default(), "slack.exe", None, false);
    assert_eq!(finish_cleaned("First, check the logs. Second, restart the service.", o), "1. Check the logs\n2. Restart the service");
    assert_eq!(finish_cleaned("1. Check the logs\n2. Restart the service", o), "1. Check the logs\n2. Restart the service");
    assert_eq!(finish_cleaned("Let's meet at three.", o), "Let's meet at three.");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (in `apps/desktop/src-tauri`): `cargo test list_formatting_follows_the_setting_and_skips_terminals accepted_ai_text_still_gets_list_structure`
Expected: compile errors (`lists` field unset in `rule_opts`'s struct literal is a compile error; `finish_cleaned` undefined).

- [ ] **Step 3: Implement**

- `rule_opts`: remove the `let _ = s;` line and set `lists: s.format_lists && !terminal` in the `RuleOptions` literal.
- Add `fn finish_cleaned(text: &str, opt: RuleOptions) -> String`: trims `text`; when `opt.lists`, returns `app_cleanup::lists::format(&trimmed).unwrap_or(trimmed)`; otherwise returns the trimmed text. (The detector is idempotent through `is_list`, so an AI list passes untouched.)
- In `cleanup`, the `Ok(())` arm becomes `CleanupResult { text: finish_cleaned(&parsed.text, rule_opts(s, app_name, before, false)), raw_used: false, reason: None }`.
- Before the `leading_space` check in the dictation path, add: if `rules::starts_own_line(before, &final_text)` then `final_text.insert(0, '\n')`; `leading_space` is then skipped (a newline already separates). Keep `else if rules::leading_space(...)` for the existing behaviour.
- `undo_text` stays `rules::apply(&raw, RuleOptions { remove_fillers: false, ..Default::default() })` but with `lists: false` added to the literal, so undo returns the words as heard, as the doc comment promises.

- [ ] **Step 4: Run the tests**

Run (in `apps/desktop/src-tauri`): `cargo test`
Expected: PASS, including the pre-existing pipeline tests.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/pipeline.rs
git commit -m "Format spoken lists on every cleanup path and start them on a new line"
```

---

### Task 6: End-to-end check

**Files:** none modified.

- [ ] **Step 1: Build and run the app** (`pnpm tauri dev` in `apps/desktop`).
- [ ] **Step 2: In a plain text editor, with cleanup Off,** dictate "first check the logs, second restart the service, third tell the team". Expected: three numbered lines.
- [ ] **Step 3: Same with cleanup Light and Medium.** Expected: a numbered list; History shows the cleaned text as the list.
- [ ] **Step 4: Stop Ollama (or set cleanup wait to 1 ms in Advanced) and dictate the same.** Expected: pill reports the fallback, the pasted text is still a list.
- [ ] **Step 5: Dictate "the first time I saw it I had second thoughts".** Expected: one sentence, no list.
- [ ] **Step 6: Type "Notes:" in the editor, leave the caret after the colon, dictate "bullet point apples bullet point pears".** Expected: the list starts on the line below "Notes:".
- [ ] **Step 7: In a terminal, dictate "first ls second pwd".** Expected: no list, text as heard.
- [ ] **Step 8: Switch "Spoken lists" off in Settings → Shortcuts & cleanup and repeat Step 2.** Expected: a flat sentence.
- [ ] **Step 9: Run the full suite** from the repo root: `cargo test --workspace` and, in `apps/desktop`, `pnpm typecheck && pnpm test`. Expected: all PASS.
