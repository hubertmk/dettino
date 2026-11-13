# Privacy

Dettino has no accounts, no telemetry, no analytics and no backend. It never takes screenshots.

## What leaves your computer

Only what you send to the providers **you** configure:

| Provider runs… | Receives |
|---|---|
| On this computer (Parakeet, Whisper, Ollama, LM Studio, local servers) | Nothing leaves the machine |
| Cloud transcription (OpenAI or a custom server) | The recording of each dictation, trimmed to speech; dictionary terms; and, unless you turn screen context off or limit it to local providers, up to 300 characters before the cursor and up to 50 names found on screen (as hints) |
| Cloud cleanup (Anthropic, OpenAI, Perplexity) | The transcript, your standing instruction, relevant dictionary terms and — unless you turn it off — screen context: app name, window title, up to 300 characters before / 100 after the cursor, the selection, and up to 20 proper nouns found on screen |

"Send context only to providers on this computer" (Settings → Context & privacy) keeps all screen context local. The "App name" and "Window title" switches remove those two from the payload. "What would be sent?" shows the exact payload for any app.

In Balanced mode, transcription runs on this computer first; the recording goes to the cloud transcription provider only if on-device transcription fails, never because it is slow.

Never read: password fields, browser address bars, and apps you mark "Never dictate".

Other network traffic, all of it started by you: model downloads (sherpa-onnx models from GitHub releases; with the guided local setup, the Ollama installer from ollama.com and the cleanup model from the Ollama registry), and a short connection check to each cloud provider you configured when Dettino starts or when you change a provider (it carries the API key and nothing else).

To paste, the text is placed on the clipboard for a moment. It is marked so that Windows Clipboard History and the cloud clipboard ignore it, and the previous clipboard content is put back.

## What is stored on your computer

In `%LOCALAPPDATA%\dev.dettino.desktop` (Windows) or `~/Library/Application Support/dev.dettino.desktop` (macOS):

- `settings.json` — your settings, dictionary and snippets (no keys).
- `history.db` — each dictation's raw and cleaned text, app, timings and context, **encrypted** with XChaCha20-Poly1305. The key is in the system keychain. Stored in clear, for ordering and retention: each entry's time, status and recording file name, and the per-provider timing statistics.
- `audio/` — recordings, deleted automatically after 1–7 days (your choice).
- `logs/` — timings and errors, and one line per shortcut press (which key, which app, what the hook decided); never what you said.

API keys live only in Windows Credential Manager / macOS Keychain.

## Deleting

Settings → Context & privacy → Delete everything removes history, recordings (including recordings kept after a crash) and statistics. Uninstalling and deleting the folder above removes everything else; keys can be removed in Settings → Providers or the system keychain ("Dettino").

## The keyboard hook

Dettino installs a low-level keyboard hook to detect your shortcut. It acts only on the configured chord keys; every other key is passed through immediately and is never stored, logged or transmitted.
