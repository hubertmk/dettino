//! Processing pipeline: take → transcription chain → snippets → cleanup chain
//! with guardrails → insertion → history. Jobs run one at a time, in order.

use crate::app::{app, now_ms, LastPaste};
use crate::controller::{sound, SessionInfo};
use crate::pill::{self, PillAction, PillState};
use app_audio::Take;
use app_cleanup::rules::{self, RuleOptions};
use app_cleanup::snippets::{self, SnippetVars};
use app_cleanup::{CleanupContext, Rejection};
use app_core::{run_chain, AsrRequest, ChainAttempt, CleanupLevel, LlmProvider, Mode, ProviderConfig, ProviderError, ProviderStats, ProviderType, Settings, Timings};
use app_platform::{ContextSnapshot, ForegroundInfo, PasteChord, PasteOutcome, PasteRequest, SoundKind};
use app_storage::{HistoryRecord, Status};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;

pub struct Job {
    pub uid: String,
    pub mode: Mode,
    pub take: Option<Take>,
    pub target: Option<ForegroundInfo>,
    pub context: Arc<Mutex<Option<ContextSnapshot>>>,
    pub hold_ms: u64,
    pub key_up: Instant,
    /// Re-processing a History item: do not paste, store the result.
    pub from_history: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Suggestion {
    pub from: String,
    pub to: String,
    pub ts: i64,
}

pub fn start(mut rx: tokio::sync::mpsc::UnboundedReceiver<Job>) {
    tauri::async_runtime::spawn(async move {
        while let Some(job) = rx.recv().await {
            let uid = job.uid.clone();
            // One job must never take the queue down with it: a panic while processing
            // (a bad snippet, a poisoned lock) is logged and the next dictation still runs.
            if let Err(e) = tokio::spawn(process(job)).await {
                if e.is_panic() {
                    tracing::error!(job = %uid, "dictation processing panicked: {e}");
                    pill::flash(
                        PillState { kind: "error".into(), label: Some("Something went wrong — see the log".into()), ..Default::default() },
                        5000,
                    );
                }
            }
        }
    });
}

/// Queue a job for processing. The queue only disappears if the processing task is
/// gone, which must be visible rather than silently dropping the dictation.
pub fn enqueue(job: Job) {
    if app().jobs.send(job).is_err() {
        tracing::error!("the processing queue is gone; restart Dettino");
        pill::flash(PillState { kind: "error".into(), label: Some("Processing stopped — restart Dettino".into()), ..Default::default() }, 6000);
    }
}

fn emit_history_changed() {
    let _ = app().handle.emit("history:changed", ());
}

fn save(rec: &HistoryRecord) {
    // One text-free line per dictation: what happened and how long each step took.
    let t = &rec.timings;
    tracing::info!(
        status = ?rec.status,
        mode = %rec.mode,
        audio_ms = t.audio_ms,
        speech_ms = t.speech_ms,
        preroll_ms = t.preroll_used_ms,
        asr = t.asr_provider.as_deref().unwrap_or(""),
        asr_ms = t.asr_ms,
        asr_attempts = %t.asr_attempts.join("; "),
        raw_chars = rec.raw_text.as_ref().map(|r| r.chars().count()).unwrap_or(0),
        cleanup = t.cleanup_model.as_deref().or(t.cleanup_provider.as_deref()).unwrap_or(""),
        cleanup_ms = t.cleanup_ms,
        cleanup_skipped = t.cleanup_skipped.as_deref().unwrap_or(""),
        cleanup_rejected = t.cleanup_rejected.as_deref().unwrap_or(""),
        insert = t.insert_verify.as_deref().unwrap_or(""),
        insert_ms = t.insert_ms,
        total_ms = t.total_ms,
        error = rec.error.as_deref().unwrap_or(""),
        "dictation"
    );
    let a = app();
    let s = a.settings.read().unwrap();
    if !s.store_history {
        return;
    }
    let days = s.audio_retention_days;
    drop(s);
    if let Some(Err(e)) = a.with_history(|h| h.upsert(rec, days)) {
        tracing::error!("history write failed: {e}");
    }
    emit_history_changed();
}

/// Move a finished take from sessions/ to audio/. The record stores the file name
/// only; `History::audio_file` resolves it under the current audio folder, so the
/// reference survives a move of the data folder.
fn keep_audio(take: &Take) -> Option<String> {
    let a = app();
    if !a.settings.read().unwrap().store_history {
        let _ = std::fs::remove_file(&take.wav_path);
        return None;
    }
    let name = take.wav_path.file_name()?.to_string_lossy().into_owned();
    let dest = a.paths.audio().join(&name);
    if std::fs::rename(&take.wav_path, &dest).is_ok() {
        Some(name)
    } else if take.wav_path.exists() {
        Some(take.wav_path.to_string_lossy().into())
    } else {
        None
    }
}

fn display_name(s: &Settings, id: &str) -> String {
    s.provider(id).map(|p| p.display_name.clone()).unwrap_or_else(|| id.to_string())
}

fn context_allowed(s: &Settings, p: &ProviderConfig, rule_ctx: bool) -> bool {
    s.context.enabled && rule_ctx && (p.is_local() || !s.context.local_only)
}

/// Hedging policy for a chain: start the next provider while this one is merely slow,
/// unless that would move audio or text from an on-device provider to a cloud one.
/// Cloud providers behind a local one are a fallback for failure, never for latency.
fn hedge_policy(s: &Settings, stats: &ProviderStats, providers: &[ProviderConfig], i: usize, floor: Duration) -> Option<Duration> {
    let next = providers.get(i + 1)?;
    if providers[i].is_local() && !next.is_local() {
        return None;
    }
    Some(s.hedge_ms.map(Duration::from_millis).unwrap_or_else(|| stats.hedge_after(&providers[i].id, floor)))
}

/// The screen context the cleanup model may see, per the privacy settings.
fn cleanup_context(s: &Settings, target: Option<&ForegroundInfo>, ctx: Option<&ContextSnapshot>) -> CleanupContext {
    CleanupContext {
        app_name: target.filter(|_| s.context.app_name).map(|t| t.app.clone()),
        app_category: target.map(|t| app_platform::app_category(&t.app, &t.title).to_string()),
        window_title: target.filter(|_| s.context.window_title).map(|t| t.title.clone()),
        url: ctx.and_then(|c| c.url.clone()),
        before: ctx.and_then(|c| c.before.clone()),
        after: ctx.and_then(|c| c.after.clone()),
        selection: ctx.and_then(|c| c.selection.clone()),
        nouns: ctx.and_then(|c| c.visible_text.as_deref()).map(|t| app_cleanup::nouns::extract(t, 20)).unwrap_or_default(),
    }
}

/// What the pill says when transcription failed. `saved`: the recording is in
/// History, so Retry can re-run it; with history off there is nothing to retry.
fn error_copy(e: &ProviderError, provider: &str, saved: bool) -> (String, Option<PillAction>) {
    let retry = || saved.then(|| PillAction { id: "retry".into(), label: "Retry".into() });
    let suffix = if saved { " — your audio is saved" } else { "" };
    match e {
        ProviderError::KeyInvalid => (format!("{provider} rejected the key"), Some(PillAction { id: "open:providers".into(), label: "Fix key".into() })),
        ProviderError::NotConfigured(_) => ("No provider is set up".into(), Some(PillAction { id: "open:providers".into(), label: "Set up".into() })),
        ProviderError::ModelMissing(m) => (format!("{m} isn't downloaded"), Some(PillAction { id: "open:providers".into(), label: "Download".into() })),
        ProviderError::Offline(_) => (format!("Offline{suffix}"), retry()),
        ProviderError::Timeout => (format!("{provider} didn't answer{suffix}"), retry()),
        _ => (format!("Couldn't transcribe{suffix}"), retry()),
    }
}

async fn wait_context(slot: &Arc<Mutex<Option<ContextSnapshot>>>, max: Duration) -> Option<ContextSnapshot> {
    let t0 = Instant::now();
    loop {
        if let Some(c) = slot.lock().unwrap().clone() {
            return Some(c);
        }
        if t0.elapsed() >= max {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Loudest 40 ms of a take below this: the microphone sent (near) digital silence.
/// The same bound the audio engine uses for its live "can't hear you" warning.
const SILENT_DB: f32 = -70.0;

/// After this much detected speech a transcript should hold words.
const EXPECT_TEXT_MS: u64 = 1500;
const EMPTY_TRANSCRIPT: &str = "empty transcript";

/// An empty transcript of clear speech is a provider failure, so the chain gives
/// the next provider a turn instead of ending on "Didn't hear anything".
fn require_text(text: String, expect_text: bool) -> Result<String, ProviderError> {
    if expect_text && text.trim().is_empty() {
        Err(ProviderError::BadResponse(EMPTY_TRANSCRIPT.into()))
    } else {
        Ok(text)
    }
}

async fn transcribe(s: &Settings, pcm: Arc<Vec<i16>>, ctx: Option<&ContextSnapshot>, rule_ctx: bool, timings: &mut Timings) -> Result<String, (ProviderError, String)> {
    let a = app();
    let chain = a.chain(&s.asr_chain);
    if chain.is_empty() {
        return Err((ProviderError::NotConfigured("no transcription provider".into()), String::new()));
    }
    let dict_terms: Vec<String> = s.dictionary.iter().filter(|d| d.enabled).map(|d| d.term.clone()).take(100).collect();
    let nouns: Vec<String> = ctx
        .and_then(|c| c.visible_text.as_deref())
        .map(|t| app_cleanup::nouns::extract(t, 50))
        .unwrap_or_default();
    let audio_ms = pcm.len() as u64 * 1000 / 16_000;
    let providers: Vec<_> = chain.iter().filter_map(|p| a.asr_provider(p).map(|x| (p.clone(), x))).collect();
    let reqs: Vec<AsrRequest> = providers
        .iter()
        .map(|(cfg, _)| {
            let allowed = context_allowed(s, cfg, rule_ctx);
            let mut hot = dict_terms.clone();
            if allowed {
                hot.extend(nouns.iter().cloned());
            }
            AsrRequest {
                pcm: pcm.clone(),
                language: s.language.clone().unwrap_or_default(),
                hotwords: hot,
                prompt_tail: if allowed { ctx.and_then(|c| c.before.clone()) } else { None },
            }
        })
        .collect();
    let stats = a.stats.lock().unwrap().clone();
    let cfgs: Vec<ProviderConfig> = providers.iter().map(|(c, _)| c.clone()).collect();
    // A long recording takes a local model longer; "slow" scales with the audio.
    let floor = Duration::from_millis(1200.max(audio_ms / 8));
    let expect_text = timings.speech_ms >= EXPECT_TEXT_MS;
    let t0 = Instant::now();
    let outcome = run_chain(
        providers.len(),
        |i| hedge_policy(s, &stats, &cfgs, i, floor),
        |i| providers[i].1.timeout(audio_ms),
        |i| {
            let p = providers[i].1.clone();
            let r = reqs[i].clone();
            async move { p.transcribe(&r).await.and_then(|t| require_text(t, expect_text)) }
        },
    )
    .await;
    timings.asr_ms = Some(t0.elapsed().as_millis() as u64);
    for at in &outcome.attempts {
        let id = &providers[at.index].0.id;
        record_attempt(a, id, at);
        timings.asr_attempts.push(format!(
            "{}{} {} ms{}",
            id,
            if at.hedged { " (hedged)" } else { "" },
            at.latency_ms,
            at.error.as_ref().map(|e| format!(" · {}", e.short())).unwrap_or_default()
        ));
        timings.asr_hedged |= at.hedged;
    }
    match outcome.result {
        Ok((i, text)) => {
            timings.asr_provider = Some(providers[i].0.id.clone());
            Ok(text)
        }
        Err(e) => {
            let name = outcome.attempts.last().map(|at| display_name(s, &providers[at.index].0.id)).unwrap_or_default();
            Err((e, name))
        }
    }
}

struct CleanupResult {
    text: String,
    raw_used: bool,
    reason: Option<String>,
}

fn rule_opts(s: &Settings, app_name: &str, before: Option<&str>, remove_fillers: bool) -> RuleOptions {
    let terminal = app_platform::app_category(app_name, "") == "terminal";
    let _ = s;
    RuleOptions {
        remove_fillers,
        capitalize: !terminal,
        trailing_period: !terminal,
        continues_sentence: rules::continues_sentence(before),
    }
}

async fn cleanup(s: &Settings, raw: &str, level: CleanupLevel, ctx: Option<&ContextSnapshot>, target: Option<&ForegroundInfo>, rule_ctx: bool, timings: &mut Timings) -> CleanupResult {
    let a = app();
    let app_name = target.map(|t| t.app.as_str()).unwrap_or("");
    let before = ctx.and_then(|c| c.before.as_deref());
    let fallback = |reason: &str, timings: &mut Timings| {
        timings.cleanup_skipped = Some(reason.to_string());
        CleanupResult { text: rules::apply(raw, rule_opts(s, app_name, before, true)), raw_used: true, reason: Some(reason.to_string()) }
    };
    if level == CleanupLevel::Off {
        timings.cleanup_skipped = Some("off".into());
        return CleanupResult { text: rules::apply(raw, rule_opts(s, app_name, before, false)), raw_used: false, reason: None };
    }
    let words = app_cleanup::words::tokens(raw).len();
    if words < 4 {
        timings.cleanup_skipped = Some("short".into());
        return CleanupResult { text: rules::apply(raw, rule_opts(s, app_name, before, true)), raw_used: false, reason: None };
    }
    if app_cleanup::too_long_for_cleanup(raw) {
        timings.cleanup_skipped = Some("long".into());
        return fallback("Too long for AI cleanup; local rules only.", timings);
    }
    let chain = a.chain(&s.llm_chain);
    if chain.is_empty() {
        return fallback("no cleanup provider set up", timings);
    }
    // Right after launch Ollama may still be starting: wait for it rather than skip.
    if chain[0].ptype == ProviderType::Ollama && crate::startup::boot() == crate::startup::Boot::Starting {
        let mut st = pill::current();
        if st.kind == "thinking" {
            st.label = Some("Espresso's almost ready — waiting for Ollama…".into());
            pill::set(st);
        }
        crate::startup::wait_for_ollama(crate::startup::CLEANUP_WAITS).await;
    }
    let providers: Vec<_> = chain.iter().filter_map(|p| a.llm_provider(p).map(|x| (p.clone(), x))).collect();
    let dict = app_cleanup::relevant_terms(raw, &s.dictionary, 40);
    let full_ctx = cleanup_context(s, target, ctx);
    let reqs: Vec<_> = providers
        .iter()
        .map(|(cfg, _)| {
            let c = if context_allowed(s, cfg, rule_ctx) {
                full_ctx.clone()
            } else {
                CleanupContext { app_category: full_ctx.app_category.clone(), ..Default::default() }
            };
            app_cleanup::build_cleanup_request(level, &s.standing_instruction, &c, &dict, raw)
        })
        .collect();
    let stats = a.stats.lock().unwrap().clone();
    let warm = a.llm_warm_ms.lock().unwrap().clone();
    let timeouts: Vec<Duration> =
        providers.iter().zip(&reqs).map(|((cfg, p), req)| cleanup_timeout(s, &stats, &warm, cfg, p.as_ref(), req)).collect();
    let cfgs: Vec<ProviderConfig> = providers.iter().map(|(c, _)| c.clone()).collect();
    // Say so on the pill well before the budget runs out, not exactly at a fixed 2.5 s.
    let notice_after = timeouts.first().map(|t| t.mul_f32(0.6).max(Duration::from_millis(1500))).unwrap_or(Duration::from_millis(2500));
    let notice = providers.first().filter(|(cfg, _)| cfg.is_local()).map(|(cfg, _)| slow_model_notice(short_name(s, &cfg.id), notice_after));
    // A longer reply takes longer; "slow" scales with the output budget.
    let floor = Duration::from_millis(1200 + reqs.first().map(|r| r.max_tokens as u64 * 8).unwrap_or(0));
    let t0 = Instant::now();
    let outcome = run_chain(
        providers.len(),
        |i| hedge_policy(s, &stats, &cfgs, i, floor),
        |i| timeouts[i],
        |i| {
            let p = providers[i].1.clone();
            let r = reqs[i].clone();
            async move { p.complete(&r).await }
        },
    )
    .await;
    if let Some(n) = notice {
        n.abort();
    }
    timings.cleanup_ms = Some(t0.elapsed().as_millis() as u64);
    for at in &outcome.attempts {
        record_attempt(a, &providers[at.index].0.id, at);
        let cfg = &providers[at.index].0;
        if cfg.ptype == ProviderType::Ollama && matches!(at.error, Some(ProviderError::Offline(_))) {
            crate::startup::heal(cfg);
        }
    }
    match outcome.result {
        Ok((i, resp)) => {
            if providers[i].0.ptype == ProviderType::Ollama {
                // Ollama answered a real request: it is up, and as the primary its model is warm.
                crate::readiness::set_ollama(crate::readiness::Stage::Ready);
                if i == 0 {
                    crate::readiness::set_cleanup(crate::readiness::Stage::Ready);
                }
            }
            timings.cleanup_provider = Some(providers[i].0.id.clone());
            timings.cleanup_model = Some(resp.model.clone());
            let Some(parsed) = app_cleanup::parse_cleanup(&resp.text) else {
                timings.cleanup_rejected = Some("unreadable reply".into());
                return fallback("The cleanup reply could not be read.", timings);
            };
            match app_cleanup::check_cleanup(raw, &parsed.text, level, &dict) {
                Ok(()) => CleanupResult { text: parsed.text.trim().to_string(), raw_used: false, reason: None },
                Err(rej) => {
                    timings.cleanup_rejected = Some(format!("{rej:?}"));
                    let r = fallback(&rej.explain(), timings);
                    timings.cleanup_skipped = None;
                    r
                }
            }
        }
        Err(_) => {
            let reason = cleanup_failure(s, &providers, &timeouts, &outcome.attempts);
            fallback(&reason, timings)
        }
    }
}

/// How long cleanup waits for a provider: the user's limit if set; otherwise
/// on-device models get a budget from their measured speed and the request size,
/// cloud providers one from the request size.
fn cleanup_timeout(
    s: &Settings,
    stats: &ProviderStats,
    warm: &HashMap<String, (String, u64)>,
    cfg: &ProviderConfig,
    p: &dyn LlmProvider,
    req: &app_core::LlmRequest,
) -> Duration {
    if let Some(ms) = s.cleanup_wait_ms {
        return Duration::from_millis(ms);
    }
    if !cfg.is_local() {
        return p.timeout(req);
    }
    let warm_ms = warm.get(&cfg.id).filter(|(model, _)| *model == cfg.model).map(|(_, ms)| *ms);
    stats.local_cleanup_timeout(&cfg.id, warm_ms, req.max_tokens)
}

/// A provider that was still running when another one won gave no answer, but how
/// long it had been running is a real (lower-bound) latency sample: that is what
/// makes hedging back off from a provider that has become slow.
fn record_attempt(a: &crate::app::App, id: &str, at: &ChainAttempt) {
    match &at.error {
        Some(ProviderError::Cancelled) => a.record_stat(id, at.latency_ms, true, Some("outpaced".into())),
        Some(e) => a.record_stat(id, at.latency_ms, false, Some(e.short())),
        None => a.record_stat(id, at.latency_ms, true, None),
    }
}

/// After `after` without an answer from an on-device model, say so on the pill.
fn slow_model_notice(name: String, after: Duration) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(after).await;
        let mut st = pill::current();
        if st.kind == "thinking" {
            st.label = Some(format!("Waiting for {name} — local model is slow…"));
            pill::set(st);
        }
    })
}

/// "Ollama (on this computer)" → "Ollama".
fn short_name(s: &Settings, id: &str) -> String {
    display_name(s, id).trim_end_matches(" (on this computer)").to_string()
}

fn secs(d: Duration) -> String {
    let v = d.as_secs_f64();
    if v < 10.0 && v.fract() != 0.0 {
        format!("{v:.1} s")
    } else {
        format!("{v:.0} s")
    }
}

/// Why cleanup failed: the first provider's reason, then the fallbacks briefly.
fn cleanup_failure(s: &Settings, providers: &[(ProviderConfig, Arc<dyn LlmProvider>)], timeouts: &[Duration], attempts: &[ChainAttempt]) -> String {
    let mut failed: Vec<&ChainAttempt> = attempts.iter().filter(|at| at.error.is_some()).collect();
    failed.sort_by_key(|at| at.index);
    let mut parts = Vec::new();
    for (n, at) in failed.iter().enumerate() {
        let cfg = &providers[at.index].0;
        let name = short_name(s, &cfg.id);
        let e = at.error.as_ref().expect("filtered");
        let local = cfg.is_local();
        parts.push(match (n, e) {
            (0, ProviderError::Timeout) if local && s.cleanup_wait_ms.is_some() => {
                format!("{name} ({}) took longer than your {} limit, so cleanup timed out.", cfg.model, secs(timeouts[at.index]))
            }
            (0, ProviderError::Timeout) if local => {
                format!("{name} ({}) took longer than {}, so cleanup timed out. A smaller model would be faster.", cfg.model, secs(timeouts[at.index]))
            }
            (0, ProviderError::Timeout) => format!("{name} didn't answer in time."),
            (0, ProviderError::KeyInvalid) => format!("{name} rejected the key."),
            (0, ProviderError::Offline(_)) if local => format!("{name} isn't running."),
            (0, ProviderError::Offline(_)) => format!("{name} is unreachable."),
            (0, other) => format!("{name}: {other}"),
            (_, ProviderError::Timeout) => format!("{name} didn't answer in time either."),
            (_, ProviderError::Offline(_)) if local => format!("{name} isn't running either."),
            (_, ProviderError::Offline(_)) => format!("{name} is unreachable too."),
            (_, other) => format!("{name} also failed: {other}"),
        });
    }
    if parts.is_empty() {
        "Cleanup failed.".into()
    } else {
        parts.join(" ")
    }
}

fn paste_request(s: &Settings, text: &str, target: Option<&ForegroundInfo>) -> PasteRequest {
    let rule = target.and_then(|t| s.rule_for(&t.app)).cloned();
    let terminal = target.map(|t| app_platform::app_category(&t.app, "") == "terminal").unwrap_or(false);
    PasteRequest {
        text: text.to_string(),
        chord: PasteChord::parse(rule.as_ref().and_then(|r| r.paste_chord.as_deref())),
        type_instead: rule.as_ref().and_then(|r| r.insert_mode) == Some(app_core::InsertMode::Type),
        restore_clipboard: !s.keep_on_clipboard,
        restore_delay: Duration::from_millis(rule.and_then(|r| r.paste_delay_ms).unwrap_or(if terminal { 400 } else { 250 })),
        target_pid: target.map(|t| t.pid).filter(|p| *p != 0),
    }
}

async fn do_paste(req: PasteRequest) -> PasteOutcome {
    tokio::task::spawn_blocking(move || app_platform::paste(&req)).await.unwrap_or(PasteOutcome::Failed("paste task failed".into()))
}

fn hold_text(rec_id: &str, text: &str, label: &str, detail: Option<String>) {
    let a = app();
    *a.held.lock().unwrap() = Some((rec_id.to_string(), text.to_string()));
    app_platform::set_clipboard_text(text);
    pill::flash(
        PillState {
            kind: "held".into(),
            label: Some(label.into()),
            detail,
            action: Some(PillAction { id: "paste_held".into(), label: "Paste".into() }),
            ..Default::default()
        },
        6000,
    );
}

async fn process(mut job: Job) {
    let a = app();
    let s = a.settings();
    let mut timings = Timings { hold_ms: job.hold_ms, ..Default::default() };
    let Some(take) = job.take.take() else {
        pill::flash(PillState { kind: "error".into(), label: Some("Nothing was recorded".into()), ..Default::default() }, 3000);
        return;
    };
    timings.audio_ms = take.audio_ms();
    timings.speech_ms = take.speech_ms();
    timings.preroll_used_ms = (take.preroll_samples * 1000 / 16_000) as u64;

    let mut rec = match &job.from_history {
        Some(id) => a.with_history(|h| h.get(id)).flatten().unwrap_or_default(),
        None => HistoryRecord {
            id: job.uid.clone(),
            created_at: now_ms(),
            status: Status::Unprocessed,
            mode: if job.mode == Mode::Command { "command".into() } else { "dictation".into() },
            app: job.target.as_ref().map(|t| t.app.clone()),
            window_title: job.target.as_ref().map(|t| t.title.clone()),
            audio_path: keep_audio(&take),
            audio_ms: take.audio_ms(),
            ..Default::default()
        },
    };
    if job.from_history.is_none() {
        save(&rec); // audio is safe before any network call completes
    }

    if !take.has_speech() {
        // A muted mic, a blocked one (Windows privacy settings) or the wrong input
        // sends digital silence; say so instead of blaming the speaker.
        let silent = take.max_db < SILENT_DB;
        tracing::info!(max_db = take.max_db, vad = take.vad, device = %take.device, silent, "no speech detected");
        rec.status = Status::NothingHeard;
        rec.timings = timings;
        save(&rec);
        let (label, ms) = if silent {
            let device = Some(crate::controller::short_device(&take.device)).filter(|d| !d.is_empty());
            (format!("Can't hear you — check {}", device.as_deref().unwrap_or("the microphone")), 4000)
        } else {
            ("Didn't hear anything".to_string(), 1400)
        };
        pill::flash(PillState { kind: "nothing".into(), label: Some(label), ..Default::default() }, ms);
        return;
    }

    let ctx = wait_context(&job.context, Duration::from_millis(80)).await;
    if let Some(c) = &ctx {
        timings.context_ms = Some(c.ms);
        timings.context_timed_out = c.timed_out;
    }
    let rule = job.target.as_ref().and_then(|t| s.rule_for(&t.app)).cloned();
    let rule_ctx = rule.as_ref().and_then(|r| r.context).unwrap_or(true);
    if s.context.enabled && rule_ctx {
        rec.url = ctx.as_ref().and_then(|c| c.url.clone());
        rec.text_before = ctx.as_ref().and_then(|c| c.before.clone());
        rec.selection = ctx.as_ref().and_then(|c| c.selection.clone());
    }

    // ---- transcription ----
    let pcm = Arc::new(take.trimmed().to_vec());
    let raw = match transcribe(&s, pcm, ctx.as_ref(), rule_ctx, &mut timings).await {
        Ok(t) => t,
        // Every provider came back empty: nothing heard after all.
        Err((ProviderError::BadResponse(m), _)) if m == EMPTY_TRANSCRIPT => String::new(),
        Err((e, provider)) => {
            rec.status = Status::Failed;
            rec.error = Some(e.to_string());
            rec.timings = timings;
            save(&rec);
            sound(SoundKind::Error);
            let (label, action) = error_copy(&e, &provider, rec.audio_path.is_some());
            pill::flash(PillState { kind: "error".into(), label: Some(label), action, ..Default::default() }, 8000);
            pill::set_badge(Some("error"), Some("Last dictation failed — see History"));
            return;
        }
    };
    if pill::current().badge.as_deref() == Some("error") {
        pill::set_badge(None, None);
    }
    rec.raw_text = Some(raw.clone());
    if raw.trim().is_empty() || app_cleanup::words::tokens(&raw).iter().all(|t| app_cleanup::words::is_filler(&t.to_lowercase())) {
        rec.status = Status::NothingHeard;
        rec.timings = timings;
        save(&rec);
        pill::flash(PillState { kind: "nothing".into(), label: Some("Didn't hear anything".into()), ..Default::default() }, 1400);
        return;
    }

    if job.mode == Mode::Command {
        command(job, rec, raw, ctx, timings).await;
        return;
    }

    // ---- snippets / cleanup ----
    let vars = SnippetVars {
        clipboard: app_platform::read_clipboard_text(),
        selection: ctx.as_ref().and_then(|c| c.selection.clone()),
        app: job.target.as_ref().map(|t| t.app.clone()),
    };
    let (snip, whole) = snippets::expand(&raw, &s.snippets, &vars);
    let level = rule.as_ref().and_then(|r| r.cleanup).unwrap_or(s.cleanup_level);
    rec.cleanup_level = Some(format!("{level:?}").to_lowercase());
    let (mut final_text, raw_used, reason) = if whole {
        timings.cleanup_skipped = Some("snippet".into());
        (snip, false, None)
    } else {
        let c = cleanup(&s, &raw, level, ctx.as_ref(), job.target.as_ref(), rule_ctx, &mut timings).await;
        rec.cleaned_text = Some(c.text.clone());
        let t = app_cleanup::apply_dictionary(&c.text, &s.dictionary);
        let (t, _) = snippets::expand(&t, &s.snippets, &vars);
        (t, c.raw_used, c.reason)
    };
    if rules::leading_space(ctx.as_ref().and_then(|c| c.before.as_deref()), &final_text) {
        final_text.insert(0, ' ');
    }
    rec.final_text = Some(final_text.clone());

    // ---- re-processing from History: store, don't paste ----
    if job.from_history.is_some() {
        rec.status = Status::Held;
        rec.timings = timings;
        save(&rec);
        pill::flash(PillState { kind: "notice".into(), label: Some("Done — the new text is in History".into()), ..Default::default() }, 2500);
        return;
    }

    // Undo restores what was heard, with the same local rules a cleanup-off paste gets.
    let undo_text = rules::apply(&raw, RuleOptions { remove_fillers: false, ..Default::default() });
    insert(job.target, ctx, rec, final_text, undo_text, raw_used, reason, timings, job.key_up).await;
}

#[allow(clippy::too_many_arguments)]
async fn insert(
    target: Option<ForegroundInfo>,
    ctx: Option<ContextSnapshot>,
    mut rec: HistoryRecord,
    final_text: String,
    // What the undo chord puts back: the raw transcript, or a command's original selection.
    undo_text: String,
    raw_used: bool,
    reason: Option<String>,
    mut timings: Timings,
    key_up: Instant,
) {
    let a = app();
    let s = a.settings();
    let now_fg = app_platform::foreground();
    let terminal = target.as_ref().map(|t| app_platform::app_category(&t.app, "") == "terminal").unwrap_or(false);
    let same = match (&target, &now_fg) {
        (Some(t), Some(n)) => t.same_target(n, terminal),
        (None, _) => true, // started over our own window or unknown: paste where the focus is
        _ => false,
    };
    let finish = |rec: &mut HistoryRecord, timings: Timings, status: Status| {
        rec.status = status;
        rec.timings = timings;
        save(rec);
    };
    if now_fg.as_ref().is_some_and(|f| f.is_self) {
        // Never into our own window (macOS sees the chord there).
        timings.total_ms = Some(key_up.elapsed().as_millis() as u64);
        finish(&mut rec, timings, Status::Held);
        hold_text(&rec.id, final_text.trim_start(), "Kept your text", Some("Dettino doesn't paste into its own window".into()));
        return;
    }
    if !same {
        timings.total_ms = Some(key_up.elapsed().as_millis() as u64);
        finish(&mut rec, timings, Status::Held);
        hold_text(&rec.id, final_text.trim_start(), "Kept your text", Some("The cursor moved before we could paste".into()));
        return;
    }
    if now_fg.as_ref().map(|f| f.blocked_elevated).unwrap_or(false) {
        finish(&mut rec, timings, Status::Held);
        let app_name = now_fg.as_ref().map(|f| f.display_name()).unwrap_or_default();
        hold_text(&rec.id, final_text.trim_start(), &format!("Can't type into {app_name} (runs as admin)"), Some("Copied to your clipboard instead".into()));
        return;
    }
    if ctx.as_ref().map(|c| c.is_password).unwrap_or(false) {
        finish(&mut rec, timings, Status::Held);
        hold_text(&rec.id, final_text.trim_start(), "Not pasted into a password field", Some("Copied to your clipboard instead".into()));
        return;
    }

    let t0 = Instant::now();
    let outcome = do_paste(paste_request(&s, &final_text, now_fg.as_ref())).await;
    timings.insert_ms = Some(t0.elapsed().as_millis() as u64);
    timings.total_ms = Some(key_up.elapsed().as_millis() as u64);
    let provider_note = timings.asr_attempts.len() > 1 || timings.asr_hedged;
    match outcome {
        PasteOutcome::Verified { .. } | PasteOutcome::Typed => {
            timings.insert_verify = Some(match &outcome { PasteOutcome::Typed => "typed".into(), _ => "clipboard read".into() });
            if let PasteOutcome::Verified { restored } = &outcome {
                timings.clipboard_restored = *restored;
            }
            let status = if raw_used { Status::OkRaw } else { Status::Ok };
            finish(&mut rec, timings.clone(), status);
            *a.last_paste.lock().unwrap() = Some(LastPaste {
                record_id: rec.id.clone(),
                raw: undo_text,
                final_text: final_text.trim_start().to_string(),
                showing_raw: false,
                at: Instant::now(),
                user_keys: app_hotkey::user_key_counter(),
            });
            let undo_until = Some(now_ms() + 60_000);
            let via = if provider_note {
                timings.asr_provider.as_ref().map(|p| format!("via {}", display_name(&s, p)))
            } else {
                None
            };
            if raw_used {
                pill::flash(
                    PillState {
                        kind: "pasted_raw".into(),
                        label: Some("Pasted as heard — AI skipped".into()),
                        detail: reason.clone(),
                        action: Some(PillAction { id: format!("why:{}", rec.id), label: "Why?".into() }),
                        undo_until_ms: undo_until,
                        ..Default::default()
                    },
                    2800,
                );
            } else {
                pill::flash(
                    PillState { kind: "pasted".into(), label: Some("Pasted".into()), detail: via, undo_until_ms: undo_until, ..Default::default() },
                    if provider_note { 1800 } else { 900 },
                );
            }
            if timings.clipboard_restored == Some(false) && !s.keep_on_clipboard {
                tracing::info!("clipboard left as is (changed during paste or not restorable)");
            }
            schedule_learn(rec.id.clone(), final_text.trim_start().to_string(), now_fg);
        }
        PasteOutcome::Unverified => {
            timings.insert_verify = Some("unverified".into());
            finish(&mut rec, timings, Status::Held);
            *a.held.lock().unwrap() = Some((rec.id.clone(), final_text.trim_start().to_string()));
            pill::flash(
                PillState {
                    kind: "held".into(),
                    label: Some("Pasted? Couldn't confirm".into()),
                    detail: Some(format!("The text is on your clipboard — press {}", crate::PASTE_KEYS)),
                    action: Some(PillAction { id: "paste_held".into(), label: "Paste again".into() }),
                    ..Default::default()
                },
                6000,
            );
        }
        PasteOutcome::Failed(e) => {
            rec.error = Some(e.clone());
            finish(&mut rec, timings, Status::Held);
            hold_text(&rec.id, final_text.trim_start(), "Kept your text", Some(e));
        }
    }
}

async fn command(job: Job, mut rec: HistoryRecord, instruction: String, ctx: Option<ContextSnapshot>, mut timings: Timings) {
    let a = app();
    let s = a.settings();
    let mut selection = ctx.as_ref().and_then(|c| c.selection.clone()).filter(|t| !t.trim().is_empty());
    let terminal = job.target.as_ref().map(|t| app_platform::app_category(&t.app, "") == "terminal").unwrap_or(false);
    // Ctrl+C with nothing selected interrupts whatever runs in a terminal.
    if selection.is_none() && !terminal {
        selection = tokio::task::spawn_blocking(app_platform::copy_selection).await.ok().flatten();
    }
    let Some(selection) = selection else {
        rec.status = Status::Failed;
        rec.error = Some("No text was selected".into());
        rec.timings = timings;
        save(&rec);
        pill::flash(PillState { kind: "notice".into(), label: Some("Select some text first".into()), ..Default::default() }, 2200);
        return;
    };
    rec.selection = Some(selection.clone());
    let fail = |rec: &mut HistoryRecord, why: &str| {
        rec.status = Status::Failed;
        rec.error = Some(why.into());
        rec.timings = timings.clone();
        save(rec);
    };
    if selection.split_whitespace().count() > 2000 {
        fail(&mut rec, "The selection is longer than 2,000 words");
        pill::flash(PillState { kind: "notice".into(), label: Some("That's a lot — select up to 2,000 words".into()), ..Default::default() }, 3000);
        return;
    }
    let chain = a.chain(&s.llm_chain);
    let providers: Vec<_> = chain.iter().filter_map(|p| a.llm_provider(p).map(|x| (p.clone(), x))).collect();
    if providers.is_empty() {
        fail(&mut rec, "No cleanup provider is set up");
        pill::flash(
            PillState { kind: "error".into(), label: Some("Editing needs a cleanup provider".into()), action: Some(PillAction { id: "open:providers".into(), label: "Set up".into() }), ..Default::default() },
            5000,
        );
        return;
    }
    pill::set(PillState { kind: "thinking".into(), mode: "command".into(), label: Some("Applying…".into()), ..Default::default() });
    let req = app_cleanup::build_command_request(&selection, &instruction);
    let t0 = Instant::now();
    let outcome = run_chain(
        providers.len(),
        |_| Some(Duration::from_millis(6000)),
        |i| providers[i].1.timeout(&req).max(Duration::from_secs(20)),
        |i| {
            let p = providers[i].1.clone();
            let r = req.clone();
            async move { p.complete(&r).await }
        },
    )
    .await;
    timings.cleanup_ms = Some(t0.elapsed().as_millis() as u64);
    let replacement = match outcome.result {
        Ok((i, resp)) => {
            timings.cleanup_provider = Some(providers[i].0.id.clone());
            app_cleanup::parse_command(&resp.text)
        }
        Err(e) => {
            rec.status = Status::Failed;
            rec.error = Some(e.to_string());
            rec.timings = timings;
            save(&rec);
            pill::flash(PillState { kind: "error".into(), label: Some("Couldn't apply the edit".into()), detail: Some(e.to_string()), ..Default::default() }, 5000);
            return;
        }
    };
    let Some(replacement) = replacement.filter(|r| app_cleanup::check_command(&selection, r).is_ok()) else {
        rec.status = Status::Failed;
        rec.error = Some(Rejection::Empty.explain());
        rec.timings = timings;
        save(&rec);
        pill::flash(PillState { kind: "error".into(), label: Some("The edit came back empty — nothing changed".into()), ..Default::default() }, 4000);
        return;
    };
    rec.raw_text = Some(instruction);
    rec.final_text = Some(replacement.clone());
    // Undo for an edit restores the original selection, exactly as it was.
    insert(job.target, ctx, rec, replacement, selection, false, None, timings, job.key_up).await;
}

fn schedule_learn(rec_id: String, pasted: String, target: Option<ForegroundInfo>) {
    let a = app();
    let s = a.settings();
    if !s.learn_from_corrections || !s.context.enabled {
        return;
    }
    let Some(target) = target else { return };
    if app_platform::app_category(&target.app, "") == "terminal" {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(8));
        let Some(now) = app_platform::foreground() else { return };
        if !now.same_target(&target, false) {
            return;
        }
        let Some(field) = app_platform::read_focused_text(Duration::from_millis(400)) else { return };
        let Some(span) = app_cleanup::learn::edited_span(&pasted, &field) else { return };
        let a = app();
        let zero = span.trim() == pasted.trim();
        a.with_history(|h| {
            if let Some(mut r) = h.get(&rec_id) {
                r.edited_text = Some(span.clone());
                r.zero_edit = Some(zero);
                let days = a.settings.read().unwrap().audio_retention_days;
                let _ = h.upsert(&r, days);
            }
        });
        if zero {
            return;
        }
        let found = app_cleanup::learn::suggest(&pasted, &span);
        if found.is_empty() {
            return;
        }
        let dict: Vec<String> = a.settings.read().unwrap().dictionary.iter().map(|d| d.term.to_lowercase()).collect();
        a.with_history(|h| {
            let mut list: Vec<Suggestion> = h.kv_get("suggestions").unwrap_or_default();
            for f in found {
                if dict.contains(&f.to.to_lowercase()) || list.iter().any(|x| x.to == f.to) {
                    continue;
                }
                list.push(Suggestion { from: f.from, to: f.to, ts: now_ms() });
            }
            let _ = h.kv_set("suggestions", &list);
        });
        let _ = a.handle.emit("dictionary:suggestions", ());
        emit_history_changed();
    });
}

