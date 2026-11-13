//! Test builds stop working after a date. `pnpm build --expiring` bakes
//! `DETTINO_EXPIRES` (YYYY-MM-DD, checked by build.rs) into the binary as the build
//! day + 60 days, `--expires YYYY-MM-DD` as a given day. A plain `pnpm build`,
//! `tauri dev`, `cargo build` and CI set nothing, so those builds never expire. Only the local date counts: a reminder for alpha testers,
//! not copy protection.

use crate::APP_NAME;
use chrono::{Local, NaiveDate};
use std::time::Duration;
use tauri::AppHandle;

const EXPIRES: Option<&str> = option_env!("DETTINO_EXPIRES");

/// The last day this build works, or `None` for a build without an end date.
pub fn date() -> Option<NaiveDate> {
    EXPIRES.and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
}

/// Days left after `today`: 0 on the last day, negative once the build has expired.
pub fn days_left(expires: NaiveDate, today: NaiveDate) -> i64 {
    (expires - today).num_days()
}

pub fn expired() -> bool {
    date().is_some_and(|d| days_left(d, Local::now().date_naive()) < 0)
}

/// Tell the person, and stop starting with Windows. Settings, history and keys stay
/// for the next build. The caller exits afterwards.
pub fn stop() {
    let Some(d) = date() else { return };
    tracing::warn!(expires = %d, "this test build has expired");
    if let Err(e) = app_platform::set_launch_at_login(false) {
        tracing::warn!("launch at login: {e}");
    }
    app_platform::alert(APP_NAME, &message(d));
}

fn message(d: NaiveDate) -> String {
    format!(
        "This test version of {APP_NAME} ended on {}.\n\nDownload a new version from github.com/hubertmk/dettino/releases. Your settings and history are kept.",
        d.format("%-d %B %Y")
    )
}

/// A tray app can run for weeks: check again every hour and quit once the date has passed.
pub fn watch(handle: AppHandle) {
    if date().is_none() {
        return;
    }
    std::thread::Builder::new()
        .name("expiry".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(60 * 60));
            if expired() {
                stop();
                crate::tray::quit_app(&handle);
                return;
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn works_through_the_last_day() {
        assert_eq!(days_left(d("2026-12-29"), d("2026-12-28")), 1);
        assert_eq!(days_left(d("2026-12-29"), d("2026-12-29")), 0);
        assert_eq!(days_left(d("2026-12-29"), d("2026-12-30")), -1);
    }

    #[test]
    fn sixty_days_span_a_year_end() {
        assert_eq!(days_left(d("2027-01-01"), d("2026-11-02")), 60);
    }

    #[test]
    fn the_message_names_the_day() {
        assert!(message(d("2026-12-29")).contains("ended on 29 December 2026"));
    }
}
