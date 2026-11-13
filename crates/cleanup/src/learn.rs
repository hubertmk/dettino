//! Auto-learn: compare what was pasted with what the field contains a few
//! seconds later and suggest dictionary entries for corrected names and terms.
//! Suggestions are never added without the user accepting them.

use crate::words::{lower_tokens, tokens, STOPWORDS};
use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub from: String,
    pub to: String,
}

const COMMON_ACRONYMS: &[&str] = &["PDF", "CEO", "USA", "OK", "FAQ", "ASAP", "AI", "API", "URL", "HTML", "CSS", "UK", "EU"];

fn looks_like_term(t: &str) -> bool {
    let first_upper = t.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
    let inner_upper = t.chars().skip(1).any(|c| c.is_uppercase());
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    let email = t.contains('@');
    (first_upper || inner_upper || has_digit || email)
        && !COMMON_ACRONYMS.contains(&t)
        && !STOPWORDS.contains(&t.to_lowercase().as_str())
        && t.chars().count() >= 2
}

/// Locate the pasted text inside the field and return the edited version of it.
pub fn edited_span(pasted: &str, field: &str) -> Option<String> {
    if field.contains(pasted) {
        return Some(pasted.to_string());
    }
    // Find the region of the field most similar to the pasted text using word anchors.
    let pw = tokens(pasted);
    if pw.len() < 2 {
        return None;
    }
    let first = pw.first()?;
    let last = pw.last()?;
    let start = field.find(first)?;
    let end_rel = field[start..].rfind(last)?;
    let mut end = start + end_rel + last.len();
    // include trailing punctuation the pasted text also ended with
    let suffix: String = pasted
        .chars()
        .rev()
        .take_while(|c| !c.is_alphanumeric() && !c.is_whitespace())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if !suffix.is_empty() && field[end..].starts_with(&suffix) {
        end += suffix.len();
    }
    let span = &field[start..end];
    // Sanity: the span must be roughly the same size as what we pasted.
    let ratio = span.len() as f64 / pasted.len().max(1) as f64;
    (0.5..2.0).contains(&ratio).then(|| span.to_string())
}

/// Word-level substitutions where the replacement looks like a name or term.
pub fn suggest(pasted: &str, edited: &str) -> Vec<Suggestion> {
    let a = tokens(pasted);
    let b = tokens(edited);
    let diff = TextDiff::from_slices(&a, &b);
    let mut out = Vec::new();
    let mut deleted: Vec<&str> = Vec::new();
    let mut inserted: Vec<&str> = Vec::new();
    let flush = |deleted: &mut Vec<&str>, inserted: &mut Vec<&str>, out: &mut Vec<Suggestion>| {
        if !deleted.is_empty() && !inserted.is_empty() && inserted.len() <= 3 && deleted.len() <= 4 {
            let from = deleted.join(" ");
            let to = inserted.join(" ");
            let (fl, tl) = (from.to_lowercase(), to.to_lowercase());
            let joined = lower_tokens(&from).concat();
            let similar = strsim::jaro_winkler(&fl, &tl) > 0.7
                || strsim::jaro_winkler(&joined, &tl.replace(' ', "")) > 0.6
                || joined == lower_tokens(&to).concat();
            if similar && inserted.iter().all(|t| looks_like_term(t)) && from.to_lowercase() != to.to_lowercase() {
                out.push(Suggestion { from, to });
            }
        }
        deleted.clear();
        inserted.clear();
    };
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => deleted.push(change.value()),
            ChangeTag::Insert => inserted.push(change.value()),
            ChangeTag::Equal => flush(&mut deleted, &mut inserted, &mut out),
        }
    }
    flush(&mut deleted, &mut inserted, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_correction_is_suggested() {
        let s = suggest("Please ask Lindquist about the release.", "Please ask Lindqvist about the release.");
        assert_eq!(s, vec![Suggestion { from: "Lindquist".into(), to: "Lindqvist".into() }]);
    }

    #[test]
    fn split_word_correction() {
        let s = suggest("We use tow rie for the app.", "We use Tauri for the app.");
        assert_eq!(s, vec![Suggestion { from: "tow rie".into(), to: "Tauri".into() }]);
    }

    #[test]
    fn ordinary_edits_are_ignored() {
        assert!(suggest("I think we should go.", "I believe we should go.").is_empty());
        assert!(suggest("Send the pdf.", "Send the PDF.").is_empty());
    }

    #[test]
    fn locating_span() {
        let field = "Hi team,\nPlease ask Lindqvist about the release.\nThanks";
        let span = edited_span("Please ask Lindquist about the release.", field).unwrap();
        assert_eq!(span, "Please ask Lindqvist about the release.");
    }
}
