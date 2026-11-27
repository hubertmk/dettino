//! Spoken enumerations ("first…, second…, third…", "bullet point …") become list
//! lines. Deterministic, idempotent, and the last word on every output path.

const ORDINALS: &[&str] = &["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth"];
const ADVERBS: &[&str] = &["firstly", "secondly", "thirdly"];
const CARDINALS: &[&str] = &["one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten"];
/// Close an open ordinal list: "and finally, the offsite".
const CLOSERS: &[&str] = &["finally", "lastly", "last"];
const CONNECTIVES: &[&str] = &["and", "then"];
const CLAUSE_END: &[char] = &['.', '!', '?', ';', ':', ','];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    /// first, second, third; firstly, secondly
    Ordinal,
    /// number one, point two
    Number,
    /// "1." "2." as ASR writes them
    Digit,
    /// bullet point, next bullet, next point
    Bullet,
}

struct Tok<'a> {
    start: usize,
    end: usize,
    /// Lower-cased core without surrounding punctuation.
    word: String,
    /// Punctuation after the core ("," in "first,").
    trailing: &'a str,
    clause_start: bool,
    /// The token before is "and"/"then" sitting at a clause start.
    after_connective: bool,
}

struct Marker {
    family: Family,
    /// Byte where the item before this marker ends (the connective's start when there is one).
    boundary: usize,
    /// Byte where this marker's own item begins.
    item_start: usize,
}

fn push_tok<'a>(text: &'a str, toks: &mut Vec<Tok<'a>>, s: usize, e: usize, newline: bool) {
    let raw = &text[s..e];
    let core = raw.trim_matches(|c: char| !c.is_alphanumeric());
    let core_end = raw.rfind(core).map(|i| i + core.len()).unwrap_or(raw.len());
    let (clause_start, after_connective) = match toks.last() {
        None => (true, false),
        Some(p) => {
            let ends = newline || p.trailing.contains(CLAUSE_END);
            let connective = p.clause_start && CONNECTIVES.contains(&p.word.as_str());
            (ends || connective, connective && !ends)
        }
    };
    toks.push(Tok { start: s, end: e, word: core.to_lowercase(), trailing: &raw[core_end..], clause_start, after_connective });
}

fn tokenize(text: &str) -> Vec<Tok<'_>> {
    let mut toks: Vec<Tok> = Vec::new();
    let mut start: Option<usize> = None;
    let mut gap_has_newline = false;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                push_tok(text, &mut toks, s, i, gap_has_newline);
                gap_has_newline = false;
            }
            gap_has_newline |= c == '\n';
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        push_tok(text, &mut toks, s, text.len(), gap_has_newline);
    }
    toks
}

fn position(list: &[&str], w: &str) -> Option<usize> {
    list.iter().position(|x| *x == w).map(|i| i + 1)
}

/// A marker starting at token `i`: its family, value (0 for bullets) and token count.
fn marker_at(toks: &[Tok], i: usize, open: Option<(Family, usize)>) -> Option<(Family, usize, usize)> {
    let t = &toks[i];
    let w = t.word.as_str();
    let next = toks.get(i + 1).map(|n| n.word.as_str());
    let next_value = open.map(|(_, n)| n + 1).unwrap_or(1);
    // Two-word bullets are explicit commands and count anywhere.
    if matches!((w, next), ("bullet", Some("point")) | ("new" | "next", Some("bullet"))) {
        return Some((Family::Bullet, 0, 2));
    }
    if t.clause_start {
        if w == "bullet" || (matches!(w, "new" | "next") && next == Some("point")) {
            return Some((Family::Bullet, 0, if w == "bullet" { 1 } else { 2 }));
        }
        if let Some(v) = position(ORDINALS, w).or_else(|| position(ADVERBS, w)) {
            return Some((Family::Ordinal, v, 1));
        }
        if CLOSERS.contains(&w) && matches!(open, Some((Family::Ordinal, n)) if n >= 1) {
            return Some((Family::Ordinal, next_value, 1));
        }
        if matches!(w, "number" | "point") {
            if let Some(v) = next.and_then(|n| position(CARDINALS, n)) {
                return Some((Family::Number, v, 2));
            }
        }
    }
    if !w.is_empty() && w.chars().all(|c| c.is_ascii_digit()) && t.trailing.starts_with('.') && i + 1 < toks.len() {
        return Some((Family::Digit, w.parse().ok()?, 1));
    }
    None
}

/// Lines that already carry list markers: `- `, `* `, `• ` or `1. `.
pub fn is_list(text: &str) -> bool {
    let marked = text
        .lines()
        .map(str::trim_start)
        .filter(|l| {
            l.starts_with("- ") || l.starts_with("* ") || l.starts_with("• ") || {
                let digits = l.chars().take_while(|c| c.is_ascii_digit()).count();
                digits > 0 && l[digits..].starts_with(". ")
            }
        })
        .count();
    marked >= 2
}

fn capitalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut done = false;
    for c in s.chars() {
        if !done && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            done = true;
        } else {
            if !done && c.is_alphanumeric() {
                done = true;
            }
            out.push(c);
        }
    }
    out
}

/// Item text between two boundaries: no surrounding spaces, no leading `,`/`:`, no
/// trailing `,` or `.` (a `?` or `!` stays), first letter capitalised.
fn item(text: &str) -> String {
    let t = text.trim().trim_start_matches([',', ':']).trim();
    let t = t.strip_suffix(['.', ',']).unwrap_or(t).trim_end();
    capitalize(t)
}

/// `Some(list)` when `text` is a spoken enumeration with at least two items: a lead-in
/// line (if any) followed by one `1. ` / `- ` line per item. `None` otherwise, and
/// `None` for text that already is a list.
pub fn format(text: &str) -> Option<String> {
    if is_list(text) {
        return None;
    }
    let toks = tokenize(text);
    let mut family: Option<Family> = None;
    let mut markers: Vec<Marker> = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let open = family.map(|f| (f, markers.len()));
        if let Some((f, value, len)) = marker_at(&toks, i, open) {
            if family.is_none_or(|x| x == f) {
                if f != Family::Bullet && value != markers.len() + 1 {
                    break;
                }
                let t = &toks[i];
                let boundary = if t.after_connective { toks[i - 1].start } else { t.start };
                markers.push(Marker { family: f, boundary, item_start: toks[i + len - 1].end });
                family = Some(f);
                i += len;
                continue;
            }
        }
        i += 1;
    }
    if markers.len() < 2 {
        return None;
    }
    let numbered = markers[0].family != Family::Bullet;
    let mut lines: Vec<String> = Vec::new();
    // The lead-in is a sentence: keep its own punctuation.
    let lead = text[..markers[0].boundary].trim();
    if !lead.is_empty() {
        lines.push(capitalize(lead));
    }
    for (n, m) in markers.iter().enumerate() {
        let end = markers.get(n + 1).map(|next| next.boundary).unwrap_or(text.len());
        let body = item(&text[m.item_start..end]);
        if body.is_empty() {
            return None;
        }
        lines.push(if numbered { format!("{}. {body}", n + 1) } else { format!("- {body}") });
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            format("shopping. bullet apples, next bullet pears, next point milk").as_deref(),
            Some("Shopping.\n- Apples\n- Pears\n- Milk")
        );
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
}
