# Contributing to Dettino

Thanks for your interest. Bug reports, fixes and small, focused features are welcome.

## Before you start

- **Bugs:** open an issue with the version (Settings → General → About → Copy), the OS, the app you dictated into, and the relevant `hotkey diag` or error lines from the logs (Settings → General → Advanced → Open logs). The logs never contain what you said.
- **Features:** open an issue first and describe the problem, not just the solution. Dettino is deliberately small: no accounts, no telemetry, no backend. Changes that add network traffic the user did not ask for will not be merged.
- **Security issues:** do not open a public issue. See [SECURITY.md](SECURITY.md).

## Setting up

The [README](README.md) has the full quick start for Windows and macOS. In short:

```sh
cd apps/desktop
pnpm install
pnpm tauri dev
```

## Checks

CI runs these on every pull request, on Windows and macOS. Run them locally before you push:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd apps/desktop && pnpm typecheck && pnpm test
```

Clippy warnings fail the build. New behaviour needs a test; the Rust crates have unit tests next to the code, the UI has Vitest tests under `apps/desktop/src/__tests__`.

## Pull requests

- One topic per pull request. Keep refactors separate from behaviour changes.
- Explain *why* in the description and in code comments; the diff already shows *what*.
- Keep the privacy promises in [PRIVACY.md](PRIVACY.md) true. If a change alters what leaves the computer or what is stored, update PRIVACY.md in the same pull request.
- Do not bump the version. Releases are cut with `pnpm release` by a maintainer.
- Match the existing style: short functions, no dead code, comments that say what the reader cannot see.

## License

By contributing you agree that your contributions are licensed under the [MIT License](LICENSE), the same as the project.
