//! Settings file (JSON). Written atomically. Missing fields fall back to defaults,
//! and a field whose value no longer parses (a renamed enum variant after a
//! downgrade, a damaged entry) is dropped on its own, so the rest of the file
//! (dictionary, snippets, app rules, provider setup) survives.

use app_core::{Accent, Settings};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Set when the file exists but could not be read (locked, permission denied, bad
/// encoding). Saving is refused for the rest of the session: defaults must never be
/// written over a file that is merely temporarily unreadable.
static UNREADABLE: AtomicBool = AtomicBool::new(false);
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn load(path: &Path) -> Settings {
    let text = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
        Err(e) => {
            tracing::error!("settings.json could not be read ({e}); using defaults for this session and not saving over it");
            UNREADABLE.store(true, Ordering::SeqCst);
            return Settings::default();
        }
    };
    let mut v = match serde_json::from_str::<Settings>(&text) {
        Ok(v) => v,
        Err(e) => {
            let kept = keep_copy(path);
            match salvage(&text) {
                Some((s, dropped)) => {
                    tracing::error!(dropped = ?dropped, copy = %kept, "settings.json partly unreadable ({e}); invalid fields reset to defaults");
                    s
                }
                None => {
                    tracing::error!(copy = %kept, "settings.json unreadable ({e}); using defaults");
                    Settings::default()
                }
            }
        }
    };
    if legacy_os_accent(&text) {
        v.accent = Accent::System;
    }
    v.clamp_ranges();
    // New default providers added in later versions appear automatically.
    for p in app_core::default_providers() {
        if v.provider(&p.id).is_none() {
            v.providers.push(p);
        }
    }
    v
}

/// Copy the broken file aside under a name that is never overwritten.
fn keep_copy(path: &Path) -> String {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let copy = path.with_extension(format!("json.{stamp}.bad"));
    let _ = std::fs::copy(path, &copy);
    copy.display().to_string()
}

/// Rebuild the settings from a file that does not parse as a whole: every top-level
/// field is tried on its own (array elements one by one) on top of the defaults, and
/// the ones that fail are dropped. Returns the result and the dropped field names.
fn salvage(text: &str) -> Option<(Settings, Vec<String>)> {
    let Value::Object(map) = serde_json::from_str::<Value>(text).ok()? else { return None };
    let mut base = serde_json::to_value(Settings::default()).ok()?;
    let mut dropped = Vec::new();
    let fits = |candidate: &Value| serde_json::from_value::<Settings>(candidate.clone()).is_ok();
    for (k, v) in map {
        let v = match v {
            Value::Array(items) => {
                let mut good = Vec::new();
                for (i, item) in items.into_iter().enumerate() {
                    let mut trial = base.clone();
                    trial[&k] = Value::Array(vec![item.clone()]);
                    if fits(&trial) {
                        good.push(item);
                    } else {
                        dropped.push(format!("{k}[{i}]"));
                    }
                }
                Value::Array(good)
            }
            other => other,
        };
        let mut trial = base.clone();
        trial[&k] = v;
        if fits(&trial) {
            base = trial;
        } else {
            dropped.push(k);
        }
    }
    serde_json::from_value(base).ok().map(|s| (s, dropped))
}

/// Files from before the accent picker turned the OS accent on with `"use_os_accent": true`.
fn legacy_os_accent(json: &str) -> bool {
    serde_json::from_str::<Value>(json)
        .is_ok_and(|j| j.get("accent").is_none() && j.get("use_os_accent") == Some(&Value::Bool(true)))
}

pub fn save(path: &Path, s: &Settings) -> std::io::Result<()> {
    if UNREADABLE.load(Ordering::SeqCst) {
        return Err(std::io::Error::other(
            "settings.json could not be read when Dettino started, so it is not being overwritten. Fix the file (or delete it) and restart.",
        ));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // A unique temp name: two saves at the same moment must not truncate each other.
    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("json.{}.{n}.tmp", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        std::io::Write::write_all(&mut f, &serde_json::to_vec_pretty(s)?)?;
        // Reach the disk before the rename, or a power loss can leave an empty file.
        f.sync_all()?;
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("settings-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn bad_copies(d: &Path) -> usize {
        std::fs::read_dir(d).unwrap().flatten().filter(|e| e.path().to_string_lossy().ends_with(".bad")).count()
    }

    #[test]
    fn roundtrip_and_corrupt_file() {
        let d = dir("roundtrip");
        let p = d.join("settings.json");
        let s = Settings { tap_max_ms: 321, ..Settings::default() };
        save(&p, &s).unwrap();
        assert_eq!(load(&p).tap_max_ms, 321);
        std::fs::write(&p, "{ not json").unwrap();
        assert_eq!(load(&p), Settings::default());
        assert_eq!(bad_copies(&d), 1);
        // a second bad launch keeps the first copy
        std::thread::sleep(std::time::Duration::from_millis(1100));
        load(&p);
        assert_eq!(bad_copies(&d), 2);
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn unknown_values_only_reset_their_own_field() {
        let d = dir("salvage");
        let p = d.join("settings.json");
        std::fs::write(
            &p,
            r#"{
              "tap_max_ms": 300,
              "accent": "from_a_newer_version",
              "pill_style": "classic",
              "dictionary": [
                {"term": "Lindqvist", "replace_from": "Lindquist", "source": "manual", "enabled": true},
                {"term": "Broken", "enabled": "yes"}
              ],
              "snippets": [{"trigger": "sig", "body": "Nadia", "enabled": true}],
              "context": {"enabled": false, "visible_text": 7}
            }"#,
        )
        .unwrap();
        let s = load(&p);
        assert_eq!(s.tap_max_ms, 300, "valid fields are kept");
        assert_eq!(s.accent, Accent::Cobalt, "the unknown accent falls back to the default");
        assert_eq!(s.pill_style, app_core::PillStyle::Classic);
        assert_eq!(s.dictionary.len(), 1, "only the broken dictionary entry is dropped");
        assert_eq!(s.dictionary[0].term, "Lindqvist");
        assert_eq!(s.snippets.len(), 1);
        assert!(s.context.enabled, "a nested object with a bad field falls back as a whole");
        assert_eq!(bad_copies(&d), 1, "the original is kept aside");
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn legacy_os_accent_becomes_system() {
        let d = dir("accent");
        let p = d.join("settings.json");
        std::fs::write(&p, r#"{"use_os_accent": true}"#).unwrap();
        assert_eq!(load(&p).accent, Accent::System);
        std::fs::write(&p, r#"{"use_os_accent": false}"#).unwrap();
        assert_eq!(load(&p).accent, Accent::Cobalt);
        std::fs::write(&p, r#"{"use_os_accent": true, "accent": "plum"}"#).unwrap();
        assert_eq!(load(&p).accent, Accent::Plum);
        std::fs::remove_dir_all(d).ok();
    }
}
