# AirType troubleshooting and diagnostics

This guide describes cloud batch transcription for 1.5.0. Earlier recommendations
about an OpenAI Realtime WebSocket migration are obsolete: the client now uses
`transcription/openai_batch.rs` and `transcription/runpod.rs`.

## Transcription

Both languages transcribe after recording stops. Hebrew uses the default RunPod
endpoint. English selects shared RunPod, custom RunPod, or OpenAI `whisper-1` in
Settings. No local model download or streaming client is required.

Use the main-window Test buttons to check credentials and bundled audio against
the selected provider. Test real microphone recording and insertion into a text
editor separately. RunPod queueing shows warming before processing; OpenAI uses
processing directly. Cold-start latency is provider dependent.

English formatting runs after recognition and before preview, history, copying,
and insertion. See [English dictation](docs/english-dictation.md). Turn both
options off to compare provider text with formatted output. Missing command words
cannot be reconstructed by formatting.
Every recognized spoken command must write its own mark. Repeated or mixed
commands accumulate; only adjacent automatic punctuation is removed. Test
“question mark question mark question mark” → `???` with removal on and off.
“Dot” is a period alias. Removal should clean `Hi. How` to `Hi how`, but retain
`I`, its contractions, acronyms, names/ambiguous capitals, paragraph openers, and
URL/email spelling. Commas, abbreviations, and decimal points do not trigger case
cleanup. The obsolete standalone lowercase setting is ignored on legacy load.
At minimum window height, focus the spoken-help button and use PageDown/End to
read clipped instructions. The tooltip should scroll while focus and the outer
settings panel stay fixed; Escape should still dismiss it.
When checking value formatting, distinguish URL suffixes from email sentence
marks and punctuation outside paired wrappers. Removal should turn
`(user@example.com);` into `user@example.com` and `(https://example.com/?);`
into `https://example.com/?`, with spoken commands either on or off.

Hebrew filler filtering checks standalone `תודה` / `תודה רבה` against nonempty
capture data in 10 ms windows. A result is discarded only if every captured sample
is finite digital zero. Any nonzero signal, empty input, or nonfinite sample keeps
the transcript. Background noise can keep hallucinations too; no amplitude cutoff
is used to infer that quiet speech was absent. Digital-silence suppression skips
injection, history, and last-transcription replacement.

## Logs and hotkeys

Settings → Logs supports refresh, copy, and export. Logs may contain transcript
text and provider diagnostics; inspect and redact them before sharing privately.
Do not post API keys or raw security correspondence in issues.

Settings invokes `update_hotkeys` after hotkey edits; a restart should not normally
be needed. Check registration errors in logs and test both combination and
modifier-only hotkeys. The vendored rdev macOS patch skips background text-input
source lookup; key events remain available but `Event::name` is not populated.

On macOS check Microphone, Accessibility, and Input Monitoring permissions.
On Linux test under X11; Wayland can restrict global input and text insertion.

See [README](README.md) for setup/releases, [AGENTS.md](AGENTS.md) for commands,
and [TODOs.md](TODOs.md) for unresolved checks. Unit tests do not replace live
provider, desktop permission, or signed-update testing.

## Progressive releases

Builds run in parallel; successful platform publishers join one shared versioned
release under a repository-wide queued lock. A public release with one platform
is available, but is not complete. Inspect each build and publisher result.
Until its target is published, an older client can report a missing-target check
error. Do not point that target at an older binary under the new version.

For a missing `latest.json`, retry the failed publisher job using its original
run/commit artifact; metadata is reconstructed from all completed signed sets.
Asset replacement can briefly return 404. Package/signature digest conflicts
require a new version, not clobbering files. If a release is immutable (including
after the first draft is published), stop; do not change protection settings.
Never rerun historical all-at-once tag workflows as progressive recovery or move
an existing tag. See README for the supported job-level recovery path.
Unpinned quick installers fall back only when the latest release lacks their
package, selecting a compatible stable release within the first 100 entries.
Explicit version pins and download/API errors never silently select an older
version. The Windows resolver has a mocked PowerShell test script; execute it on
a machine with PowerShell rather than treating a skipped test as a pass.
