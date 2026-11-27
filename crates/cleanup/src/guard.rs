//! Guardrails against over-editing and hallucination. Any rejection means the
//! raw transcript (plus local rules) is pasted instead, and the pill says so.

use crate::lists;
use crate::words::{content_words, identifiers, is_filler, lower_tokens, replace_word_ci};
use app_core::CleanupLevel;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    Empty,
    TooShort { pct: u32 },
    TooLong { pct: u32 },
    AddedContent { pct: u32 },
    DroppedTerm(String),
    /// The model replied to the transcript (a preface, a quoted or formatted answer).
    Reply,
}

impl Rejection {
    /// Text for History and the pill's "Why?".
    pub fn explain(&self) -> String {
        match self {
            Self::Empty => "The cleanup returned nothing.".into(),
            Self::TooShort { pct } => format!("The cleanup removed too much: the result was {pct} % of your words."),
            Self::TooLong { pct } => format!("The cleanup made the text {pct} % as long as what you said."),
            Self::AddedContent { pct } => {
                format!("Only {pct} % of the cleaned words came from what you said; it looked like added content.")
            }
            Self::DroppedTerm(t) => format!("The cleanup changed \"{t}\", a dictionary term or code name."),
            Self::Reply => "The cleanup answered instead of cleaning.".into(),
        }
    }
}

/// Whole-word, case-insensitive containment.
fn contains_word(text: &str, term: &str) -> bool {
    !term.is_empty() && replace_word_ci(text, term, "") != text
}

/// A preface, a quoted answer or markdown: the model talked instead of cleaning.
/// Anything the transcript itself starts with is not held against the output.
fn looks_like_a_reply(out: &str, raw: &str) -> bool {
    let lower = out.to_lowercase();
    let raw_l = raw.trim().to_lowercase();
    let prefaces = ["sure", "certainly", "here's", "here is", "here you go", "okay, here", "of course"];
    let preface = prefaces.iter().any(|p| lower.starts_with(p) && !raw_l.starts_with(p));
    let quoted = out.len() > 2 && out.starts_with('"') && out.ends_with('"') && !raw_l.starts_with('"');
    let markdown = out.starts_with("```") || out.starts_with("# ") || (out.contains("**") && !raw.contains("**"));
    preface || quoted || markdown
}

fn spoken_word_count(text: &str) -> usize {
    lower_tokens(text).iter().filter(|t| !is_filler(t)).count()
}

/// Validate a cleanup result against the raw transcript.
pub fn check_cleanup(raw: &str, cleaned: &str, level: CleanupLevel, dictionary: &[String]) -> Result<(), Rejection> {
    let cleaned_t = cleaned.trim();
    if cleaned_t.is_empty() {
        // Only acceptable when the raw text was nothing but fillers.
        return if spoken_word_count(raw) == 0 { Ok(()) } else { Err(Rejection::Empty) };
    }
    if looks_like_a_reply(cleaned_t, raw) {
        return Err(Rejection::Reply);
    }
    // Dictionary terms and identifiers present in the input must survive (whole words:
    // "Cal" inside "basically" is not the term).
    for term in dictionary {
        if contains_word(raw, term) && !contains_word(cleaned_t, term) {
            return Err(Rejection::DroppedTerm(term.clone()));
        }
    }
    for id in identifiers(raw) {
        if !cleaned_t.contains(&id) {
            return Err(Rejection::DroppedTerm(id));
        }
    }
    let (lo, hi, min_precision) = match level {
        CleanupLevel::Medium => (0.35, 1.6, 0.55),
        _ => (0.45, 1.35, 0.7),
    };
    // Spoken list markers ("bullet point", "first") are structure, not words: a list
    // reply is measured against the transcript with those markers folded into lines.
    let measured = if lists::is_list(cleaned_t) { lists::format(raw) } else { None };
    let raw = measured.as_deref().unwrap_or(raw);
    let n_in = spoken_word_count(raw);
    let n_out = spoken_word_count(cleaned_t);
    if n_in >= 5 {
        let ratio = n_out as f64 / n_in as f64;
        let pct = (ratio * 100.0).round() as u32;
        if ratio < lo {
            return Err(Rejection::TooShort { pct });
        }
        if ratio > hi {
            return Err(Rejection::TooLong { pct });
        }
    } else if n_out > n_in + 6 {
        return Err(Rejection::TooLong { pct: ((n_out as f64 / n_in.max(1) as f64) * 100.0) as u32 });
    }

    // Precision: share of output content words that appear in the input. A dictionary
    // term the model spelled correctly counts as coming from the input: fixing a
    // misheard name is what the dictionary is for.
    let cin = content_words(raw);
    let cout = content_words(cleaned_t);
    let dict_words: HashSet<String> = dictionary.iter().flat_map(|t| content_words(t)).collect();
    let known = |w: &String| cin.contains(w) || dict_words.contains(w) || w.chars().all(|c| c.is_ascii_digit());
    let hits = cout.iter().filter(|w| known(w)).count();
    if cout.len() >= 3 {
        let precision = hits as f64 / cout.len() as f64;
        if precision < min_precision {
            return Err(Rejection::AddedContent { pct: (precision * 100.0).round() as u32 });
        }
    } else if hits < cout.len() {
        // Too short for a ratio: every content word has to come from what was said.
        return Err(Rejection::AddedContent { pct: (hits * 100 / cout.len().max(1)) as u32 });
    }

    Ok(())
}

