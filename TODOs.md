# Release 1.5.0 validation

Implementation status is separate from platform validation. Describing these
features does not mean that the three PRs have been merged or released.

## Before merging the release PR

- Run frontend checks/build, Rust tests/build, and release tests on the combined
  changes. Use Node.js 20+ for the supported setup path.
- Verify real recordings on shared RunPod, custom RunPod, and OpenAI, including
  formatting combinations, literal escapes, multiline insertion, Hebrew silence,
  quiet speech, and a short phrase surrounded by long pauses.
- On Linux, macOS, and Windows verify hotkeys (hold/toggle and modifier-only),
  microphone permissions, text insertion, tray/window behavior, and saved settings.
- Verify macOS launch-on-login and modifier-only hotkeys on the patched rdev build.
- Verify an older installed client discovers and installs a signed update while
  retaining settings and history. This requires published artifacts; manifest
  tests alone do not establish it.
- Merge #2 and #4 before #3; #3 starts the 1.5.0 release on merge to `main`.

## Implemented features to retain

Configurable indicator placement, warming/processing states, single tray icon,
macOS window reopening, in-app logs, live hotkey re-registration, signed updater
support, local history, and dismissible last-transcription preview are implemented.
English formatting defaults off for legacy settings. The setup helper checks
prerequisites and installs locked npm dependencies without installing system
packages or downloading models.

Review regressions are covered by tests: Hebrew filler suppression now keeps
quiet/brief speech and unknown capture data, discarding only nonempty digital
silence. Spoken commands preserve preceding URL syntax with removal on or off.
Settings tests cover the actual disk-write boundary, redacted secret fields,
unchanged IPC values, file permissions, and write failures without touching the
host keyring.
