//! Small text utilities shared by the cleanup modules.

pub const FILLERS: &[&str] = &["um", "uh", "erm", "uhm", "hmm", "mm", "ah", "er"];

pub const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "so", "to", "of", "in", "on", "at", "for", "with", "is", "are",
    "was", "were", "be", "been", "it", "this", "that", "i", "you", "we", "he", "she", "they", "me", "my",
    "your", "our", "us", "do", "did", "does", "have", "has", "had", "not", "no", "yes", "if", "as", "by",
    "from", "up", "out", "just", "like", "will", "would", "can", "could", "should", "let's", "lets", "its",
    "it's", "im", "i'm", "actually", "okay", "ok", "well", "then", "there", "here", "what", "which",
];

/// Word tokens: letters, digits, apostrophes, underscores, dots and hyphens inside words.
pub fn tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (i, &(pos, c)) in chars.iter().enumerate() {
        let inner = matches!(c, '\'' | '’' | '_' | '.' | '-' | '/')
            && start.is_some()
            && chars.get(i + 1).map(|(_, n)| n.is_alphanumeric()).unwrap_or(false);
        if c.is_alphanumeric() || inner {
            if start.is_none() {
                start = Some(pos);
            }
        } else if let Some(s) = start.take() {
            out.push(&text[s..pos]);
        }
    }
    if let Some(s) = start {
        out.push(&text[s..]);
    }
    out
}

pub fn lower_tokens(text: &str) -> Vec<String> {
    tokens(text).into_iter().map(|t| t.to_lowercase().replace('’', "'")).collect()
}

pub fn is_filler(t: &str) -> bool {
    FILLERS.contains(&t)
}

pub fn content_words(text: &str) -> Vec<String> {
    lower_tokens(text)
        .into_iter()
        .filter(|t| !is_filler(t) && !STOPWORDS.contains(&t.as_str()))
        .map(|t| stem(&t))
        .collect()
}

/// Crude suffix stemmer: enough to match "meeting"/"meet", "emails"/"email".
pub fn stem(t: &str) -> String {
    for suf in ["ing", "ed", "es", "s", "ly"] {
        if t.len() > suf.len() + 2 && t.ends_with(suf) {
            return t[..t.len() - suf.len()].to_string();
        }
    }
    t.to_string()
}

/// `config.toml`, `main.rs`: a dot between two parts of at least two letters, not
/// an abbreviation (`p.m`, `e.g`) or a number (`2.5`).
fn looks_like_filename(t: &str) -> bool {
    let parts: Vec<&str> = t.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| p.len() >= 2)
        && parts.last().is_some_and(|p| p.chars().all(|c| c.is_ascii_alphanumeric()) && p.chars().any(|c| c.is_alphabetic()))
        && !t.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Code identifiers that must survive cleanup verbatim.
pub fn identifiers(text: &str) -> Vec<String> {
    let mut out: Vec<String> = tokens(text)
        .into_iter()
        .filter(|t| {
            let has_upper_inside = t.chars().skip(1).any(|c| c.is_uppercase()) && t.chars().any(|c| c.is_lowercase());
            t.contains('_') || t.contains('/') || looks_like_filename(t) || has_upper_inside
        })
        .map(String::from)
        .collect();
    // `--flags` lose their dashes in tokens(); take them from the raw text.
    for w in text.split_whitespace() {
        let w = w.trim_end_matches(['.', ',', ';', ':', '!', '?']);
        if w.starts_with("--") && w.len() > 2 && !out.iter().any(|o| o == w) {
            out.push(w.to_string());
        }
    }
    out
}

/// Replace whole-word occurrences of `from` (case-insensitive, may contain spaces).
///
/// Matching compares characters, not lower-cased bytes: lower-casing can change a
/// character's byte length (`İ`, `K`, `Ω`), so byte offsets from a lower-cased copy
/// would slice the original mid-character. Word boundaries treat `_` as part of a
/// word, so `user` never rewrites `user_count`.
pub fn replace_word_ci(text: &str, from: &str, to: &str) -> String {
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    let needle: Vec<char> = from.chars().map(fold).collect();
    let n = needle.len();
    if n == 0 {
        return text.to_string();
    }
    let chars: Vec<(usize, char)> = text.char_indices().map(|(i, c)| (i, fold(c))).collect();
    if chars.len() < n {
        return text.to_string();
    }
    let word_char = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::with_capacity(text.len());
    let mut copied = 0; // bytes of `text` already written to `out`
    let mut i = 0; // char index
    while i + n <= chars.len() {
        if chars[i..i + n].iter().map(|c| c.1).eq(needle.iter().copied()) {
            let s = chars[i].0;
            let e = chars.get(i + n).map(|c| c.0).unwrap_or(text.len());
            let before_ok = i == 0 || !word_char(chars[i - 1].1);
            let after_ok = i + n == chars.len() || !word_char(chars[i + n].1);
            if before_ok && after_ok {
                out.push_str(&text[copied..s]);
                out.push_str(to);
                copied = e;
                i += n;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&text[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizing() {
        assert_eq!(tokens("Hi, it's user_count in src/main.rs."), vec!["Hi", "it's", "user_count", "in", "src/main.rs"]);
        assert_eq!(tokens("--verbose"), vec!["verbose"]);
    }

    #[test]
    fn ids() {
        let ids = identifiers("rename userId and user_count in config.toml please");
        assert_eq!(ids, vec!["userId", "user_count", "config.toml"]);
        assert_eq!(identifiers("run it with --verbose please."), vec!["--verbose"]);
        assert!(identifiers("let's meet at 3 p.m. or 2.5 hours later, e.g. tomorrow").is_empty());
        assert_eq!(identifiers("open src/main.rs"), vec!["src/main.rs"]);
    }

    #[test]
    fn word_replace() {
        assert_eq!(replace_word_ci("Cat catalog CAT", "cat", "dog"), "dog catalog dog");
        assert_eq!(replace_word_ci("my sig here", "my sig", "Nadia"), "Nadia here");
        assert_eq!(replace_word_ci("user and user_count", "user", "User"), "User and user_count");
    }

    #[test]
    fn word_replace_survives_width_changing_case_folds() {
        // İ lower-cases to a longer byte sequence, K (Kelvin sign) to a shorter one: the
        // total length stays equal while every offset after İ is shifted.
        let text = "İstanbul Kat cat";
        assert_eq!(replace_word_ci(text, "cat", "dog"), "İstanbul Kat dog");
        // İ folds to a plain i, so a dictionary term spelled with I still matches.
        assert_eq!(replace_word_ci("İSTANBUL cat", "istanbul", "Istanbul"), "Istanbul cat");
        assert_eq!(replace_word_ci("ẞ cat ẞ", "cat", "dog"), "ẞ dog ẞ");
        assert_eq!(replace_word_ci("straße cat", "STRASSE", "x"), "straße cat");
    }
}
