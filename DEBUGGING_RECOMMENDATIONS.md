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