/// Keep a cancelled take in History for Restore. Returns whether it was kept.
pub fn store_cancelled(info: &SessionInfo, take: Option<Take>) -> bool {
    let a = app();
    // Never leave an older recording as the thing Restore would bring back.
    *a.cancelled.lock().unwrap() = None;
    let Some(take) = take else { return false };
    let rec = HistoryRecord {
        id: info.uid.clone(),
        created_at: now_ms(),
        status: Status::Cancelled,
        mode: if info.mode == Mode::Command { "command".into() } else { "dictation".into() },
        app: info.target.as_ref().map(|t| t.app.clone()),
        window_title: info.target.as_ref().map(|t| t.title.clone()),
        audio_path: keep_audio(&take),
        audio_ms: take.audio_ms(),
        ..Default::default()
    };
    if rec.audio_path.is_none() {
        return false;
    }
    save(&rec);
    *a.cancelled.lock().unwrap() = Some(rec.id.clone());
    true
}

/// Build a job from a stored recording (restore after Esc, retry after failure).
pub fn job_from_history(id: &str, paste: bool) -> Option<Job> {
    let a = app();
    let (rec, path) = a.with_history(|h| h.get(id).map(|r| (r.clone(), h.audio_file(&r)))).flatten()?;
    let path = path?;
    let pcm = app_audio::wav::read(&path).ok()?;
    let take = Take {
        session: 0,
        preroll_samples: 0,
        speech_start: Some(0),
        speech_end: Some(pcm.len()),
        max_db: -20.0,
        wav_path: path,
        device: String::new(),
        vad: "stored",
        pcm,
    };
    Some(Job {
        uid: rec.id.clone(),
        mode: if rec.mode == "command" { Mode::Command } else { Mode::Dictation },
        take: Some(take),
        target: if paste { app_platform::foreground().filter(|f| !f.is_self) } else { None },
        context: Arc::new(Mutex::new(Some(ContextSnapshot::default()))),
        hold_ms: rec.audio_ms,
        key_up: Instant::now(),
        from_history: if paste { None } else { Some(rec.id) },
    })
}