pub fn check_command(selection: &str, replacement: &str) -> Result<(), Rejection> {
    if replacement.trim().is_empty() {
        return Err(Rejection::Empty);
    }
    let limit = selection.chars().count() * 4 + 400;
    if replacement.chars().count() > limit {
        return Err(Rejection::TooLong {
            pct: (replacement.len() * 100 / selection.len().max(1)) as u32,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use CleanupLevel::*;

    #[test]
    fn self_correction_is_allowed() {
        assert!(check_cleanup("um so let's meet at two actually three", "Let's meet at three.", Light, &[]).is_ok());
    }

    #[test]
    fn filler_removal_is_allowed() {
        assert!(check_cleanup(
            "uh I think we should uh probably ship the the release tomorrow",
            "I think we should probably ship the release tomorrow.",
            Light,
            &[]
        )
        .is_ok());
    }

    #[test]
    fn answering_the_question_is_rejected() {
        let r = check_cleanup(
            "what is the capital of france",
            "The capital of France is Paris, a city known for the Eiffel Tower and its museums.",
            Light,
            &[],
        );
        assert!(matches!(r, Err(Rejection::TooLong { .. }) | Err(Rejection::AddedContent { .. })), "{r:?}");
    }

    #[test]
    fn added_content_is_rejected() {
        let r = check_cleanup(
            "send the report to marketing by friday please",
            "Message: Send quarterly budget figures to finance by Monday.",
            Light,
            &[],
        );
        assert!(matches!(r, Err(Rejection::AddedContent { .. })), "{r:?}");
    }

    #[test]
    fn over_shortening_is_rejected() {
        let r = check_cleanup(
            "we need to talk about the budget and the hiring plan and the offsite next month",
            "Discuss budget.",
            Light,
            &[],
        );
        assert!(matches!(r, Err(Rejection::TooShort { .. })), "{r:?}");
    }

    #[test]
    fn medium_allows_more() {
        assert!(check_cleanup(
            "so basically the thing is that the build is like broken because of the cache i think",
            "The build is broken, probably because of the cache.",
            Medium,
            &[]
        )
        .is_ok());
    }

    #[test]
    fn dictionary_and_identifiers_must_survive() {
        let r = check_cleanup("ask Lindqvist about it", "Ask Lindquist about it.", Light, &["Lindqvist".into()]);
        assert_eq!(r, Err(Rejection::DroppedTerm("Lindqvist".into())));
        let r = check_cleanup("rename user_count to total", "Rename user count to total.", Light, &[]);
        assert_eq!(r, Err(Rejection::DroppedTerm("user_count".into())));
    }

    #[test]
    fn a_dictionary_correction_is_not_added_content() {
        let dict = vec!["Lindqvist".to_string()];
        assert!(check_cleanup("ask lindquist today", "Ask Lindqvist today.", Light, &dict).is_ok());
        assert!(check_cleanup("um ask lindquist about the budget today", "Ask Lindqvist about the budget today.", Light, &dict).is_ok());
    }

    #[test]
    fn a_term_inside_another_word_is_not_the_term() {
        let dict = vec!["Cal".to_string()];
        assert!(check_cleanup("so basically we need it by friday", "We need it by Friday.", Medium, &dict).is_ok());
    }

    #[test]
    fn short_answers_and_prefaces_are_rejected() {
        assert!(matches!(check_cleanup("thanks a lot guys", "You're welcome!", Light, &[]), Err(Rejection::AddedContent { .. })));
        assert_eq!(
            check_cleanup("send the report to marketing by friday", "Sure, here's the cleaned text: Send the report to marketing by Friday.", Light, &[]),
            Err(Rejection::Reply)
        );
        assert_eq!(check_cleanup("send the report by friday", "\"Send the report by Friday.\"", Light, &[]), Err(Rejection::Reply));
        assert_eq!(check_cleanup("send the report by friday", "**Send** the report by Friday.", Light, &[]), Err(Rejection::Reply));
        // a transcript that itself starts with "sure" is fine
        assert!(check_cleanup("sure we can do that tomorrow", "Sure, we can do that tomorrow.", Light, &[]).is_ok());
    }

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
        assert_eq!(
            check_cleanup("first apples second pears third milk", "- **Apples**\n- Pears\n- Milk", Light, &[]),
            Err(Rejection::Reply)
        );
    }

    #[test]
    fn only_fillers_may_become_empty() {
        assert!(check_cleanup("um uh", "", Light, &[]).is_ok());
        assert_eq!(check_cleanup("hello there", "", Light, &[]), Err(Rejection::Empty));
    }

    #[test]
    fn command_limits() {
        assert!(check_command("hello", "Hello, how are you?").is_ok());
        assert!(check_command("hi", "").is_err());
    }
}
