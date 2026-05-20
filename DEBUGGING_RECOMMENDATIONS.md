# AirType — English Transcription Fix & Improvements

## Root Cause

**OpenAI Realtime API β→GA migration (beta shutdown: ~May 2026).** Multiple protocol-level changes between beta and GA break the current WebSocket client.

---

## Issues Found (ordered by severity)

### 1. 🔴 OpenAI Realtime — full beta→GA migration needed

**File:** `src-tauri/src/transcription/openai_realtime.rs`

Five changes required in this file:

| Line | Current (Beta) | Required (GA) |
|---|---|---|
| `14` — URL | `?intent=transcription` | `?model=gpt-4o-transcribe` |
| `22` — event type | `transcription_session.update` | `session.update` |
| `35-41` — session config | no `type` field | add `type: "transcription"` |
| `124` — WS header | `OpenAI-Beta: realtime=v1` | **delete line** |
| `201` — server events | `transcription_session.created/updated` | `session.created/updated` |

Audio buffer events (`input_audio_buffer.append/commit`) and transcription output events (`conversation.item.input_audio_transcription.delta/completed`) are **unchanged** in GA.

**Impact:** This restores English transcription.

---

### 2. 🟠 OpenAI errors are silent — never reach the UI

**File:** `src-tauri/src/transcription/openai_realtime.rs:192-199` and `src-tauri/src/lib.rs:216-229`

The WebSocket reader task logs errors with `log::error!` but **does not emit an `"error"` event** to the frontend. The callback closure doesn't have access to the `AppHandle`. This is why you saw nothing in the UI when the beta API was rejected.

**Fix:** Pass a clone of `AppHandle` into the reader task and emit `"error"` on WebSocket errors and connection failures.

**Impact:** All future OpenAI API errors (auth, quota, format changes) will appear as the red error toast in the UI.

---

### 3. 🟠 Event name mismatch — streaming deltas never display

**Files:**
- Backend emits: `"transcription-delta"` (`src-tauri/src/lib.rs:228`)
- Frontend listens: `"transcription-partial"` (`src/routes/+page.svelte:82`)

**Fix:** Rename one to match the other. Recommend changing backend to emit `"transcription-partial"` (matches the semantic name used in the UI).

**Impact:** Live streaming text will appear in the UI as the user speaks (currently only the final result shows, if it arrives at all).

---

### 4. 🟡 No in-app log viewer — requires terminal to debug

The app uses `env_logger` which writes to stderr. On macOS, packaged apps don't have a visible terminal, so logs are invisible unless launched from CLI.

**Proposed fix:** Add a ring-buffer log capture in Rust and expose it via a Tauri command `get_app_logs`. Add a **"Logs"** section to the Settings panel with a scrollable log view and optional log-level filter.

**Approach:**
- **Backend:** Add a custom `log` layer that writes to an in-memory `VecDeque<LogEntry>` (capped at ~500 entries) behind an `Arc<RwLock>`. Expose `get_app_logs` and `export_logs` Tauri commands. `export_logs` writes all captured logs to a timestamped file (e.g. `~/Desktop/airtype-logs-2026-05-15.txt`) and returns the path.
- **Frontend:** New collapsible section in `SettingsPanel.svelte` below "System" — shows timestamped log entries, color-coded by level (error=red, warn=yellow, info=default, debug=dim). Add a "Refresh" button, a "Copy All" button, and an **"Export Logs"** button that saves to a file and shows the path.

**Impact:** Users can self-diagnose issues without opening a terminal. Export enables sharing logs for support. Minimal memory overhead (~50KB for 500 entries).

---

## Option: RunPod for English Transcription

### How it would work

The existing RunPod endpoint uses `ivrit-ai/whisper-large-v3-turbo-ct2` — this model supports **all Whisper languages**, including English. The only change needed is sending `language: "en"` instead of `"he"`.

### Pros
- **Already works** — same infrastructure, same API key, same endpoint
- **No OpenAI key needed** — single provider for both languages
- **Simpler codebase** — remove the entire OpenAI Realtime WebSocket client
- **Predictable costs** — RunPod pay-per-second pricing

### Cons
- **No live streaming** — text only appears after recording stops (batch mode)
- **Cold start latency** — first request after idle can take 5-15s extra
- **Quality tradeoff** — OpenAI's `gpt-4o-transcribe` is generally more accurate for English than Whisper Large v3 Turbo
- **Dependency** — both languages go down if RunPod has issues

### Recommendation

**Keep both options available.** Add a setting to choose English transcription backend:

| Setting value | English backend | Hebrew backend | Live streaming |
|---|---|---|---|
| `OpenAI` (current) | OpenAI Realtime WS | RunPod ivrit-ai | ✅ English only |
| `RunPod` (new) | RunPod whisper (batch) | RunPod ivrit-ai | ❌ No |

This gives users a fallback if one provider has issues. Implementation is small — generalize `transcribe_hebrew` to accept a language parameter and route based on a new setting.

---

### 6. 🔴 Hotkey changes don't take effect — requires app restart

**Files:** `src-tauri/src/lib.rs:407-410`

The `save_settings` Tauri command only persists settings to disk and updates in-memory state. It **never re-registers hotkeys with the OS**. Hotkeys are registered once at app startup in two places:
- `build_global_shortcut_plugin` (manager.rs:572) — for combo keys like `Ctrl+Shift+E`
- `keyboard_listener.register_modifier_hotkey` (lib.rs:846,873) — for modifier-only keys like `RightControl`

When a user changes a hotkey in the UI, the new key is saved and displayed correctly, but the OS still listens for the old key.

**Fix:** Add a `update_hotkeys` Tauri command (or extend `save_settings`) that:
1. Unregisters all current hotkeys (`hotkey_manager.unregister_all()` + `keyboard_listener.stop()` + clear modifier registrations)
2. Re-registers with the new hotkey values from settings
3. Restarts the keyboard listener if modifier-only hotkeys are configured

**Impact:** Hotkey changes take effect immediately without restarting the app.

---

## Summary of Changes

| # | Change | Files touched | Effort |
|---|---|---|---|
| 1 | Full beta→GA migration (URL, header, events, session config) | `openai_realtime.rs` | ~20 lines |
| 2 | Emit OpenAI errors to frontend | `openai_realtime.rs`, `lib.rs` | ~15 lines |
| 3 | Fix `transcription-delta` → `transcription-partial` | `lib.rs` | 1 line |
| 4 | In-app log viewer with export | `lib.rs`, `SettingsPanel.svelte`, new `log_capture.rs` | ~150 lines |
| 5 | RunPod English option (optional) | `runpod.rs`, `settings/mod.rs`, `lib.rs`, `SettingsPanel.svelte` | ~80 lines |
| 6 | Live hotkey re-registration on save | `lib.rs`, `hotkeys/keyboard.rs` | ~60 lines |

**Items 1-3, 6 are critical fixes.** Item 4 is quality-of-life. Item 5 is an enhancement.
