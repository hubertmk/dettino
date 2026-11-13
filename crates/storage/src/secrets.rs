//! API keys and the history key live in the OS keychain (Windows Credential
//! Manager, macOS Keychain). They are never written to settings or logs.

const SERVICE: &str = "Dettino";

fn entry(name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, name).map_err(|e| e.to_string())
}

/// Read a secret. `Ok(None)` means the keychain has no such entry; `Err` means the
/// keychain could not be asked at all (locked, a denied prompt, the service missing).
/// Callers that would create or overwrite something must treat the two differently.
pub fn try_get(name: &str) -> Result<Option<String>, String> {
    match entry(name)?.get_password() {
        Ok(v) if !v.is_empty() => Ok(Some(v)),
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// [`try_get`] for callers that only need "is there a usable value right now".
pub fn get(name: &str) -> Option<String> {
    try_get(name).ok().flatten()
}

pub fn set(name: &str, value: &str) -> Result<(), String> {
    let e = entry(name)?;
    if value.trim().is_empty() {
        return match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(err.to_string()),
        };
    }
    e.set_password(value.trim()).map_err(|e| e.to_string())
}

pub fn provider_key_name(provider_id: &str) -> String {
    format!("provider:{provider_id}")
}

/// The API key of a provider; `Err` when the keychain is unavailable (not cached by callers).
pub fn provider_key(provider_id: &str) -> Result<Option<String>, String> {
    try_get(&provider_key_name(provider_id))
}

/// 32-byte history key, created on first use.
///
/// A new key is generated only when the keychain positively reports that there is
/// none. Any other failure is returned as an error, so a locked keychain or a
/// dismissed prompt can never replace the key that the existing history was
/// encrypted with.
pub fn history_key() -> Result<[u8; 32], String> {
    if let Some(hex_key) = try_get("history-key")? {
        let bytes = hex::decode(hex_key).map_err(|e| e.to_string())?;
        return bytes.try_into().map_err(|_| "history key has the wrong length".to_string());
    }
    let mut k = [0u8; 32];
    getrandom::fill(&mut k).map_err(|e| e.to_string())?;
    set("history-key", &hex::encode(k))?;
    Ok(k)
}

pub fn has_history_key() -> bool {
    get("history-key").is_some()
}
