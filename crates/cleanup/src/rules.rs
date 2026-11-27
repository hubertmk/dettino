//! Local rules: the last link of every cleanup chain, and all of cleanup "Off".

use crate::lists;
use crate::words::replace_word_ci;

#[derive(Debug, Clone, Copy)]
pub struct RuleOptions {
    /// Remove um/uh (used when the LLM failed at Light/Medium, not at Off).
    pub remove_fillers: bool,
    /// Capitalize the first letter unless the text continues a sentence.
    pub capitalize: bool,
    /// Append a period if the text ends without punctuation (off in terminals).
    pub trailing_period: bool,
    /// Text before the caret ends mid-sentence.
    pub continues_sentence: bool,
    /// Turn spoken enumerations (first…, second…; bullet point…) into list lines.
    pub lists: bool,
}

impl Default for RuleOptions {
    fn default() -> Self {
        Self { remove_fillers: false, capitalize: true, trailing_period: true, continues_sentence: false, lists: true }
    }
}

const SPOKEN: &[(&str, &str)] = &[
    ("new paragraph", "\n\n"),
    ("new line", "\n"),
    ("question mark", "?"),
    ("exclamation mark", "!"),
    ("exclamation point", "!"),
    ("full stop", "."),
    ("comma", ","),
    ("colon", ":"),
    ("semicolon", ";"),
];

/// Replace spoken punctuation commands with symbols ("comma" → ","). ASR often
/// adds its own punctuation around the command word (", comma,"); that is dropped.
pub fn spoken_punctuation(text: &str) -> String {
    let mut marked = text.to_string();
    for (i, (spoken, _)) in SPOKEN.iter().enumerate() {
        marked = replace_word_ci(&marked, spoken, &format!("\u{1}{i}\u{2}"));
    }
    if !marked.contains('\u{1}') {
        return text.to_string();
    }
    let mut res = String::with_capacity(marked.len());
    let mut chars = marked.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1}' {
            res.push(c);
            continue;
        }
        let mut idx = String::new();
        for d in chars.by_ref() {
            if d == '\u{2}' {
                break;
            }
            idx.push(d);
        }
        let sym = SPOKEN[idx.parse::<usize>().unwrap_or(0)].1;
        // A line break keeps the sentence's own period; a symbol replaces stray ones.
        let strip: &[char] = if sym.contains('\n') { &[' ', ','] } else { &[' ', ',', '.'] };
        while res.ends_with(strip) {
            res.pop();
        }
        res.push_str(sym);
        while matches!(chars.peek(), Some(',') | Some('.')) {
            chars.next();
        }
        if sym.contains('\n') {
            while chars.peek() == Some(&' ') {
                chars.next();
            }
        } else if chars.peek().map(|n| n.is_alphanumeric()).unwrap_or(false) {
            res.push(' ');
        }
    }
    res
}

fn remove_fillers(text: &str) -> String {
    let mut out = text.to_string();
    for f in ["um", "uh", "erm", "uhm"] {
        out = replace_word_ci(&out, f, "");
    }
    tidy(&out)
}

/// Collapse doubled spaces and fix spacing before punctuation.
pub fn tidy(text: &str) -> String {
    tidy_with(text, true)
}

/// `punctuation`: also pull `, . ? ! : ;` onto the word before them. Off for text that
/// is not prose (terminals: `git add .` and `cd ..` must stay as spoken).
fn tidy_with(text: &str, punctuation: bool) -> String {
    let mut s = String::with_capacity(text.len());
    for line in text.split('\n') {
        let mut l = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if punctuation {
            for p in [" ,", " .", " ?", " !", " :", " ;"] {
                l = l.replace(p, &p[1..]);
            }
        }
        while l.contains(",,") {
            l = l.replace(",,", ",");
        }
        l = l.trim_start_matches([',', ' ']).to_string();
        s.push_str(&l);
        s.push('\n');
    }
    s.pop();
    s.trim().to_string()
}

/// Capitalize after sentence ends. A period only ends a sentence when whitespace (or
/// the end) follows it: `example.com`, `node.js` and `e.g.` keep their case.
fn capitalize_sentences(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut cap_next = true;
    for (i, &c) in chars.iter().enumerate() {
        if cap_next && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            cap_next = false;
            continue;
        }
        out.push(c);
        let break_follows = chars.get(i + 1).map(|n| n.is_whitespace()).unwrap_or(true);
        if c == '\n' || (matches!(c, '.' | '?' | '!') && break_follows) {
            cap_next = true;
        } else if !c.is_whitespace() && c != '"' {
            cap_next = false;
        }
    }
    out
}

