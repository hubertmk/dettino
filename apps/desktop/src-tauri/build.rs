use std::process::Command;

/// Build identity for `src/build_info.rs`, passed on as `cargo:rustc-env`.
///
/// `scripts/build.mjs` sets these four variables for installer builds (computed
/// once, fresh, with the same git commands) and names the installer from them. Any
/// other build (`tauri dev`, `cargo build`) probes git here. Missing git never fails
/// the build; it just yields "unknown".
///
///   DETTINO_BUILD_COMMIT       short sha of HEAD
///   DETTINO_BUILD_DIRTY        "1" when `git status --porcelain` lists anything (untracked too)
///   DETTINO_BUILD_TAGGED       "1" when a tag `v<CARGO_PKG_VERSION>` points at HEAD
///   DETTINO_BUILD_COMMIT_DATE  YYYY-MM-DD of HEAD
const BUILD_VARS: [&str; 4] = ["DETTINO_BUILD_COMMIT", "DETTINO_BUILD_DIRTY", "DETTINO_BUILD_TAGGED", "DETTINO_BUILD_COMMIT_DATE"];

fn git(args: &[&str]) -> Option<String> {
    let dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let out = Command::new("git").args(args).current_dir(dir).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn flag(on: bool) -> String {
    (if on { "1" } else { "0" }).to_string()
}

fn build_identity() {
    for v in BUILD_VARS {
        println!("cargo:rerun-if-env-changed={v}");
    }
    // Probe again after a commit or checkout. Not on the index: every rerun recompiles
    // this crate, and installer builds get fresh values from build.mjs anyway.
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        for f in ["HEAD", "logs/HEAD"] {
            let p = format!("{dir}/{f}");
            if std::path::Path::new(&p).exists() {
                println!("cargo:rerun-if-changed={p}");
            }
        }
    }
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let env_or = |name: &str, probe: &dyn Fn() -> String| std::env::var(name).ok().filter(|v| !v.is_empty()).unwrap_or_else(probe);
    let unknown = || "unknown".to_string();
    let commit = env_or(BUILD_VARS[0], &|| git(&["rev-parse", "--short=9", "HEAD"]).unwrap_or_else(unknown));
    let dirty = env_or(BUILD_VARS[1], &|| flag(git(&["status", "--porcelain"]).is_some_and(|s| !s.is_empty())));
    let tagged = env_or(BUILD_VARS[2], &|| flag(git(&["tag", "--points-at", "HEAD"]).is_some_and(|t| t.lines().any(|l| l == format!("v{version}")))));
    let date = env_or(BUILD_VARS[3], &|| git(&["show", "-s", "--format=%cs", "HEAD"]).unwrap_or_else(unknown));
    for (name, value) in BUILD_VARS.iter().zip([commit, dirty, tagged, date]) {
        println!("cargo:rustc-env={name}={value}");
    }
}

/// `DETTINO_EXPIRES` (YYYY-MM-DD), set by `scripts/build.mjs`, is the last day a
/// test build works (`src/expiry.rs` reads it with `option_env!`). A date that does
/// not exist fails the build here: at runtime it would mean "never expires".
fn check_expiry() {
    println!("cargo:rerun-if-env-changed=DETTINO_EXPIRES");
    let Some(d) = std::env::var("DETTINO_EXPIRES").ok().filter(|d| !d.is_empty()) else { return };
    if !valid_date(&d) {
        panic!("DETTINO_EXPIRES={d:?} is not a real date in the form YYYY-MM-DD");
    }
}

fn valid_date(d: &str) -> bool {
    let parts: Vec<&str> = d.split('-').collect();
    let [y, m, day] = parts[..] else { return false };
    if y.len() != 4 || m.len() != 2 || day.len() != 2 {
        return false;
    }
    let (Ok(y), Ok(m), Ok(day)) = (y.parse::<u32>(), m.parse::<u32>(), day.parse::<u32>()) else { return false };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

fn main() {
    build_identity();
    check_expiry();
    // sherpa-onnx and onnxruntime are dylibs linked as `@rpath/...`. In a bundle they
    // sit in Contents/Frameworks; in target/<profile> they sit next to the binary.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,@executable_path/../Frameworks");
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,@executable_path");
    }
    tauri_build::build()
}
