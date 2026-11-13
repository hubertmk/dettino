//! Which build is this? The version comes from the workspace `Cargo.toml` (the one
//! place it is defined; `tauri.conf.json` and `package.json` carry none), the rest
//! from `build.rs` (`DETTINO_BUILD_*`). `label_for` is mirrored in
//! `scripts/build.mjs`, which names the installer by the same rule.
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct BuildInfo {
    /// Plain semver: what the installer, Apps & Features and the file properties show.
    pub version: &'static str,
    /// What people read: `0.2.0` for a release, else `0.2.0+a1b2c3d4e[.dirty]`.
    pub label: String,
    pub commit: &'static str,
    /// Built with uncommitted or untracked changes: the commit alone does not reproduce it.
    pub dirty: bool,
    /// A clean build at its own tag `v<version>`, which is what `pnpm release` makes.
    pub release: bool,
    /// Date of the commit, YYYY-MM-DD.
    pub commit_date: &'static str,
    /// `release` or `debug` (the Cargo profile, unrelated to `release` above).
    pub profile: &'static str,
    /// Last day a test build works, YYYY-MM-DD (see `expiry.rs`); `None` never expires.
    pub expires: Option<String>,
}

pub fn current() -> BuildInfo {
    let version = env!("CARGO_PKG_VERSION");
    let commit = env!("DETTINO_BUILD_COMMIT");
    let dirty = env!("DETTINO_BUILD_DIRTY") == "1";
    let tagged = env!("DETTINO_BUILD_TAGGED") == "1";
    BuildInfo {
        version,
        label: label_for(version, commit, dirty, tagged),
        commit,
        dirty,
        release: tagged && !dirty,
        commit_date: env!("DETTINO_BUILD_COMMIT_DATE"),
        profile: if cfg!(debug_assertions) { "debug" } else { "release" },
        expires: crate::expiry::date().map(|d| d.to_string()),
    }
}

/// A clean build at its own tag is just the version. Anything else names the commit
/// it came from, and `.dirty` when the tree had changes beyond that commit.
pub fn label_for(version: &str, commit: &str, dirty: bool, tagged: bool) -> String {
    if tagged && !dirty {
        return version.to_string();
    }
    format!("{version}+{commit}{}", if dirty { ".dirty" } else { "" })
}

#[cfg(test)]
mod tests {
    use super::label_for;

    #[test]
    fn a_clean_tagged_build_is_just_the_version() {
        assert_eq!(label_for("0.2.0", "a1b2c3d4e", false, true), "0.2.0");
    }

    #[test]
    fn an_untagged_build_names_its_commit() {
        assert_eq!(label_for("0.2.0", "a1b2c3d4e", false, false), "0.2.0+a1b2c3d4e");
    }

    #[test]
    fn uncommitted_changes_are_marked_even_at_a_tag() {
        assert_eq!(label_for("0.2.0", "a1b2c3d4e", true, true), "0.2.0+a1b2c3d4e.dirty");
        assert_eq!(label_for("0.2.0", "a1b2c3d4e", true, false), "0.2.0+a1b2c3d4e.dirty");
    }
}
