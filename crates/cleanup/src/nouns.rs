//! On-device proper-noun extraction from screen text; nothing leaves the machine.

use crate::words::{tokens, STOPWORDS};
use std::collections::HashMap;

const COMMON_CAPS: &[&str] = &[
    "The", "A", "An", "I", "It", "This", "That", "We", "You", "He", "She", "They", "Hi", "Hello", "Thanks",
    "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday", "January", "February",
    "March", "April", "May", "June", "July", "August", "September", "October", "November", "December",
    "File", "Edit", "View", "Help", "Window", "Home", "Search", "Settings", "New", "Open", "Save", "Close",
    "Reply", "Send", "Inbox", "Today", "Yesterday",
];

/// Up to `max` distinctive tokens: capitalized words not at sentence start,
/// CamelCase, identifiers with digits or underscores, e-mail addresses.
pub fn extract(text: &str, max: usize) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for t in tokens(text) {
        let t = t.trim_matches(|c: char| c == '.' || c == '-' || c == '/');
        if t.chars().count() < 2 || t.chars().count() > 40 {
            continue;
        }
        let first_upper = t.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
        let inner_upper = t.chars().skip(1).any(|c| c.is_uppercase()) && t.chars().any(|c| c.is_lowercase());
        let ident = t.contains('_') || (t.chars().any(|c| c.is_ascii_digit()) && t.chars().any(|c| c.is_alphabetic()));
        let email = t.contains('@');
        let all_caps = t.chars().all(|c| c.is_uppercase() || c.is_ascii_digit())
            && t.chars().any(|c| c.is_uppercase())
            && t.chars().count() <= 6
            && t.chars().count() >= 2;
        let keep = (first_upper || inner_upper || ident || email || all_caps)
            && !COMMON_CAPS.contains(&t)
            && !STOPWORDS.contains(&t.to_lowercase().as_str());
        if keep {
            let e = counts.entry(t.to_string()).or_insert(0);
            if *e == 0 {
                order.push(t.to_string());
            }
            *e += 1;
        }
    }
    // Frequent first, then first-seen order.
    order.sort_by_key(|t| std::cmp::Reverse(counts[t]));
    order.truncate(max);
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_names_and_identifiers() {
        let n = extract("Hi Nadia, the userCount field in AppConfig broke. Ask Lindqvist on Monday. user_id v2", 10);
        for want in ["Nadia", "userCount", "AppConfig", "Lindqvist", "user_id", "v2"] {
            assert!(n.contains(&want.to_string()), "{want} missing from {n:?}");
        }
        assert!(!n.contains(&"Monday".to_string()));
        assert!(!n.contains(&"Hi".to_string()));
    }
}
