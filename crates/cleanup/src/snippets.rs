//! Snippets: a spoken trigger phrase expands into saved text with variables.

use crate::words::replace_word_ci;
use app_core::Snippet;

#[derive(Debug, Clone, Default)]
pub struct SnippetVars {
    pub clipboard: Option<String>,
    pub selection: Option<String>,
    pub app: Option<String>,
}

fn normalize(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Format `now` with a user-supplied strftime pattern. `None` for a pattern chrono
/// cannot render (`%Q`, a lone `%`): chrono's `Display` fails on those, and
/// `to_string()` on a failing `Display` panics, so the check comes first.
fn format_time(now: &chrono::DateTime<chrono::Local>, fmt: &str) -> Option<String> {
    use chrono::format::{Item, StrftimeItems};
    let items: Vec<Item> = StrftimeItems::new(fmt).collect();
    if items.iter().any(|i| matches!(i, Item::Error)) {
        return None;
    }
    let mut out = String::new();
    std::fmt::Write::write_fmt(&mut out, format_args!("{}", now.format_with_items(items.iter()))).ok()?;
    Some(out)
}

/// Render `{date}`, `{time}`, `{date:%d.%m.%Y}`, `{clipboard}`, `{selection}`, `{app}`.
/// A variable that cannot be rendered is left as written.
pub fn render(body: &str, vars: &SnippetVars) -> String {
    let now = chrono::Local::now();
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let var = &after[..end];
        let (name, fmt) = match var.split_once(':') {
            Some((n, f)) => (n, Some(f)),
            None => (var, None),
        };
        let value = match name {
            "date" => format_time(&now, fmt.unwrap_or("%Y-%m-%d")),
            "time" => format_time(&now, fmt.unwrap_or("%H:%M")),
            "clipboard" => Some(vars.clipboard.clone().unwrap_or_default()),
            "selection" => Some(vars.selection.clone().unwrap_or_default()),
            "app" => Some(vars.app.clone().unwrap_or_default()),
            _ => None,
        };
        match value {
            Some(v) => out.push_str(&v),
            None => {
                out.push('{');
                out.push_str(var);
                out.push('}');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Expand snippets. If the whole utterance is a trigger, the result is exactly the
/// body (no cleanup punctuation added). Otherwise triggers inside the text are
/// replaced inline, longest first. Returns (text, whole_match).
pub fn expand(text: &str, snippets: &[Snippet], vars: &SnippetVars) -> (String, bool) {
    let mut active: Vec<&Snippet> = snippets.iter().filter(|s| s.enabled && !s.trigger.trim().is_empty()).collect();
    if active.is_empty() {
        return (text.to_string(), false);
    }
    let norm = normalize(text);
    for s in &active {
        if normalize(&s.trigger) == norm {
            return (render(&s.body, vars), true);
        }
    }
    active.sort_by_key(|s| std::cmp::Reverse(s.trigger.trim().chars().count()));
    let mut out = text.to_string();
    for s in active {
        let trigger = s.trigger.trim();
        // Render only when the trigger occurs: bodies may read the clipboard or carry
        // a format the user is still typing.
        if !out.to_lowercase().contains(&trigger.to_lowercase()) {
            continue;
        }
        let rendered = render(&s.body, vars);
        out = replace_word_ci(&out, trigger, &rendered);
    }
    (out, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sn(t: &str, b: &str) -> Snippet {
        Snippet { trigger: t.into(), body: b.into(), enabled: true }
    }

    #[test]
    fn whole_utterance() {
        let (t, whole) = expand("My address.", &[sn("my address", "Main St 1\n1010 Vienna")], &SnippetVars::default());
        assert!(whole);
        assert_eq!(t, "Main St 1\n1010 Vienna");
    }

    #[test]
    fn inline_and_longest_first() {
        let s = vec![sn("sig", "S"), sn("my sig", "— Nadia")];
        let (t, whole) = expand("Thanks, my sig", &s, &SnippetVars::default());
        assert!(!whole);
        assert_eq!(t, "Thanks, — Nadia");
    }

    #[test]
    fn bad_time_formats_are_left_alone() {
        let v = SnippetVars::default();
        assert_eq!(render("{date:%Q} {time:50%}", &v), "{date:%Q} {time:50%}");
        assert_eq!(render("{date:%Y}", &v).len(), 4);
        // An unused snippet with a bad format never runs at all.
        let (t, whole) = expand("Hello there", &[sn("sig", "{date:%Q}")], &v);
        assert_eq!((t.as_str(), whole), ("Hello there", false));
    }

    #[test]
    fn variables() {
        let v = SnippetVars { clipboard: Some("CLIP".into()), app: Some("Slack".into()), ..Default::default() };
        let r = render("{clipboard} in {app} on {date:%Y} {unknown}", &v);
        assert!(r.starts_with("CLIP in Slack on 20"));
        assert!(r.ends_with("{unknown}"));
    }
}
