# Third-party notices

Dettino is MIT-licensed. It builds on the components below; their licenses apply to them. The lists are grouped by how a component reaches your computer: compiled into the app, bundled next to it in the installer, downloaded by the app at your request, or used only for the website and the promo video.

## Compiled into the app

Every Rust and npm dependency is under a permissive license. The full, exact inventory for a given commit comes from the lockfiles:

```sh
cargo metadata --format-version 1 | python3 -c 'import json,sys; [print(p["name"], p.get("license")) for p in json.load(sys.stdin)["packages"]]'
cd apps/desktop && pnpm licenses list
```

The notable ones:

| Component | License | Notes |
|---|---|---|
| Tauri, tao, wry, tauri plugins, tray-icon, muda | MIT OR Apache-2.0 | |
| tauri-nspanel | MIT | macOS pill window |
| sherpa-rs | MIT | Rust bindings to sherpa-onnx |
| cpal, hound, similar, tao | Apache-2.0 | |
| rusqlite / SQLite | MIT / Public Domain | |
| reqwest, hyper, tokio, serde, tracing, crossbeam, chrono, strsim, axum | MIT OR Apache-2.0 | |
| chacha20poly1305, aead (RustCrypto) | MIT OR Apache-2.0 | history encryption |
| keyring | MIT OR Apache-2.0 | system keychain |
| rustls, ring, hyper-rustls | Apache-2.0 / ISC / MIT | TLS |
| webpki-roots | CDLA-Permissive-2.0 | Mozilla root certificates |
| windows-rs | MIT OR Apache-2.0 | © Microsoft Corporation |
| objc2 family | MIT OR Zlib OR Apache-2.0 | macOS bindings |
| icu4x crates (icu_*, tinystr, zerovec, …) | Unicode-3.0 | © Unicode, Inc. |
| cssparser, selectors, dtoa-short, option-ext | MPL-2.0 | file-level copyleft; used unmodified |
| brotli, brotli-decompressor | BSD-3-Clause / MIT | |
| bindgen, clang-sys | BSD-3-Clause / Apache-2.0 | build time only |
| encoding_rs | (Apache-2.0 OR MIT) AND BSD-3-Clause | |
| gtk, glib, webkit2gtk bindings | MIT | Linux only, dynamically linked to LGPL system libraries |
| React, React DOM | MIT | © Meta Platforms, Inc. |
| Vite, Vitest, TypeScript | MIT / Apache-2.0 | build and test only |
| lightningcss | MPL-2.0 | build time only |
| lucide-react | ISC | © Lucide contributors |

## Bundled in the installer

| Component | License | Notes |
|---|---|---|
| sherpa-onnx (prebuilt C API libraries) | Apache-2.0 | © Xiaomi Corporation and contributors. <https://github.com/k2-fsa/sherpa-onnx/blob/master/LICENSE> |
| ONNX Runtime | MIT | © Microsoft Corporation. <https://github.com/microsoft/onnxruntime/blob/main/LICENSE> |
| cargs | MIT | © Leonard Iklé; shipped with sherpa-onnx on Windows |

## Downloaded by the app at your request

Nothing below is part of this repository. The app downloads it from the upstream source when you choose a mode or provider, and the upstream terms apply.

| Component | License | Source |
|---|---|---|
| NVIDIA Parakeet TDT 0.6B v2 / v3 | CC-BY-4.0 | © NVIDIA Corporation. Int8 ONNX export published by the sherpa-onnx project on GitHub releases. Used unmodified. <https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2> |
| OpenAI Whisper weights (ONNX export) | MIT | © OpenAI. Export published by the sherpa-onnx project. <https://github.com/openai/whisper/blob/main/LICENSE> |
| Silero VAD | MIT | © Silero Team. <https://github.com/snakers4/silero-vad/blob/master/LICENSE> |
| Ollama | MIT | Installed by the guided local setup from ollama.com. <https://github.com/ollama/ollama/blob/main/LICENSE> |
| Gemma 4 (`gemma4:e2b-it-qat`) | Gemma Terms of Use | © Google. Pulled from the Ollama registry by the guided local setup. Use is subject to <https://ai.google.dev/gemma/terms> and the Gemma Prohibited Use Policy. |

Cloud providers you configure (OpenAI, Anthropic, Perplexity, or an OpenAI-compatible server) are governed by your agreement with them.

## Website and promo video only

Not part of the app.

| Component | License | Notes |
|---|---|---|
| Archivo (`site/fonts/archivo-latin.woff2`) | SIL Open Font License 1.1 | © The Archivo Project Authors (Omnibus-Type). Latin subset from Google Fonts, unmodified, self-hosted by the website. |
| Geist, Geist Mono | SIL Open Font License 1.1 | © Vercel. Loaded by `@remotion/google-fonts` in `tools/promo-video`. |
| Remotion | Remotion License | <https://github.com/remotion-dev/remotion/blob/main/LICENSE.md>. Free for individuals and for companies with up to three employees; larger companies need a company license to render with it. Only `tools/promo-video` uses it; the app does not. |
