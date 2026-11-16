<p align="center">
  <img src="site/icon.svg" width="112" height="112" alt="Dettino logo: a lowercase d whose bowl is a speech bubble">
</p>

<h1 align="center">Dettino</h1>

<p align="center"><strong>Hold a key, speak, release. Your words are typed wherever your cursor is.</strong></p>

<p align="center">Windows 11 · macOS (Apple Silicon) · on-device or your own API keys</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-2EC4A0" alt="MIT license"></a>
  <a href="https://buymeacoffee.com/hubertmk"><img src="https://img.shields.io/badge/Buy%20me%20a%20coffee-%E2%98%95-ffdd00?labelColor=1f1f1f" alt="Buy me a coffee"></a>
</p>

---

**Nothing here is new, and that's okay.** Hold-to-talk dictation with AI cleanup is an idea other apps had first, and they do it well. Dettino is my learning and assessment project. I wanted to understand how a tool like this works from the inside, and to find out for myself how good dictation can get when everything runs on my own machine, with or without an internet connection. The answer so far: surprisingly good, even with local models only.

I could only build it because so many people did the hard work and shared it. Thank you to:

- NVIDIA for [Parakeet](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2) and OpenAI for [Whisper](https://github.com/openai/whisper), which turn speech into text
- the [Silero](https://github.com/snakers4/silero-vad) team, for knowing when someone is speaking
- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) and [ONNX Runtime](https://github.com/microsoft/onnxruntime), which make running these models on an ordinary laptop easy
- [Ollama](https://ollama.com) and Google's [Gemma](https://ai.google.dev/gemma), for text cleanup that never leaves your computer
- [Tauri](https://tauri.app), the Rust and React communities, and the many open-source maintainers listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)

Dettino is mostly me connecting their pieces and learning a lot along the way. It's still one person's tool, so expect some rough edges.

You choose who hears you and who polishes the text: fully on-device models, your own cloud API keys, or a mix, with automatic fallback.

## Status

| Area | Windows 11 | macOS (Apple Silicon) |
|---|---|---|
| Global shortcuts (hold / tap pass-through / hands-free / command / undo / paste-last) | ✅ | ✅ CGEventTap (needs Accessibility) |
| Warm microphone with 1.5 s pre-roll, Silero VAD, crash-safe WAV | ✅ | ✅ (cpal) |
| On-device transcription (Parakeet, Whisper via sherpa-onnx) | ✅ CPU | ✅ CPU |
| Cloud transcription (OpenAI, any OpenAI-compatible server) | ✅ | ✅ |
| Cleanup (Anthropic, OpenAI, Perplexity Agent API, Ollama, LM Studio / any OpenAI-compatible) | ✅ | ✅ |
| Guided local setup: installs Ollama, picks and pulls a cleanup model for the hardware | ✅ | ✅ (Ollama.app into /Applications) |
| Screen context, verified clipboard paste, typing mode | ✅ UI Automation | ✅ Accessibility, NSPasteboard promise |
| Pill, onboarding, settings, history, dictionary, snippets, app rules | ✅ | ✅ (pill is a non-activating NSPanel) |

## Quick start (Windows)

Prerequisites:

```powershell
winget install Rustlang.Rustup            # stable MSVC toolchain
winget install OpenJS.NodeJS.LTS          # then: corepack enable
winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
python -m pip install --user libclang     # libclang.dll for bindgen (or install LLVM)
```

Point bindgen at libclang once per machine in `.cargo/config.toml` (git-ignored):

```toml
[env]
LIBCLANG_PATH = 'C:\Users\<you>\AppData\Roaming\Python\Python311\site-packages\clang\native'
```

Run:

```powershell
git clone https://github.com/hubertmk/dettino.git
cd dettino/apps/desktop
pnpm install
pnpm tauri dev          # development, hot reload
pnpm build             # installer (with the ONNX runtime DLLs) in target/release/bundle/nsis/
pnpm build --expiring  # the same, but it stops working after 60 days (see "End date for test builds")
```

## Quick start (macOS)

Prerequisites: Xcode or the Command Line Tools (`xcode-select --install`), then:

```sh
brew install rustup node pnpm && rustup-init -y
```

Run:

```sh
git clone https://github.com/hubertmk/dettino.git
cd dettino/apps/desktop
pnpm install
pnpm build            # signed Dettino.app and DMG in target/release/bundle/{macos,dmg}/
open ../../target/release/bundle/macos/Dettino.app
```

Dettino needs three permissions, which the second onboarding step asks for with live status: **Microphone**, **Accessibility** (the shortcut, pasting and screen context) and **Input Monitoring** (some macOS versions want it for keyboard taps). macOS ties these to the app's signature. `pnpm build` therefore signs with the first *Developer ID Application* identity in your keychain, so permissions survive rebuilds. Override it with `APPLE_SIGNING_IDENTITY`. Without a certificate it signs ad hoc, and you have to switch Accessibility off and on again after every build.

`pnpm tauri dev` works too, but macOS then attributes the permissions to the terminal or editor that launched it (grant Accessibility to VS Code or Terminal). Use the built app to try the real permission flow.

**Release builds.** Notarization runs when the Apple credentials are in the environment, either `APPLE_API_KEY`, `APPLE_API_ISSUER` and `APPLE_API_KEY_PATH` (an App Store Connect API key), or `APPLE_ID`, `APPLE_PASSWORD` (an app-specific password) and `APPLE_TEAM_ID`. Without them the DMG is signed but not notarized: it runs on your Mac, but on other Macs Gatekeeper blocks it until the tester right-clicks → Open (or allows it under Privacy & Security).

Default shortcuts are the same keys as on Windows: hold **Control + Space** to dictate. With more than one input source, macOS also uses Control + Space to switch sources: a tap still switches, a hold dictates. Data lives in `~/Library/Application Support/dev.dettino.desktop`, keys in the macOS Keychain.

## First launch

On first launch the five-step onboarding wizard sets up the mode you pick. **Private** (the default) runs everything on the PC: one *Install everything* job downloads the speech model (≈ 480 MB), installs Ollama silently if it is missing, and pulls the cleanup model `gemma4:e2b-it-qat` (the only one offered; E4B was not better on real dictations), with one progress bar and a warning on low disk space or slow CPU-only machines. The downloads start when you arrive on the install step; *Continue* stays locked until everything is installed **and** loaded (the backend's `readiness` reports the speech model warm, Ollama answering and the cleanup model warm), and the wizard then moves on to its *Ready* page by itself. **Cloud** asks for an OpenAI key, which then covers both transcription and cleanup. Keys are stored in Windows Credential Manager or the macOS Keychain. *Finish* closes the window and the pill says *Pronto!*.

On Windows, `pnpm build` produces an unsigned per-user installer, `Dettino_<version>_x64-setup.exe` (≈ 9 MB; see [Versioning and releases](#versioning-and-releases) for the name). It installs program files to `%LOCALAPPDATA%\Dettino`, separate from the data folder.

**Setting it up for someone else?** [`docs/setup-guide.md`](docs/setup-guide.md) is a step-by-step guide for non-developers: which path fits their PC, the SmartScreen warning, every wizard step with screenshots, everyday use, troubleshooting, updating and uninstalling. Send it along with the installer.

While you dictate, a small pill near the bottom of the screen shows what Dettino is doing. Two looks are available under *Shortcuts & cleanup → Feedback → Pill style*: **One line** (default), a line that ripples with your voice and flashes when the text lands, and **Classic**, the dot that grows into a pill. The accent colour is Seafoam by default (the pill still listens in blue and glows seafoam after a paste); *Shortcuts & cleanup → Feedback → Accent colour* offers other palettes and *System*, which follows the Windows or macOS accent.

## Versioning and releases

The version is defined once, as `version` under `[workspace.package]` in the root `Cargo.toml`. Every crate inherits it, Tauri reads it for the installer and the file properties (`tauri.conf.json` and `package.json` carry no version), and the app logs and shows it. Never edit it by hand; `pnpm release` is the one way it changes:

```powershell
cd apps/desktop
pnpm release minor      # or patch / major / 1.2.3
git push --follow-tags
```

`pnpm release` refuses a dirty tree, bumps `Cargo.toml` and `Cargo.lock`, commits `Release vX.Y.Z`, tags `vX.Y.Z` and runs `pnpm build`. If the build fails, it takes the commit and tag back (nothing has been pushed yet), so the number stays free. Once pushed, a tag is never moved.

**Building releases on GitHub.** Pushing a `vX.Y.Z` tag runs [`.github/workflows/release.yml`](.github/workflows/release.yml), which builds the Windows installer and the macOS DMG with the same `pnpm build` and attaches both to a draft GitHub Release for you to review and publish. To leave the build to CI, tag without building locally:

```sh
pnpm release minor --no-build
git push --follow-tags
```

If the release for that tag already exists, for example because you uploaded an installer by hand, the workflow adds its files to it and replaces any with the same name. *Actions → Release → Run workflow* builds the current commit and attaches the installers to a draft pre-release named "Test build" plus the commit; download them from there and delete the draft. (Installers never go through workflow artifacts, so the account's artifact storage quota does not matter.) The macOS job needs the Developer ID certificate as repository secrets (`APPLE_CERTIFICATE`, the base64 of the exported `.p12`, and `APPLE_CERTIFICATE_PASSWORD`) and, for notarization, either `APPLE_API_KEY`, `APPLE_API_ISSUER` and `APPLE_API_KEY_P8` (the contents of the `.p8` file) or `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID`. The DMG is built on Apple Silicon only.

Every build also records the commit it came from. A clean build at its own tag (what `pnpm release` makes) is labelled with the version alone; anything else carries the commit, so two installers of the same version can never be confused:

| Build | Label and installer name |
|---|---|
| `pnpm release` | `0.2.0` → `Dettino_0.2.0_x64-setup.exe` |
| `pnpm build`, clean tree, no tag at HEAD | `0.2.0+a1b2c3d4e` → `Dettino_0.2.0+a1b2c3d4e_x64-setup.exe` |
| `pnpm build` with uncommitted or untracked changes | `0.2.0+a1b2c3d4e.dirty` → `Dettino_0.2.0+a1b2c3d4e.dirty_x64-setup.exe` |
| any of these with `--expiring` or `--expires` | the same label plus the end date, e.g. `Dettino_0.2.0_until-2026-12-29_x64-setup.exe` |

The label is shown at the bottom of the settings sidebar and under *General → About* (with a *Copy* button for bug reports), and it is the first line of every log file (`version`, `commit`, `dirty`, `exe`). The installer window, the file properties and *Apps & features* show the plain version. `scripts/build.mjs` computes the identity and hands it to `src-tauri/build.rs` as `DETTINO_BUILD_*`; `tauri dev` and plain `cargo` builds probe git themselves.

### End date for test builds

Installers never expire by default. For alpha testers you can build one that stops working on a date. There are no license keys, just a date baked into the build:

```powershell
pnpm build                          # never expires (the default)
pnpm build --expiring               # works until today + 60 days
pnpm build --expires 2027-01-31     # a date of your choosing
pnpm release patch --expiring       # pnpm release passes either flag on to pnpm build
```

The build prints the date and puts it in the installer name. The app shows it under *General → About*, and Home shows it as a quiet line, then as a warning in the last 7 days. After the date, Dettino shows a message box at launch (or within the hour if it is still running), stops starting with Windows and quits. Settings, history and keys stay, so a newer build picks up where the old one stopped. The uninstaller keeps working after expiry. Only the local date counts: it is a reminder for testers, not copy protection.

`scripts/build.mjs` passes the date as `DETTINO_EXPIRES`; `src-tauri/build.rs` refuses a date that does not exist, and `src-tauri/src/expiry.rs` does the rest. A plain `pnpm build`, `tauri dev`, plain `cargo` builds and CI set no date and never expire.

## Troubleshooting the shortcut

- **Nothing happens while the Dettino window is focused.** This is expected. On Windows, a focused WebView2 window's keys never reach global keyboard hooks; on macOS the shortcut is passed through to the window. Dettino never pastes into itself. Switch to another app to dictate.
- **Nothing happens in another app.** Each press is logged as a `hotkey diag` line in `%LOCALAPPDATA%\dev.dettino.desktop\logs\`. The line shows the tracked and OS modifier state, the outcome, the foreground app and whether it runs elevated. If the target window runs as admin and Dettino does not, Windows blocks the paste. In that case the pill shows "runs as admin" and the text is copied to the clipboard instead. Set `DETTINO_LOG=debug` for more detail.
- **Testing with injected keys.** Keystrokes sent with `SendInput`, AutoHotkey and similar tools can be invisible to the hook on some machines, for example with Razer or Logitech input drivers. Test with the physical keyboard.

### macOS

- **Nothing happens.** Check *Settings → Advanced → Permissions*. The pill shows a keyboard badge while Accessibility is missing. Shortcuts start working within a second of turning it on, no restart needed. If the switch is on but Dettino still says "Needed", the grant belongs to an older build: remove Dettino from the list with "−" and add it again.
- **Secure input.** While a password field or Terminal's *Secure Keyboard Entry* is active, macOS hides all keys from other apps, including Dettino's shortcut.
- The `hotkey diag` lines in `~/Library/Application Support/dev.dettino.desktop/logs/` show what the event tap saw, as on Windows.

## Tests and tools

```powershell
cargo test --workspace                     # 100+ Rust tests (state machine, chain/hedging, guardrails, adapters via fake servers, storage encryption …)
cd apps/desktop; pnpm test                 # UI unit tests
```

CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) runs on every pull request and on pushes to `main`, on Windows and macOS: `pnpm typecheck`, `pnpm test`, `vite build`, `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`.

UI visual QA without Tauri: `pnpm vite:dev`, then open `http://localhost:1420/?mock=ready&section=providers` (or `pill.html?mock=ready&pill=listening`) in a browser. The mock bridge lives in `apps/desktop/src/dev/` and is excluded from production builds.

Data lives in `%LOCALAPPDATA%\dev.dettino.desktop` on Windows and `~/Library/Application Support/dev.dettino.desktop` on macOS (settings, encrypted history, recordings, models, logs). `DETTINO_DATA_DIR=<folder>` runs an isolated instance with its own settings, history and models.

## Layout

```
crates/core        settings, capture state machine, provider traits, fallback chain with hedging
crates/hotkey      chord tracker (pure) + Windows low-level hook / macOS event tap, key injection
crates/audio       capture, resampler, pre-roll ring, level meter, VAD trait, WAV writer
crates/asr-local   Parakeet / Whisper / Silero via sherpa-onnx
crates/asr-cloud   OpenAI and OpenAI-compatible transcription
crates/llm         Anthropic, OpenAI, Perplexity, Ollama, OpenAI-compatible cleanup
crates/cleanup     prompt, guardrails, local rules, dictionary, snippets, auto-learn, nouns
crates/platform    foreground app, UI Automation / Accessibility context, clipboard paste/verify/restore, sounds, permissions
crates/storage     settings file, keychain secrets, encrypted history, model downloads
apps/desktop       Tauri app (Rust wiring) + React UI (pill, onboarding, settings)
docs                  end-user setup guide with screenshots
```

## Known gaps

- Real dictation with the physical shortcut has not yet been checked end to end across many apps (terminals, elevated windows, German keyboard layout). The installer has only been installed and uninstalled silently (`/S`) on the development PC.
- The guided setup's fresh "Install Ollama" path, the SmartScreen dialog and the installer's "Run Dettino" finish option have not been tried on a clean PC yet.
- A dictation right after switching the cleanup model waits for the model to load, and the pill gives no hint why.
- macOS: Caps Lock works only as a tap chord, because macOS reports the lock state, not the key. The Fn key can't be recorded in the shortcut field, because the browser engine never sees it.
- Not implemented yet: OpenAI realtime streaming transcription (cloud transcription runs in batch when the key is released) and the "Apply as edit?" confirmation for command dictations.

## Privacy

No accounts, no telemetry, no screenshots. See [PRIVACY.md](PRIVACY.md).

## Support

Dettino is free, open source and has no accounts or subscriptions. If it saves you typing, you can [buy me a coffee](https://buymeacoffee.com/hubertmk). It pays for the signing certificates and the test machines.

## License

MIT — see [LICENSE](LICENSE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
