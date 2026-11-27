//! Turning a transcript into the text that gets pasted: the cleanup prompt,
//! guardrails against over-editing, local rules, dictionary, snippets,
//! auto-learn suggestions and on-screen proper-noun extraction.

pub mod guard;
pub mod learn;
pub mod lists;
pub mod nouns;
pub mod prompt;
pub mod rules;
pub mod snippets;
pub mod words;

pub use guard::{check_cleanup, check_command, Rejection};
pub use prompt::{build_cleanup_request, too_long_for_cleanup, build_command_request, parse_cleanup, parse_command, CleanupContext, Parsed};

use app_core::DictionaryEntry;

/// Apply "misspelling → term" replacements, whole words, case-insensitive.
pub fn apply_dictionary(text: &str, dict: &[DictionaryEntry]) -> String {
    let mut out = text.to_string();
    // An entry without a term would delete every match of its misspelling.
    for e in dict.iter().filter(|e| e.enabled && !e.term.trim().is_empty()) {
        if let Some(from) = e.replace_from.as_deref().filter(|f| !f.trim().is_empty()) {
            out = words::replace_word_ci(&out, from.trim(), e.term.trim());
        }
    }
    out
}

/// Dictionary terms most relevant to a transcript (for prompts with a size budget).
pub fn relevant_terms(transcript: &str, dict: &[DictionaryEntry], max: usize) -> Vec<String> {
    let toks: Vec<String> = words::tokens(transcript).into_iter().map(|t| t.to_lowercase()).collect();
    let mut scored: Vec<(f64, &str)> = dict
        .iter()
        .filter(|e| e.enabled && !e.term.trim().is_empty())
        .map(|e| {
            let term = e.term.to_lowercase();
            let best = toks
                .iter()
                .map(|t| strsim::jaro_winkler(t, &term))
                .chain(e.replace_from.iter().filter(|f| !f.trim().is_empty()).map(|f| {
                    if transcript.to_lowercase().contains(&f.trim().to_lowercase()) { 1.0 } else { 0.0 }
                }))
                .fold(0.0f64, f64::max);
            (best, e.term.as_str())
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<String> = scored.iter().filter(|(s, _)| *s >= 0.75).map(|(_, t)| t.to_string()).collect();
    // Fill remaining budget with other terms so small dictionaries are always sent whole.
    if dict.len() <= max {
        for (_, t) in &scored {
            if !out.iter().any(|o| o == t) {
                out.push(t.to_string());
            }
        }
    }
    out.truncate(max);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(term: &str, from: Option<&str>) -> DictionaryEntry {
        DictionaryEntry { term: term.into(), replace_from: from.map(Into::into), ..Default::default() }
    }

    #[test]
    fn dictionary_replacement() {
        let d = vec![entry("Lindqvist", Some("lind quest")), entry("Tauri", None)];
        assert_eq!(apply_dictionary("Hi Lind Quest, see tauri.", &d), "Hi Lindqvist, see tauri.");
    }

    #[test]
    fn empty_entries_are_ignored() {
        let d = vec![entry("", Some("hello")), entry("Lindqvist", Some(""))];
        assert_eq!(apply_dictionary("hello there", &d), "hello there");
        assert_eq!(relevant_terms("nothing", &d, 5), vec!["Lindqvist".to_string()], "the entry without a term is dropped");
        // an empty misspelling used to score a perfect match on every transcript
        let d = vec![entry("Alpha", Some("")), entry("Beta", None), entry("Gamma", None)];
        assert!(relevant_terms("nothing", &d, 1).is_empty(), "nothing in the transcript is close to any term");
    }

    #[test]
    fn relevant_terms_ranks_close_matches() {
        let dict: Vec<_> = (0..100).map(|i| entry(&format!("Term{i}"), None)).chain([entry("Lindqvist", None)]).collect();
        let t = relevant_terms("please email lindquist today", &dict, 5);
        assert_eq!(t.first().map(String::as_str), Some("Lindqvist"));
        let small = vec![entry("Alpha", None), entry("Beta", None)];
        assert_eq!(relevant_terms("nothing", &small, 40).len(), 2);
    }
}
