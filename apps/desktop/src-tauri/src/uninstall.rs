//! `dettino --uninstall-cleanup [--remove-ollama]`, run by the Windows uninstaller
//! (installer.nsi) before it deletes the program. It covers what the uninstaller
//! cannot reach itself: the keys in the OS keychain, data folders of the builds
//! before the rename and, when that box is ticked, Ollama. The uninstaller deletes
//! the current data folder afterwards.

use app_core::Settings;
use app_storage::{secrets, settings as settings_store, Paths};

/// Handle the uninstaller's arguments; `true` when the process should exit right away.
pub fn handle_args() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = args.iter().any(|a| a == "--uninstall-cleanup");
    let ollama = args.iter().any(|a| a == "--remove-ollama");
    if data {
        remove_data();
    }
    #[cfg(windows)]
    if ollama {
        let _ = crate::ollama::remove();
    }
    data || ollama
}

fn remove_data() {
    // Keys are stored per provider; the settings know the custom ones.
    let s = settings_store::load(&Paths::default_root().settings());
    let mut ids: Vec<String> = s.providers.iter().chain(Settings::default().providers.iter()).map(|p| p.id.clone()).collect();
    ids.sort();
    ids.dedup();
    for id in ids {
        let _ = secrets::set(&secrets::provider_key_name(&id), "");
    }
    let _ = secrets::set("history-key", "");
    let _ = app_platform::set_launch_at_login(false);
}
