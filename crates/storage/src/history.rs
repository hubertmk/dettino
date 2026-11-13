//! Encrypted history. Each record is a JSON payload sealed with XChaCha20-Poly1305;
//! only the id, timestamp and status are stored in clear (for ordering and
//! retention). Audio files are referenced by path and purged after N days.

use app_core::{ProviderStats, Timings};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::XChaCha20Poly1305;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Ok,
    /// Pasted the raw transcript because cleanup was skipped, failed or rejected.
    OkRaw,
    /// Could not paste into the target; text waits for the user.
    Held,
    Failed,
    Cancelled,
    NothingHeard,
    /// Recording survived a crash and was never processed.
    Unprocessed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HistoryRecord {
    pub id: String,
    pub created_at: i64,
    pub status: Status,
    pub mode: String,
    pub raw_text: Option<String>,
    pub cleaned_text: Option<String>,
    pub final_text: Option<String>,
    pub edited_text: Option<String>,
    pub zero_edit: Option<bool>,
    pub cleanup_level: Option<String>,
    pub app: Option<String>,
    pub window_title: Option<String>,
    pub url: Option<String>,
    pub text_before: Option<String>,
    pub selection: Option<String>,
    pub audio_path: Option<String>,
    pub audio_ms: u64,
    pub error: Option<String>,
    pub timings: Timings,
}

pub struct History {
    conn: Connection,
    cipher: XChaCha20Poly1305,
    /// Where recordings live; `audio_path` values that are bare file names resolve here.
    audio_dir: Option<PathBuf>,
}

