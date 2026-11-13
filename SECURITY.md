# Security policy

## Reporting a vulnerability

Please do not report security issues in public issues or pull requests.

Use [GitHub's private vulnerability reporting](https://github.com/hubertmk/dettino/security/advisories/new) for this repository, or write to <ciao@dettino.com>.

Include what you found, how to reproduce it, the Dettino version (Settings → General → About → Copy) and your OS. You will get an acknowledgement within a few days. Fixes ship in the next release and the advisory is published once a fixed version is available.

## Supported versions

Only the latest release receives fixes. Expiring test builds are not supported.

## What is in scope

Dettino runs with your user's permissions and handles sensitive material. Reports are especially welcome for:

- the low-level keyboard hook and macOS event tap (anything that captures keys other than the configured chord);
- the clipboard paste path (leaking the previous clipboard, pasting into the wrong app, clipboard history exposure);
- screen context gathering (reading password fields, address bars or apps marked "Never dictate");
- the encrypted history database and keychain handling (key material in logs, settings exports or crash files);
- the guided installer for Ollama (download verification, signature checks, privilege use);
- anything that sends data to a provider the user did not configure.

Out of scope: the behaviour of third-party providers you configured yourself, and the security of models you download.
