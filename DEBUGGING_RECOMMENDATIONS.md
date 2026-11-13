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
and insertion. See [English dictation](docs/english-dictation.md). Turn all three
options off to compare provider text with formatted output. Missing command words
cannot be reconstructed by formatting.

Hebrew filler filtering checks standalone `תודה` / `תודה רבה` and whole-recording
RMS below 0.02. It skips injection, history, and last-transcription replacement.
The threshold can also suppress quiet speech or a short phrase surrounded by
silence; test those cases when investigating missing Hebrew text.

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