/// kv key of the canary that proves the key in the keychain matches this database.
const KEY_CHECK: &str = "key_check";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl History {
    pub fn open(path: &Path, key: [u8; 32]) -> Result<Self, String> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS history (
                id TEXT PRIMARY KEY,
                created_at INTEGER NOT NULL,
                status TEXT NOT NULL,
                purge_audio_after INTEGER,
                payload BLOB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS history_created ON history(created_at DESC);
             CREATE TABLE IF NOT EXISTS provider_stats (
                provider TEXT NOT NULL, ts INTEGER NOT NULL, latency_ms INTEGER NOT NULL, ok INTEGER NOT NULL, err TEXT
             );
             CREATE TABLE IF NOT EXISTS kv (k TEXT PRIMARY KEY, payload BLOB NOT NULL);",
        )
        .map_err(|e| e.to_string())?;
        // Clear-text file name of the recording, so retention never depends on the key.
        let has_audio_file: bool = conn
            .prepare("PRAGMA table_info(history)")
            .and_then(|mut s| s.query_map([], |r| r.get::<_, String>(1))?.collect::<Result<Vec<_>, _>>())
            .map(|cols| cols.iter().any(|c| c == "audio_file"))
            .map_err(|e| e.to_string())?;
        if !has_audio_file {
            conn.execute_batch("ALTER TABLE history ADD COLUMN audio_file TEXT;").map_err(|e| e.to_string())?;
        }
        let cipher = XChaCha20Poly1305::new(&key.into());
        let h = Self { conn, cipher, audio_dir: None };
        h.verify_key()?;
        Ok(h)
    }

    pub fn with_audio_dir(mut self, dir: PathBuf) -> Self {
        self.audio_dir = Some(dir);
        self
    }

    /// Refuse to work with a key that does not open this database: writing new rows
    /// with a second key would leave a file nobody can read as a whole. A database
    /// without the canary is checked against its newest row, then gets one.
    fn verify_key(&self) -> Result<(), String> {
        let canary: Option<Vec<u8>> = self
            .conn
            .query_row("SELECT payload FROM kv WHERE k = ?1", params![KEY_CHECK], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        let newest: Option<Vec<u8>> = self
            .conn
            .query_row("SELECT payload FROM history ORDER BY created_at DESC LIMIT 1", [], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        let sample = canary.clone().or(newest);
        match sample {
            Some(blob) if self.open_sealed(&blob).is_none() => {
                Err("the history key in the keychain does not match history.db (was the keychain entry replaced?)".into())
            }
            _ => {
                if canary.is_none() {
                    self.kv_set(KEY_CHECK, &"ok")?;
                }
                Ok(())
            }
        }
    }

    /// The recording behind a record, if it still exists: an absolute path as stored
    /// (older records), or a file name under the audio folder.
    pub fn audio_file(&self, rec: &HistoryRecord) -> Option<PathBuf> {
        let p = rec.audio_path.as_deref()?;
        let as_stored = PathBuf::from(p);
        if as_stored.is_absolute() && as_stored.exists() {
            return Some(as_stored);
        }
        let name = as_stored.file_name()?;
        let under_dir = self.audio_dir.as_ref()?.join(name);
        under_dir.exists().then_some(under_dir)
    }

    fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, String> {
        let mut nonce = [0u8; 24];
        getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
        let mut out = nonce.to_vec();
        out.extend(self.cipher.encrypt(&nonce.into(), plain).map_err(|_| "encrypt failed".to_string())?);
        Ok(out)
    }

    fn open_sealed(&self, sealed: &[u8]) -> Option<Vec<u8>> {
        if sealed.len() < 24 {
            return None;
        }
        let nonce: [u8; 24] = sealed[..24].try_into().ok()?;
        self.cipher.decrypt(&nonce.into(), &sealed[24..]).ok()
    }

    pub fn new_id() -> String {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let mut b = [0u8; 6];
        if getrandom::fill(&mut b).is_err() {
            // Still unique within the process: two ids in one millisecond must not collide.
            let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            b.copy_from_slice(&n.to_le_bytes()[..6]);
        }
        format!("{:013x}{}", now_ms(), hex::encode(b))
    }

    pub fn upsert(&self, rec: &HistoryRecord, audio_retention_days: u32) -> Result<(), String> {
        let payload = self.seal(&serde_json::to_vec(rec).map_err(|e| e.to_string())?)?;
        let purge = rec.audio_path.as_ref().map(|_| rec.created_at + audio_retention_days as i64 * 86_400_000);
        let audio_file = rec.audio_path.as_deref().and_then(|p| Path::new(p).file_name()).map(|n| n.to_string_lossy().into_owned());
        let status = serde_json::to_value(rec.status).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        self.conn
            .execute(
                "INSERT INTO history (id, created_at, status, purge_audio_after, audio_file, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET status=excluded.status, purge_audio_after=excluded.purge_audio_after,
                 audio_file=excluded.audio_file, payload=excluded.payload",
                params![rec.id, rec.created_at, status, purge, audio_file, payload],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<HistoryRecord> {
        let blob: Vec<u8> = self
            .conn
            .query_row("SELECT payload FROM history WHERE id = ?1", params![id], |r| r.get(0))
            .ok()?;
        serde_json::from_slice(&self.open_sealed(&blob)?).ok()
    }

    /// Newest first. `query` filters on raw, cleaned and final text (case-insensitive).
    pub fn list(&self, limit: usize, query: Option<&str>) -> Vec<HistoryRecord> {
        let Ok(mut stmt) = self.conn.prepare("SELECT payload FROM history ORDER BY created_at DESC LIMIT ?1") else {
            return Vec::new();
        };
        let scan = if query.is_some() { 5000 } else { limit };
        let rows = stmt.query_map(params![scan as i64], |r| r.get::<_, Vec<u8>>(0));
        let q = query.map(|q| q.to_lowercase()).filter(|q| !q.trim().is_empty());
        let mut out = Vec::new();
        if let Ok(rows) = rows {
            for blob in rows.flatten() {
                let Some(plain) = self.open_sealed(&blob) else { continue };
                let Ok(rec) = serde_json::from_slice::<HistoryRecord>(&plain) else { continue };
                if let Some(q) = &q {
                    let hay = [&rec.raw_text, &rec.cleaned_text, &rec.final_text, &rec.app]
                        .iter()
                        .filter_map(|s| s.as_deref())
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase();
                    if !hay.contains(q.as_str()) {
                        continue;
                    }
                }
                out.push(rec);
                if out.len() >= limit {
                    break;
                }
            }
        }
        out
    }

    pub fn latest_pasted(&self) -> Option<HistoryRecord> {
        self.list(20, None).into_iter().find(|r| r.final_text.is_some())
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        // The clear-text file name works even when the row cannot be decrypted.
        let file: Option<Option<String>> = self
            .conn
            .query_row("SELECT audio_file FROM history WHERE id = ?1", params![id], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(p) = file.flatten().and_then(|f| self.audio_dir.as_ref().map(|d| d.join(f))) {
            let _ = std::fs::remove_file(p);
        }
        if let Some(p) = self.get(id).and_then(|r| self.audio_file(&r)) {
            let _ = std::fs::remove_file(p);
        }
        self.conn.execute("DELETE FROM history WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_all(&self, audio_dir: &Path) -> Result<(), String> {
        self.conn.execute_batch("DELETE FROM history; DELETE FROM kv; DELETE FROM provider_stats; VACUUM;").map_err(|e| e.to_string())?;
        if let Ok(rd) = std::fs::read_dir(audio_dir) {
            for e in rd.flatten() {
                let _ = std::fs::remove_file(e.path());
            }
        }
        Ok(())
    }

    /// Delete audio past its retention date. Returns the number of files removed.
    /// The file reference is dropped from the record only once the file is gone (or
    /// was already gone); a locked file is tried again on the next run.
    pub fn purge_audio(&self) -> usize {
        let now = now_ms();
        let rows: Vec<(String, Option<String>)> = self
            .conn
            .prepare("SELECT id, audio_file FROM history WHERE purge_audio_after IS NOT NULL AND purge_audio_after < ?1")
            .and_then(|mut s| s.query_map(params![now], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
            .unwrap_or_default();
        let mut n = 0;
        for (id, file) in rows {
            let rec = self.get(&id);
            let mut candidates: Vec<PathBuf> = Vec::new();
            if let Some(p) = rec.as_ref().and_then(|r| self.audio_file(r)) {
                candidates.push(p);
            }
            if let (Some(f), Some(dir)) = (file, &self.audio_dir) {
                candidates.push(dir.join(f));
            }
            candidates.dedup();
            let mut gone = true;
            for p in candidates {
                match std::fs::remove_file(&p) {
                    Ok(()) => n += 1,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        tracing::warn!("could not delete {}: {e}", p.display());
                        gone = false;
                    }
                }
            }
            if !gone {
                continue;
            }
            if let Some(mut rec) = rec {
                rec.audio_path = None;
                let _ = self.upsert(&rec, 0);
            }
            let _ = self.conn.execute("UPDATE history SET purge_audio_after = NULL, audio_file = NULL WHERE id = ?1", params![id]);
        }
        n
    }

    pub fn record_stat(&self, provider: &str, latency_ms: u64, ok: bool, err: Option<&str>) {
        let _ = self.conn.execute(
            "INSERT INTO provider_stats (provider, ts, latency_ms, ok, err) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![provider, now_ms(), latency_ms as i64, ok as i64, err],
        );
        // keep the table small
        let _ = self.conn.execute(
            "DELETE FROM provider_stats WHERE rowid IN (SELECT rowid FROM provider_stats WHERE provider = ?1 ORDER BY ts DESC LIMIT -1 OFFSET 500)",
            params![provider],
        );
    }

    pub fn load_stats(&self) -> ProviderStats {
        let mut stats = ProviderStats::default();
        if let Ok(mut s) = self.conn.prepare("SELECT provider, ts, latency_ms, ok, err FROM provider_stats ORDER BY ts ASC") {
            if let Ok(rows) = s.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            }) {
                for (p, ts, ms, ok, err) in rows.flatten() {
                    stats.record(&p, app_core::stats::Sample { ts: ts as u64, latency_ms: ms as u64, ok: ok != 0, err });
                }
            }
        }
        stats
    }

    /// Encrypted key-value store (pending dictionary suggestions, etc.).
    pub fn kv_get<T: for<'de> Deserialize<'de>>(&self, k: &str) -> Option<T> {
        let blob: Vec<u8> = self.conn.query_row("SELECT payload FROM kv WHERE k = ?1", params![k], |r| r.get(0)).ok()?;
        serde_json::from_slice(&self.open_sealed(&blob)?).ok()
    }

    pub fn kv_set<T: Serialize>(&self, k: &str, v: &T) -> Result<(), String> {
        let payload = self.seal(&serde_json::to_vec(v).map_err(|e| e.to_string())?)?;
        self.conn
            .execute("INSERT INTO kv (k, payload) VALUES (?1, ?2) ON CONFLICT(k) DO UPDATE SET payload=excluded.payload", params![k, payload])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// WAVs left in `sessions/` by a crash: repaired and listed so they can be retried.
pub fn orphaned_sessions(sessions_dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(sessions_dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e == "wav").unwrap_or(false)).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("hist-{}-{}", std::process::id(), History::new_id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn roundtrip_search_and_encryption() {
        let d = tmp();
        let h = History::open(&d.join("h.db"), [7u8; 32]).unwrap();
        let mut r = HistoryRecord { id: History::new_id(), created_at: now_ms(), raw_text: Some("secret words".into()), ..Default::default() };
        h.upsert(&r, 7).unwrap();
        r.final_text = Some("Secret words.".into());
        h.upsert(&r, 7).unwrap();
        assert_eq!(h.get(&r.id).unwrap().final_text.as_deref(), Some("Secret words."));
        assert_eq!(h.list(10, Some("SECRET")).len(), 1);
        assert_eq!(h.list(10, Some("nothing")).len(), 0);
        drop(h);
        // plaintext must not appear in the database file
        let raw = std::fs::read(d.join("h.db")).unwrap();
        assert!(!raw.windows(6).any(|w| w == b"secret"));
        // a wrong key is refused outright instead of silently showing an empty history
        let err = History::open(&d.join("h.db"), [8u8; 32]).err().expect("key mismatch is detected");
        assert!(err.contains("does not match"), "{err}");
        // the right key still opens it
        assert!(History::open(&d.join("h.db"), [7u8; 32]).is_ok());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn key_mismatch_without_canary_is_detected_from_rows() {
        let d = tmp();
        let h = History::open(&d.join("h.db"), [1u8; 32]).unwrap();
        // simulate a database written before the canary existed
        h.conn.execute("DELETE FROM kv", []).unwrap();
        h.upsert(&HistoryRecord { id: History::new_id(), created_at: now_ms(), ..Default::default() }, 7).unwrap();
        drop(h);
        assert!(History::open(&d.join("h.db"), [2u8; 32]).is_err());
        assert!(History::open(&d.join("h.db"), [1u8; 32]).is_ok());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn audio_purge_resolves_file_names_under_the_audio_folder() {
        let d = tmp();
        let audio = d.join("audio");
        std::fs::create_dir_all(&audio).unwrap();
        let h = History::open(&d.join("h.db"), [1u8; 32]).unwrap().with_audio_dir(audio.clone());
        std::fs::write(audio.join("a.wav"), b"x").unwrap();
        // a record from before the data folder moved: absolute path that no longer exists
        let r = HistoryRecord {
            id: History::new_id(),
            created_at: now_ms() - 8 * 86_400_000,
            audio_path: Some(d.join("old-folder").join("a.wav").to_string_lossy().into()),
            ..Default::default()
        };
        h.upsert(&r, 7).unwrap();
        assert_eq!(h.audio_file(&r), Some(audio.join("a.wav")));
        assert_eq!(h.purge_audio(), 1);
        assert!(!audio.join("a.wav").exists());
        assert!(h.get(&r.id).unwrap().audio_path.is_none());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn audio_purge() {
        let d = tmp();
        let h = History::open(&d.join("h.db"), [1u8; 32]).unwrap();
        let wav = d.join("a.wav");
        std::fs::write(&wav, b"x").unwrap();
        let r = HistoryRecord {
            id: History::new_id(),
            created_at: now_ms() - 8 * 86_400_000,
            audio_path: Some(wav.to_string_lossy().into()),
            ..Default::default()
        };
        h.upsert(&r, 7).unwrap();
        assert_eq!(h.purge_audio(), 1);
        assert!(!wav.exists());
        assert!(h.get(&r.id).unwrap().audio_path.is_none());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn stats_persist() {
        let d = tmp();
        let h = History::open(&d.join("h.db"), [1u8; 32]).unwrap();
        h.record_stat("p", 100, true, None);
        h.record_stat("p", 300, false, Some("401"));
        let s = h.load_stats().summary("p");
        assert_eq!(s.count, 2);
        assert_eq!(s.last_error.as_deref(), Some("401"));
        std::fs::remove_dir_all(d).ok();
    }
}
