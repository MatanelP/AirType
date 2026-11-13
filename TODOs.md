# Release and live validation

Version 1.5.0 is published, including PRs #2, #4, and #3. Release CI and public
updater-manifest checks passed; live installed-client and device checks below
remain separate. The 1.5.1 follow-up preserves every spoken punctuation command.

## Before merging the release PR

- Run frontend checks/build, Rust tests/build, and release tests on the combined
  changes. Use Node.js 20+ for the supported setup path.
- Verify real recordings on shared RunPod, custom RunPod, and OpenAI, including
  formatting combinations, literal escapes, multiline insertion, Hebrew silence,
  quiet speech, and a short phrase surrounded by long pauses.
- Verify repeated and mixed spoken punctuation, including “question mark” three
  times producing `???` and “dot dot dot” producing `...`, with removal on/off.
  Check conservative sentence-case cleanup, preserved `I`/contractions, names,
  acronyms, decimals, abbreviations, and paragraph openers. Check the help tooltip
  by pointer, keyboard, and touch, including clipped-help scrolling at 400px
  height, and full switch width in narrow windows.
- On Linux, macOS, and Windows verify hotkeys (hold/toggle and modifier-only),
  microphone permissions, text insertion, tray/window behavior, and saved settings.
- Verify macOS launch-on-login and modifier-only hotkeys on the patched rdev build.
- Verify an older installed client discovers and installs a signed update while
  retaining settings and history. This requires published artifacts; manifest
  tests alone do not establish it.
- Merge all intended changes before a version bump; merging that bump to `main`
  starts the signed release path.
- On the next authorized release, verify that the first completed platform can
  update before others finish, all targets ultimately join the same release,
  and a failed publisher recovers using its original artifact. Local fault tests
  cover orchestration; no review release should be published to test it.
- Before progressive publication, verify repository/owner release immutability
  is disabled without changing protections. Confirm no unknown/immutable release
  is promoted. Verify quick installers during a partial rollout on each OS.

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
Removal separately cleans email sentence marks and paired wrappers, including
punctuation outside closing wrappers, without shortening the contained URL.
Settings tests cover the actual disk-write boundary, redacted secret fields,
unchanged IPC values, file permissions, and write failures without touching the
host keyring.