pub fn apply(text: &str, opt: RuleOptions) -> String {
    let mut t = spoken_punctuation(text);
    if opt.remove_fillers {
        t = remove_fillers(&t);
    }
    let mut is_list = false;
    if opt.lists {
        if let Some(list) = lists::format(&t) {
            t = list;
            is_list = true;
        }
    }
    t = tidy_with(&t, opt.capitalize || opt.trailing_period);
    if t.is_empty() {
        return t;
    }
    if opt.capitalize {
        t = capitalize_sentences(&t);
        if opt.continues_sentence && !is_list {
            // keep the first word lowercase unless it is "I" or looks like a name/acronym in the source
            let mut cs = t.chars();
            if let Some(first) = cs.next() {
                let rest: String = cs.collect();
                let original_first = text.trim_start().chars().next();
                if original_first.map(|c| c.is_lowercase()).unwrap_or(false) && !t.starts_with("I ") {
                    t = first.to_lowercase().collect::<String>() + &rest;
                }
            }
        }
    }
    if opt.trailing_period && !is_list && !t.ends_with(['.', '?', '!', ':', ';', '\n', '"', ')']) {
        t.push('.');
    }
    t
}

/// A list pasted after text on the same line starts on its own line.
pub fn starts_own_line(before: Option<&str>, text: &str) -> bool {
    let Some(b) = before else { return false };
    let b = b.trim_end_matches([' ', '\t']);
    !b.is_empty() && !b.ends_with(['\n', '\r']) && lists::is_list(text)
}

/// Does text before the caret end mid-sentence (so the dictation continues it)?
/// A line break before the caret starts a new line, whatever came before it.
pub fn continues_sentence(before: Option<&str>) -> bool {
    let Some(b) = before else { return false };
    let b = b.trim_end_matches([' ', '\t']);
    if b.ends_with(['\n', '\r']) {
        return false;
    }
    let b = b.trim_end();
    !b.is_empty() && !b.ends_with(['.', '?', '!', ':'])
}

/// Spacing between existing text and the inserted text.
pub fn leading_space(before: Option<&str>, text: &str) -> bool {
    match before {
        Some(b) if !b.is_empty() => {
            let last = b.chars().last().unwrap();
            !last.is_whitespace() && !matches!(last, '(' | '[' | '"' | '\'' | '/') && !text.starts_with(['.', ',', '?', '!'])
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spoken_commands() {
        assert_eq!(spoken_punctuation("hello comma world period"), "hello, world period");
        assert_eq!(spoken_punctuation("hello, comma, world full stop"), "hello, world.");
        assert_eq!(spoken_punctuation("first line new line second"), "first line\nsecond");
        assert_eq!(spoken_punctuation("is it question mark"), "is it?");
        // the sentence's own period survives a line break command
        assert_eq!(spoken_punctuation("Done. New paragraph. Next one."), "Done.\n\nNext one.");
    }

    #[test]
    fn dots_inside_words_are_not_sentence_ends() {
        let o = RuleOptions::default();
        assert_eq!(apply("go to example.com now", o), "Go to example.com now.");
        assert_eq!(apply("we use node.js here", o), "We use node.js here.");
        assert_eq!(apply("done. next thing", o), "Done. Next thing.");
    }

    #[test]
    fn terminal_text_keeps_its_spacing() {
        let o = RuleOptions { capitalize: false, trailing_period: false, ..Default::default() };
        assert_eq!(apply("git add .", o), "git add .");
        assert_eq!(apply("cd ..", o), "cd ..");
    }

    #[test]
    fn a_line_break_before_the_caret_starts_a_new_line() {
        assert!(!continues_sentence(Some("Dear Bob,\n\n")));
        assert!(!continues_sentence(Some("Dear Bob,\n")));
        assert!(continues_sentence(Some("Dear Bob, ")));
    }

    #[test]
    fn off_mode_rules() {
        let o = RuleOptions::default();
        assert_eq!(apply("hello world", o), "Hello world.");
        assert_eq!(apply("hello world. how are you", o), "Hello world. How are you.");
        assert_eq!(apply("Already done?", o), "Already done?");
    }

    #[test]
    fn terminal_rules_keep_command_as_is() {
        let o = RuleOptions { capitalize: false, trailing_period: false, ..Default::default() };
        assert_eq!(apply("git status", o), "git status");
    }

    #[test]
    fn filler_removal_fallback() {
        let o = RuleOptions { remove_fillers: true, ..Default::default() };
        assert_eq!(apply("um so uh we ship", o), "So we ship.");
    }

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
        assert!(!apply("first apples, second pears", o).ends_with('.'));
    }

    #[test]
    fn lists_can_be_switched_off() {
        let o = RuleOptions { lists: false, ..Default::default() };
        // The same input makes a list with `lists: true` (see spoken_lists_become_lines).
        assert_eq!(apply("first apples, second pears", o), "First apples, second pears.");
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

    #[test]
    fn continuation() {
        assert!(continues_sentence(Some("Thanks for")));
        assert!(!continues_sentence(Some("Thanks.")));
        let o = RuleOptions { continues_sentence: true, ..Default::default() };
        assert_eq!(apply("your help", o), "your help.");
        assert!(leading_space(Some("Thanks for"), "your help"));
        assert!(!leading_space(Some("Thanks for "), "your help"));
    }
}