fn paste_now(text: String) {
    let a = app();
    let s = a.settings();
    let fg = app_platform::foreground().filter(|f| !f.is_self);
    let req = paste_request(&s, &text, fg.as_ref());
    match app_platform::paste(&req) {
        PasteOutcome::Verified { .. } | PasteOutcome::Typed => pill::flash(PillState { kind: "pasted".into(), label: Some("Pasted".into()), ..Default::default() }, 900),
        _ => pill::flash(PillState { kind: "held".into(), label: Some(format!("On your clipboard — press {}", crate::PASTE_KEYS)), ..Default::default() }, 3000),
    }
}

pub fn paste_last() {
    let a = app();
    let held = a.held.lock().unwrap().take();
    let text = held.map(|h| h.1).or_else(|| a.with_history(|h| h.latest_pasted()).flatten().and_then(|r| r.final_text));
    match text {
        Some(t) => paste_now(t.trim_start().to_string()),
        None => pill::flash(PillState { kind: "notice".into(), label: Some("Nothing to paste yet".into()), ..Default::default() }, 1500),
    }
}

pub fn undo_ai() {
    let a = app();
    let mut guard = a.last_paste.lock().unwrap();
    let Some(lp) = guard.as_mut() else {
        drop(guard);
        pill::flash(PillState { kind: "notice".into(), label: Some("Nothing to undo".into()), ..Default::default() }, 1500);
        return;
    };
    if lp.at.elapsed() > Duration::from_secs(60) || app_hotkey::user_key_counter() != lp.user_keys {
        let raw = lp.raw.clone();
        drop(guard);
        app_platform::set_clipboard_text(&raw);
        pill::flash(PillState { kind: "notice".into(), label: Some("Raw text copied — paste it where you need it".into()), ..Default::default() }, 2500);
        return;
    }
    let (current, other) = if lp.showing_raw { (lp.raw.clone(), lp.final_text.clone()) } else { (lp.final_text.clone(), lp.raw.clone()) };
    let to_raw = !lp.showing_raw;
    drop(guard);
    if !app_platform::select_recent_text(&current, Duration::from_millis(500)) {
        app_platform::set_clipboard_text(&other);
        pill::flash(PillState { kind: "notice".into(), label: Some("Can't undo here — raw text copied".into()), ..Default::default() }, 2500);
        return;
    }
    let s = a.settings();
    let fg = app_platform::foreground();
    let req = paste_request(&s, &other, fg.as_ref());
    let ok = matches!(app_platform::paste(&req), PasteOutcome::Verified { .. } | PasteOutcome::Typed);
    if ok {
        let mut rid = None;
        if let Some(lp) = a.last_paste.lock().unwrap().as_mut() {
            lp.showing_raw = to_raw;
            lp.at = Instant::now();
            lp.user_keys = app_hotkey::user_key_counter();
            rid = Some(lp.record_id.clone());
        }
        // History shows what is actually in the field now.
        if let Some(id) = rid {
            if let Some(Some(mut r)) = a.with_history(|h| h.get(&id)) {
                r.final_text = Some(other.clone());
                r.status = if to_raw { Status::OkRaw } else { Status::Ok };
                save(&r);
            }
        }
        pill::flash(
            PillState {
                kind: "pasted".into(),
                label: Some(if to_raw { "Raw version pasted".into() } else { "Cleaned version pasted".into() }),
                undo_until_ms: Some(now_ms() + 60_000),
                ..Default::default()
            },
            1500,
        );
    }
}

pub fn pill_action(id: &str) {
    let a = app();
    let open = |section: &str| crate::commands::show_main(Some(section.to_string()));
    match id {
        "paste_held" => {
            if let Some((_, text)) = a.held.lock().unwrap().clone() {
                std::thread::spawn(move || paste_now(text));
            }
        }
        "restore" => {
            if let Some(rid) = a.cancelled.lock().unwrap().take() {
                if let Some(job) = job_from_history(&rid, true) {
                    pill::set(PillState { kind: "thinking".into(), ..Default::default() });
                    enqueue(job);
                }
            }
        }
        "retry" => {
            let last_failed = a.with_history(|h| h.list(5, None).into_iter().find(|r| r.status == Status::Failed)).flatten();
            if let Some(r) = last_failed {
                if let Some(job) = job_from_history(&r.id, true) {
                    pill::set(PillState { kind: "thinking".into(), ..Default::default() });
                    enqueue(job);
                }
            }
        }
        "undo" => {
            std::thread::spawn(undo_ai);
        }
        "paste_last" => {
            std::thread::spawn(paste_last);
        }
        "pause" => {
            crate::commands::toggle_pause();
        }
        "settings" => open("home"),
        _ if id.starts_with("open:") => open(&id[5..]),
        _ if id.starts_with("why:") => open(&format!("history:{}", &id[4..])),
        _ if id.starts_with("allow:") => {
            a.allow_until.lock().unwrap().insert(id[6..].to_string(), Instant::now() + Duration::from_secs(300));
            pill::flash(PillState { kind: "notice".into(), label: Some("Allowed for 5 minutes".into()), ..Default::default() }, 1500);
        }
        _ => tracing::warn!("unknown pill action {id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fg(app: &str, title: &str) -> ForegroundInfo {
        ForegroundInfo { app: app.into(), title: title.into(), ..Default::default() }
    }

    #[test]
    fn privacy_switches_gate_app_name_and_window_title() {
        let mut s = Settings::default();
        let target = fg("slack.exe", "DM with Jane");
        let c = cleanup_context(&s, Some(&target), None);
        assert_eq!(c.app_name.as_deref(), Some("slack.exe"));
        assert_eq!(c.window_title.as_deref(), Some("DM with Jane"));
        s.context.window_title = false;
        let c = cleanup_context(&s, Some(&target), None);
        assert_eq!(c.app_name.as_deref(), Some("slack.exe"));
        assert_eq!(c.window_title, None);
        s.context.app_name = false;
        let c = cleanup_context(&s, Some(&target), None);
        assert_eq!(c.app_name, None);
        assert_eq!(c.app_category.as_deref(), Some("chat"), "the tone hint stays");
    }

    #[test]
    fn cloud_providers_behind_a_local_one_are_failover_only() {
        let s = Settings::default();
        let stats = ProviderStats::default();
        let by_id = |id: &str| s.provider(id).cloned().unwrap();
        let balanced = [by_id("parakeet"), by_id("whisper"), by_id("openai-asr")];
        let floor = Duration::from_millis(1200);
        assert!(hedge_policy(&s, &stats, &balanced, 0, floor).is_some(), "local → local hedges");
        assert_eq!(hedge_policy(&s, &stats, &balanced, 1, floor), None, "local → cloud never hedges");
        assert_eq!(hedge_policy(&s, &stats, &balanced, 2, floor), None, "nothing after the last one");
        let only_cloud_after = [by_id("parakeet"), by_id("openai-asr")];
        assert_eq!(hedge_policy(&s, &stats, &only_cloud_after, 0, floor), None);
        let fast = [by_id("openai-asr"), by_id("custom-asr")];
        assert!(hedge_policy(&s, &stats, &fast, 0, floor).is_some(), "cloud → cloud hedges");
        let mut lan = by_id("custom-asr");
        lan.ptype = ProviderType::OpenaiCompatAsr;
        lan.treat_as_local = true;
        assert!(hedge_policy(&s, &stats, &[by_id("parakeet"), lan], 0, floor).is_some(), "a server marked local counts as local");
    }

    /// Two providers whose answers are fixed, wrapped the way `transcribe` wraps them.
    async fn chain_of(answers: [&'static str; 2], expect_text: bool) -> Result<(usize, String), ProviderError> {
        let outcome = run_chain(
            2,
            |_| None,
            |_| Duration::from_secs(1),
            |i| async move { require_text(answers[i].to_string(), expect_text) },
        )
        .await;
        outcome.result
    }

    #[tokio::test]
    async fn an_empty_transcript_of_speech_falls_through_to_the_next_provider() {
        assert_eq!(chain_of(["", "hello there"], true).await.unwrap(), (1, "hello there".to_string()));
        assert_eq!(chain_of(["hi", "unused"], true).await.unwrap(), (0, "hi".to_string()));
        // Everyone empty: the error `process` turns back into "Didn't hear anything".
        assert!(matches!(chain_of(["", " "], true).await, Err(ProviderError::BadResponse(m)) if m == EMPTY_TRANSCRIPT));
        // Little speech: an empty answer is believed, as before.
        assert_eq!(chain_of(["", "unused"], false).await.unwrap(), (0, String::new()));
    }
}
