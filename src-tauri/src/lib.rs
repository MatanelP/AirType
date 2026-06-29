//! AirType - Voice to Text Desktop Application
//!
//! A cross-platform desktop app for live voice-to-text transcription.
//! Press a global hotkey anywhere to record voice, which is transcribed
//! and inserted at the cursor position.

use parking_lot::RwLock;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Listener, Manager, State,
};

pub mod audio;
pub mod history;
pub mod hotkeys;
pub mod injection;
pub mod log_capture;
pub mod settings;
pub mod transcription;

use audio::AudioCapture;
use history::{HistoryStore, TranscriptionEntry};
use hotkeys::{
    build_global_shortcut_plugin, is_modifier_only_hotkey, HotkeyEvent, HotkeyManager,
    KeyboardListener, ModifierKey,
};
use injection::TextInjector;
use settings::{EnglishEndpointType, IndicatorAlign, Settings, SettingsStore};
use transcription::{
    encode_wav, english_test_wav, hebrew_test_wav, transcribe_audio, transcribe_audio_wav,
    transcribe_english, transcribe_hebrew_wav, validate_runpod,
};
use tauri_plugin_updater::UpdaterExt;

use std::sync::atomic::{AtomicU64, Ordering};

/// High-level recording phase. This is the single source of truth that drives
/// both the floating indicator and the main-window UI, and gates hotkey actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordingPhase {
    /// Not recording and nothing in flight
    Idle,
    /// Microphone is capturing audio
    Recording,
    /// Recording stopped; transcription/injection in flight
    Processing,
}

/// Application state shared across all Tauri commands
pub struct AppState {
    /// Audio capture instance
    pub audio: RwLock<Option<Arc<AudioCapture>>>,
    /// Current recording language
    pub recording_language: RwLock<String>,
    /// Hotkey manager
    pub hotkey_manager: Arc<HotkeyManager>,
    /// Low-level keyboard listener for modifier-only hotkeys
    pub keyboard_listener: Arc<KeyboardListener>,
    /// Settings store
    pub settings_store: RwLock<SettingsStore>,
    /// Whether currently recording (kept in sync with `phase` for existing read sites)
    pub is_recording: RwLock<bool>,
    /// Authoritative recording phase (Idle/Recording/Processing)
    pub phase: RwLock<RecordingPhase>,
    /// Monotonic session counter. Incremented on each recording start so that
    /// delayed/async emissions from an old session can be discarded.
    pub generation: AtomicU64,
    /// Last transcription result
    pub last_transcription: RwLock<String>,
    /// Persistent transcription history
    pub history: HistoryStore,
}

impl AppState {
    pub fn new(hotkey_manager: Arc<HotkeyManager>, keyboard_listener: Arc<KeyboardListener>) -> Self {
        let settings_store = SettingsStore::new().unwrap_or_else(|e| {
            log::error!("Failed to create settings store: {}", e);
            panic!("Cannot start without settings store");
        });
        let audio = AudioCapture::new().ok().map(Arc::new);
        if audio.is_none() {
            log::warn!("Audio capture not prewarmed at startup");
        }
        let history = HistoryStore::new(SettingsStore::get_config_dir());
        Self {
            audio: RwLock::new(audio),
            recording_language: RwLock::new("en".to_string()),
            hotkey_manager,
            keyboard_listener,
            settings_store: RwLock::new(settings_store),
            is_recording: RwLock::new(false),
            phase: RwLock::new(RecordingPhase::Idle),
            generation: AtomicU64::new(0),
            last_transcription: RwLock::new(String::new()),
            history,
        }
    }

    /// Get the current settings
    pub fn get_settings(&self) -> Settings {
        self.settings_store.read().get()
    }

    pub fn current_phase(&self) -> RecordingPhase {
        *self.phase.read()
    }

    pub fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Transition to Recording. Returns the new generation number.
    pub fn begin_recording(&self) -> u64 {
        let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        *self.phase.write() = RecordingPhase::Recording;
        *self.is_recording.write() = true;
        gen
    }

    /// Transition to Processing (mic stopped, transcription in flight).
    pub fn begin_processing(&self) {
        *self.phase.write() = RecordingPhase::Processing;
        *self.is_recording.write() = false;
    }

    /// Transition back to Idle.
    pub fn finish(&self) {
        *self.phase.write() = RecordingPhase::Idle;
        *self.is_recording.write() = false;
    }

    fn get_audio_capture(&self) -> Result<Arc<AudioCapture>, String> {
        if let Some(capture) = self.audio.read().as_ref() {
            return Ok(capture.clone());
        }

        let capture = AudioCapture::new()
            .map(Arc::new)
            .map_err(|e| format!("Failed to initialize audio: {}", e))?;
        *self.audio.write() = Some(capture.clone());
        Ok(capture)
    }
}

// ============================================================================
// Phase helpers
// ============================================================================

/// Emit a unified `phase-changed` event consumed by both the indicator and main windows.
/// `visual_phase` is the string sent to the UI ("idle","recording","processing","done").
fn emit_phase<R: tauri::Runtime>(
    app: &AppHandle<R>,
    visual_phase: &str,
    language: &str,
    generation: u64,
) {
    let _ = app.emit(
        "phase-changed",
        serde_json::json!({
            "phase": visual_phase,
            "language": language,
            "generation": generation,
        }),
    );
}

/// Surface a **critical, flow-blocking** error (no mic, transcription failure,
/// injection failure, missing credentials, …) to the user in BOTH the main UI
/// (toast) and the floating indicator (red error state). Minor/recoverable
/// conditions should NOT go through here — they stay as plain log warnings.
fn surface_critical_error<R: tauri::Runtime>(app: &AppHandle<R>, message: impl Into<String>) {
    let message = message.into();
    log::error!("{}", message);

    // Abort any in-flight session and bump the generation so a pending
    // hide_indicator from the aborted flow can't clobber the error display.
    let state = app.state::<AppState>();
    state.finish();
    let gen = state.generation.fetch_add(1, Ordering::SeqCst) + 1;

    // Both the main-UI toast and the indicator window listen for "error".
    let _ = app.emit("error", message);

    // Make the indicator window visible; it switches itself to the error state.
    position_and_show_indicator(app);

    // Auto-hide after a few seconds, unless a new session started meanwhile.
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        hide_indicator(&app_clone, gen);
    });
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// Start recording audio
#[tauri::command]
async fn start_recording(state: State<'_, AppState>, app: AppHandle, language: Option<String>) -> Result<(), String> {
    // Block if transcription is already in flight.
    if state.current_phase() == RecordingPhase::Processing {
        log::info!("start_recording: blocked — processing in flight");
        return Ok(());
    }

    if let Some(lang) = language {
        *state.recording_language.write() = lang;
    }

    let language = state.recording_language.read().clone();
    log::info!("Starting recording (post-capture setup) for language: {}", language);

    // Ensure capture is active.
    let capture = state.get_audio_capture()?;
    if !capture.is_recording() {
        capture.clear_stream_sender();
        capture
            .start_recording()
            .map_err(|e| format!("Failed to start recording: {}", e))?;
    }
    let gen = state.begin_recording();
    let _ = app.emit("recording-started", ());
    emit_phase(&app, "recording", &language, gen);

    Ok(())
}

/// Stop recording and get transcription
#[tauri::command]
async fn stop_recording(state: State<'_, AppState>, app: AppHandle) -> Result<String, String> {
    log::info!("Stopping recording...");

    if state.current_phase() != RecordingPhase::Recording {
        return Err("Not recording".to_string());
    }

    let settings = state.get_settings();
    let language = state.recording_language.read().clone();

    let capture = {
        state
            .audio
            .read()
            .as_ref()
            .cloned()
            .ok_or_else(|| "No audio capture instance".to_string())?
    };

    let samples = capture
        .stop_recording()
        .map_err(|e| format!("Failed to stop recording: {}", e))?;

    // Transition to Processing immediately so rapid presses are blocked.
    state.begin_processing();
    let gen = state.current_generation();
    let _ = app.emit("recording-stopped", ());

    // RunPod can cold-start, so show orange "Warming up" first; the status
    // callback below refines it (IN_QUEUE -> warming, IN_PROGRESS -> processing).
    // OpenAI has no cold start, so go straight to purple "Processing".
    let uses_runpod = language == "he"
        || matches!(
            settings.english_endpoint_type,
            EnglishEndpointType::SharedRunPod | EnglishEndpointType::CustomRunPod
        );
    emit_phase(&app, if uses_runpod { "warming" } else { "processing" }, &language, gen);

    if samples.is_empty() {
        log::warn!("No audio samples captured");
        state.finish();
        hide_indicator(&app, gen);
        return Ok(String::new());
    }

    log::info!("Captured {} audio samples for {} (Batch)", samples.len(), language);

    let app_cb = app.clone();
    let lang_cb = language.clone();
    let status_cb = move |status: &str| {
        let vphase = match status {
            "warming_up" => "warming",
            "processing"  => "processing",
            _ => return,
        };
        emit_phase(&app_cb, vphase, &lang_cb, gen);
    };

    let transcription = if language == "he" {
        let rp_key = settings.runpod_api_key
            .filter(|k| !k.is_empty())
            .ok_or_else(|| "Hebrew default RunPod API key not set. Go to Settings to add it.".to_string())?;
        let rp_endpoint = settings.runpod_endpoint_id
            .filter(|k| !k.is_empty())
            .ok_or_else(|| "Hebrew default RunPod Endpoint ID not set. Go to Settings to add it.".to_string())?;

        transcribe_audio(&rp_key, &rp_endpoint, &samples, &language, status_cb).await?
    } else {
        match settings.english_endpoint_type {
            EnglishEndpointType::SharedRunPod => {
                let rp_key = settings.runpod_api_key
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| "Default RunPod API key not set. Go to Settings to add it.".to_string())?;
                let rp_endpoint = settings.runpod_endpoint_id
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| "Default RunPod Endpoint ID not set. Go to Settings to add it.".to_string())?;

                transcribe_audio(&rp_key, &rp_endpoint, &samples, &language, status_cb).await?
            }
            EnglishEndpointType::CustomRunPod => {
                let rp_key = settings.english_custom_runpod_api_key
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| "Custom RunPod API key for English not set. Go to Settings to add it.".to_string())?;
                let rp_endpoint = settings.english_custom_runpod_endpoint_id
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| "Custom RunPod Endpoint ID for English not set. Go to Settings to add it.".to_string())?;

                transcribe_audio(&rp_key, &rp_endpoint, &samples, &language, status_cb).await?
            }
            EnglishEndpointType::OpenAI => {
                let openai_key = settings.english_openai_api_key
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| "OpenAI API key not set. Go to Settings to add it.".to_string())?;

                let wav_bytes = encode_wav(&samples, 16000);
                transcribe_english(&openai_key, &wav_bytes).await?
            }
        }
    };

    log::info!("{} transcription: {}", language, transcription);
    *state.last_transcription.write() = transcription.clone();
    let _ = app.emit("transcription-complete", &transcription);

    if !transcription.is_empty() {
        let entry = state.history.add(&transcription, &language);
        let _ = app.emit("history-added", &entry);

        log::info!("Injecting text: {}", transcription);
        let inject_result = tokio::task::spawn_blocking(move || {
            let mut injector = TextInjector::new().map_err(|e| format!("Failed to create injector: {}", e))?;
            injector.inject_text_clipboard(&transcription)
                .map_err(|e| format!("Failed to inject text: {}", e))
        })
        .await
        .map_err(|e| format!("Injection task failed: {}", e))?;

        match &inject_result {
            Ok(_) => log::info!("Text injected successfully"),
            Err(e) => log::error!("Text injection failed: {}", e),
        }
        inject_result?;
        let _ = app.emit("text-injected", ());
    }

    state.finish();
    emit_phase(&app, "done", &language, gen);
    hide_indicator(&app, gen);
    Ok(state.last_transcription.read().clone())
}

/// Get current settings
#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.get_settings()
}

/// Save settings
#[tauri::command]
fn save_settings(settings: Settings, state: State<'_, AppState>) -> Result<(), String> {
    let store = state.settings_store.read();
    store.update(settings).map_err(|e| e.to_string())
}

/// Return captured log entries for in-app log viewer
#[tauri::command]
fn get_app_logs() -> Vec<log_capture::LogEntry> {
    log_capture::get_entries()
}

/// Export captured logs to a timestamped file on the Desktop (or home dir).
/// Returns the file path written.
#[tauri::command]
fn export_logs() -> Result<String, String> {
    let entries = log_capture::get_entries();

    // Build text content
    let mut content = String::new();
    for e in &entries {
        // Convert Unix ms to readable timestamp
        let secs = e.timestamp / 1000;
        let ms = e.timestamp % 1000;
        content.push_str(&format!(
            "[{}.{:03}] {} {} — {}\n",
            secs, ms, e.level, e.target, e.message
        ));
    }

    // Choose export path: ~/Desktop if it exists, else home dir
    let base = dirs::desktop_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Format as YYYYMMDD-HHMMSS (UTC approximation)
    let day = now / 86400;
    let time = now % 86400;
    let date_str = format!("{}-{:02}:{:02}:{:02}",
        1970 + day / 365, (time / 3600) % 24, (time / 60) % 60, time % 60);
    let filename = format!("airtype-logs-{}.txt", now);
    let path = base.join(&filename);

    std::fs::write(&path, &content)
        .map_err(|e| format!("Failed to write log file: {}", e))?;

    let _ = date_str; // suppress unused warning
    log::info!("Exported {} log entries to {:?}", entries.len(), path);
    Ok(path.to_string_lossy().to_string())
}

/// Re-register all hotkeys from current settings. Call after saving settings that changed hotkeys.
#[tauri::command]
fn update_hotkeys(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let settings = state.get_settings();
    let english_hotkey = settings.hotkey_english.clone();
    let hebrew_hotkey = settings.hotkey_hebrew.clone();
    let hotkey_mode = settings.hotkey_mode;

    // 1. Unregister all existing global shortcuts
    let _ = state.hotkey_manager.unregister_all(&app);

    // 2. Clear modifier-only hotkey registrations (the listener thread keeps running)
    state.keyboard_listener.clear_all_modifier_hotkeys();

    // 3. Re-register non-modifier hotkeys via global shortcut plugin
    let english_is_modifier = is_modifier_only_hotkey(&english_hotkey);
    let hebrew_is_modifier = is_modifier_only_hotkey(&hebrew_hotkey);

    if !english_is_modifier && !english_hotkey.is_empty() {
        let config = hotkeys::HotkeyConfig::new(
            &english_hotkey,
            hotkeys::HotkeyAction::RecordEnglish,
            hotkeys::HotkeyMode::from(hotkey_mode),
        );
        if let Err(e) = state.hotkey_manager.register_shortcut(&app, config) {
            log::error!("Failed to register English hotkey '{}': {}", english_hotkey, e);
            return Err(format!("Failed to register English hotkey: {}", e));
        }
        log::info!("Registered English hotkey: {}", english_hotkey);
    }

    if !hebrew_is_modifier && !hebrew_hotkey.is_empty() {
        let config = hotkeys::HotkeyConfig::new(
            &hebrew_hotkey,
            hotkeys::HotkeyAction::RecordHebrew,
            hotkeys::HotkeyMode::from(hotkey_mode),
        );
        if let Err(e) = state.hotkey_manager.register_shortcut(&app, config) {
            log::error!("Failed to register Hebrew hotkey '{}': {}", hebrew_hotkey, e);
            return Err(format!("Failed to register Hebrew hotkey: {}", e));
        }
        log::info!("Registered Hebrew hotkey: {}", hebrew_hotkey);
    }

    // 4. Re-register modifier-only hotkeys
    let keyboard_listener = state.keyboard_listener.clone();
    let app_en = app.clone();
    if let Some(modifier) = ModifierKey::from_str(&english_hotkey) {
        let mode = hotkey_mode;
        keyboard_listener.register_modifier_hotkey(modifier, move |_key, pressed| {
            let phase = app_en.state::<AppState>().current_phase();
            if pressed {
                match (mode, phase) {
                    (_, RecordingPhase::Processing) => {}
                    (settings::HotkeyMode::Toggle, RecordingPhase::Recording) => {
                        let _ = app_en.emit("hotkey-event", &HotkeyEvent::RecordingStop);
                    }
                    _ => {
                        prewarm_capture(&app_en, "en");
                        let _ = app_en.emit("hotkey-event", &HotkeyEvent::RecordingStart { language: "en".to_string() });
                    }
                }
            } else if mode == settings::HotkeyMode::Hold && phase == RecordingPhase::Recording {
                let _ = app_en.emit("hotkey-event", &HotkeyEvent::RecordingStop);
            }
        });
        log::info!("Registered modifier English hotkey: {:?}", modifier);
    }

    if let Some(modifier) = ModifierKey::from_str(&hebrew_hotkey) {
        let mode = hotkey_mode;
        let app_he = app.clone();
        keyboard_listener.register_modifier_hotkey(modifier, move |_key, pressed| {
            let phase = app_he.state::<AppState>().current_phase();
            if pressed {
                match (mode, phase) {
                    (_, RecordingPhase::Processing) => {}
                    (settings::HotkeyMode::Toggle, RecordingPhase::Recording) => {
                        let _ = app_he.emit("hotkey-event", &HotkeyEvent::RecordingStop);
                    }
                    _ => {
                        prewarm_capture(&app_he, "he");
                        let _ = app_he.emit("hotkey-event", &HotkeyEvent::RecordingStart { language: "he".to_string() });
                    }
                }
            } else if mode == settings::HotkeyMode::Hold && phase == RecordingPhase::Recording {
                let _ = app_he.emit("hotkey-event", &HotkeyEvent::RecordingStop);
            }
        });
        log::info!("Registered modifier Hebrew hotkey: {:?}", modifier);
    }

    // 5. Ensure the keyboard listener is running if any modifier hotkeys are registered
    if english_is_modifier || hebrew_is_modifier {
        keyboard_listener.start(); // no-op if already running
    }

    log::info!("Hotkeys updated successfully");
    Ok(())
}

/// Check whether this process is trusted for Accessibility (macOS only).
/// On non-macOS platforms this always returns true.
#[tauri::command]
fn check_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            fn AXIsProcessTrusted() -> bool;
        }
        unsafe { AXIsProcessTrusted() }
    }
    #[cfg(not(target_os = "macos"))]
    true
}

/// Open System Settings → Privacy & Security → Accessibility so the user can
/// grant the required permission. macOS-only; no-op on other platforms.
#[tauri::command]
fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

/// Whether this process has Input Monitoring access (needed for the low-level
/// keyboard listener that powers bare-modifier hotkeys). macOS-only.
#[cfg(target_os = "macos")]
fn input_monitoring_granted() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightListenEventAccess() -> bool;
    }
    unsafe { CGPreflightListenEventAccess() }
}

/// Check whether this process has Input Monitoring access (macOS only).
/// On non-macOS platforms this always returns true.
#[tauri::command]
fn check_input_monitoring_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        input_monitoring_granted()
    }
    #[cfg(not(target_os = "macos"))]
    true
}

/// Returns true only when the user actually relies on a bare-modifier hotkey
/// (which uses the low-level listener) AND Input Monitoring is not yet granted.
/// This keeps the banner from nagging users who only use combo hotkeys.
#[tauri::command]
fn needs_input_monitoring(state: State<'_, AppState>) -> bool {
    #[cfg(target_os = "macos")]
    {
        let s = state.get_settings();
        let uses_modifier = is_modifier_only_hotkey(&s.hotkey_english)
            || is_modifier_only_hotkey(&s.hotkey_hebrew);
        uses_modifier && !input_monitoring_granted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = state;
        false
    }
}

/// Open System Settings → Privacy & Security → Input Monitoring. Also issues a
/// request so the app is registered in the list (and prompted on first use).
/// macOS-only; no-op on other platforms.
#[tauri::command]
fn open_input_monitoring_settings() {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGRequestListenEventAccess() -> bool;
        }
        // Registers AirType in the Input Monitoring list / triggers the prompt.
        unsafe { CGRequestListenEventAccess() };
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
            .spawn();
    }
}

/// Validate OpenAI API key by making a lightweight request
#[tauri::command]
async fn validate_openai_key(api_key: String) -> Result<bool, String> {
    if api_key.is_empty() {
        return Ok(false);
    }
    
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    
    Ok(resp.status().is_success())
}

/// Validate RunPod API key and endpoint ID
#[tauri::command]
async fn validate_runpod_key(api_key: String, endpoint_id: String) -> Result<bool, String> {
    if api_key.is_empty() || endpoint_id.is_empty() {
        return Ok(false);
    }
    Ok(validate_runpod(&api_key, &endpoint_id).await)
}
/// Run a bundled transcription test against the configured endpoints.
#[tauri::command]
async fn run_transcription_test(language: String, state: State<'_, AppState>) -> Result<String, String> {
    let settings = state.get_settings();

    match language.to_lowercase().as_str() {
        "en" => {
            match settings.english_endpoint_type {
                settings::EnglishEndpointType::SharedRunPod => {
                    let rp_key = settings.runpod_api_key
                        .filter(|k| !k.is_empty())
                        .ok_or_else(|| "Default RunPod API key not set".to_string())?;
                    let rp_endpoint = settings.runpod_endpoint_id
                        .filter(|k| !k.is_empty())
                        .ok_or_else(|| "Default RunPod Endpoint ID not set".to_string())?;
                    transcribe_audio_wav(&rp_key, &rp_endpoint, &english_test_wav(), "en", |_| {}).await
                }
                settings::EnglishEndpointType::CustomRunPod => {
                    let rp_key = settings.english_custom_runpod_api_key
                        .filter(|k| !k.is_empty())
                        .ok_or_else(|| "Custom RunPod API key for English not set".to_string())?;
                    let rp_endpoint = settings.english_custom_runpod_endpoint_id
                        .filter(|k| !k.is_empty())
                        .ok_or_else(|| "Custom RunPod Endpoint ID for English not set".to_string())?;
                    transcribe_audio_wav(&rp_key, &rp_endpoint, &english_test_wav(), "en", |_| {}).await
                }
                settings::EnglishEndpointType::OpenAI => {
                    let api_key = settings.english_openai_api_key
                        .filter(|k| !k.is_empty())
                        .ok_or_else(|| "OpenAI API key not set".to_string())?;
                    transcribe_english(&api_key, english_test_wav()).await
                }
            }
        }
        "he" => {
            let rp_key = settings.runpod_api_key
                .filter(|k| !k.is_empty())
                .ok_or_else(|| "RunPod API key not set".to_string())?;
            let rp_endpoint = settings.runpod_endpoint_id
                .filter(|k| !k.is_empty())
                .ok_or_else(|| "RunPod Endpoint ID not set".to_string())?;
            transcribe_hebrew_wav(&rp_key, &rp_endpoint, hebrew_test_wav()).await
        }
        other => Err(format!("Unsupported test language: {}", other)),
    }
}



/// Get last transcription
#[tauri::command]
fn get_last_transcription(state: State<'_, AppState>) -> String {
    state.last_transcription.read().clone()
}

/// Clear the cached "last transcription" (used when the user dismisses the card)
#[tauri::command]
fn clear_last_transcription(state: State<'_, AppState>) {
    *state.last_transcription.write() = String::new();
}

/// Get the full transcription history, most recent first
#[tauri::command]
fn get_transcription_history(state: State<'_, AppState>) -> Vec<TranscriptionEntry> {
    state.history.all()
}

/// Delete a single history entry by id
#[tauri::command]
fn delete_transcription_entry(id: u64, state: State<'_, AppState>) {
    state.history.delete(id);
}

/// Clear the entire transcription history
#[tauri::command]
fn clear_transcription_history(state: State<'_, AppState>) {
    state.history.clear();
}

/// Check if currently recording
#[tauri::command]
fn is_recording(state: State<'_, AppState>) -> bool {
    *state.is_recording.read()
}

/// Set autostart on login
#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    
    let autostart_manager = app.autolaunch();
    if enabled {
        autostart_manager.enable().map_err(|e| e.to_string())?;
    } else {
        autostart_manager.disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ============================================================================
// Indicator Window Helpers - Run on main thread to avoid X11 crashes
// ============================================================================

/// Start microphone capture synchronously on the hotkey callback thread so the
/// OS starts collecting audio with minimal latency after the keypress. All
/// transcription/session setup happens afterwards; any samples captured in the
/// meantime are buffered and forwarded once the session is ready.
fn prewarm_capture<R: tauri::Runtime>(app: &AppHandle<R>, language: &str) {
    let state = app.state::<AppState>();
    // Block if already active or transcription is in flight.
    if state.current_phase() != RecordingPhase::Idle {
        return;
    }
    *state.recording_language.write() = language.to_string();

    let capture = match state.get_audio_capture() {
        Ok(c) => c,
        Err(e) => {
            surface_critical_error(app, format!("No microphone available: {}", e));
            return;
        }
    };
    if capture.is_recording() {
        return;
    }
    capture.clear_stream_sender();
    match capture.start_recording() {
        Ok(_) => {
            let gen = state.begin_recording();
            let _ = app.emit("recording-started", ());
            // Optimistic paint: show recording phase immediately on keypress.
            emit_phase(app, "recording", language, gen);
            log::info!("Mic capture pre-warmed (language={})", language);
        }
        Err(e) => {
            surface_critical_error(app, format!("Couldn't start microphone: {}", e));
        }
    }
}

/// Show the floating indicator window using current indicator settings.
/// `gen` is the session generation so the indicator knows which session this belongs to.
fn show_indicator<R: tauri::Runtime>(app: &AppHandle<R>, language: &str, gen: u64) {
    log::info!("Showing indicator for language: {}", language);

    // Emit unified phase event so both windows update instantly.
    emit_phase(app, "recording", language, gen);

    position_and_show_indicator(app);
}

/// Position the floating indicator window per current settings and show it.
/// Does not emit any phase — the caller decides what state the indicator shows.
fn position_and_show_indicator<R: tauri::Runtime>(app: &AppHandle<R>) {
    let settings = app.state::<AppState>().get_settings();
    let win_w = settings.indicator_width as f64;
    let win_h = settings.indicator_height as f64;
    let bottom_offset = settings.indicator_bottom_offset;
    let x_offset = settings.indicator_x_offset;
    let align = settings.indicator_align;

    let app_clone = app.clone();

    // Run window operations on main thread to avoid X11 threading issues
    let _ = app.run_on_main_thread(move || {
        if let Some(indicator) = app_clone.get_webview_window("indicator") {
            let _ = indicator.set_size(tauri::Size::Logical(tauri::LogicalSize { width: win_w, height: win_h }));

            if let Ok(Some(monitor)) = indicator.primary_monitor() {
                let size = monitor.size();
                let scale = monitor.scale_factor();
                let logical_w = size.width as f64 / scale;
                let logical_h = size.height as f64 / scale;

                let x = match align {
                    IndicatorAlign::Left   => x_offset,
                    IndicatorAlign::Center => (logical_w - win_w) / 2.0 + x_offset,
                    IndicatorAlign::Right  => logical_w - win_w + x_offset,
                };
                let y = logical_h - win_h - bottom_offset;

                log::info!("Indicator position: ({}, {}) size: {}x{}", x, y, win_w, win_h);
                let _ = indicator.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
            }

            let _ = indicator.show();
        }
    });
}

/// Resize the main window live (used when the user edits the default size in
/// Settings → Advanced). The configured size is also applied on next startup.
#[tauri::command]
fn set_main_window_size(app: AppHandle, width: u32, height: u32) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
            width: width as f64,
            height: height as f64,
        }));
    }
}

/// Briefly show the indicator so the user can preview the current position/size settings.
#[tauri::command]
fn preview_indicator(app: AppHandle, state: State<'_, AppState>) {
    let language = state.recording_language.read().clone();
    let gen = state.current_generation();
    show_indicator(&app, &language, gen);
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        hide_indicator(&app_clone, gen);
    });
}

// ── Self-update ───────────────────────────────────────────────────────────────

/// Metadata about an available update, sent to the frontend.
#[derive(Clone, serde::Serialize)]
pub struct UpdateInfo {
    /// Version offered by the update manifest (e.g. "1.2.0")
    pub version: String,
    /// Version currently running
    pub current_version: String,
    /// Release notes / changelog body, if the manifest provides one
    pub notes: Option<String>,
    /// Publish date from the manifest, if present
    pub date: Option<String>,
}

/// Query the update endpoint and return info about an available update, or
/// `None` if the running version is already current.
#[tauri::command]
async fn check_for_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            log::info!("Update available: {} (current {})", update.version, update.current_version);
            Ok(Some(UpdateInfo {
                version: update.version.clone(),
                current_version: update.current_version.clone(),
                notes: update.body.clone(),
                date: update.date.map(|d| d.to_string()),
            }))
        }
        Ok(None) => {
            log::info!("No update available — already on the latest version");
            Ok(None)
        }
        Err(e) => Err(format!("Update check failed: {}", e)),
    }
}

/// Download and install the available update, emitting `update-progress`
/// (0–100) as it downloads, then relaunch the app.
#[tauri::command]
async fn download_and_install_update(app: AppHandle) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| format!("Update check failed: {}", e))?
        .ok_or_else(|| "No update available".to_string())?;

    log::info!("Downloading update {}...", update.version);

    let downloaded = Arc::new(AtomicU64::new(0));
    let app_progress = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                let so_far = downloaded.fetch_add(chunk as u64, Ordering::Relaxed) + chunk as u64;
                let pct = total
                    .map(|t| (so_far as f64 / t as f64) * 100.0)
                    .unwrap_or(0.0);
                let _ = app_progress.emit("update-progress", pct);
            },
            || {
                log::info!("Update download finished, installing...");
            },
        )
        .await
        .map_err(|e| format!("Update install failed: {}", e))?;

    log::info!("Update installed — relaunching");
    app.restart();
}

/// On startup, check for an update (if enabled in settings) and notify the
/// frontend via the `update-available` event so it can surface a prompt.
fn spawn_startup_update_check(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // Give the UI a moment to mount its `update-available` listener.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        if !handle.state::<AppState>().get_settings().auto_check_updates {
            log::info!("Automatic update check disabled in settings");
            return;
        }

        let updater = match handle.updater() {
            Ok(u) => u,
            Err(e) => {
                log::warn!("Updater unavailable: {}", e);
                return;
            }
        };

        match updater.check().await {
            Ok(Some(update)) => {
                log::info!("Startup: update {} available", update.version);
                let _ = handle.emit(
                    "update-available",
                    UpdateInfo {
                        version: update.version.clone(),
                        current_version: update.current_version.clone(),
                        notes: update.body.clone(),
                        date: update.date.map(|d| d.to_string()),
                    },
                );
            }
            Ok(None) => log::info!("Startup: no update available"),
            Err(e) => log::warn!("Startup update check failed: {}", e),
        }
    });
}

/// Hide the floating indicator window. The `gen` guard ensures a delayed hide
/// from an old session never clobbers a new session that started in the meantime.
fn hide_indicator<R: tauri::Runtime>(app: &AppHandle<R>, gen: u64) {
    log::info!("Hiding indicator (gen={})", gen);
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        // Short done flash (~400 ms) then hide — but only if no new session started.
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let state = app_clone.state::<AppState>();
        if state.current_generation() != gen {
            log::info!("hide_indicator: stale gen {}, skipping hide", gen);
            return;
        }
        let _ = app_clone.emit("indicator-hide", ());
        let app_inner = app_clone.clone();
        let _ = app_clone.run_on_main_thread(move || {
            if let Some(indicator) = app_inner.get_webview_window("indicator") {
                let _ = indicator.hide();
            }
        });
    });
}

/// Bring the main window to the foreground. On macOS, `set_focus` alone does not
/// reliably raise the window when the app is in the background, so we also
/// re-activate the application and unminimize the window.
fn focus_main_window<R: tauri::Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    // Run on the main thread: tray clicks and the SettingsOpen hotkey can fire
    // off the main thread, and the AppKit activation below must be main-thread.
    let _ = app.clone().run_on_main_thread(move || {
        // On macOS, window.set_focus() alone does not reliably raise a background
        // app (especially from a status-bar click). Explicitly activate the
        // application first so the window actually comes to the foreground.
        #[cfg(target_os = "macos")]
        unsafe {
            use objc::runtime::{Object, YES};
            use objc::{class, msg_send, sel, sel_impl};
            let _ = app.show();
            let ns_app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![ns_app, activateIgnoringOtherApps: YES];
        }
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    });
}

// ============================================================================
// App Entry Point
// ============================================================================

use tauri_plugin_autostart::MacosLauncher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logger with in-memory capture (replaces plain env_logger::init)
    log_capture::init("info");

    log::info!("Starting AirType...");

    // Load settings first
    let settings_store = SettingsStore::new().expect("Failed to create settings store");
    let loaded_settings = settings_store.load().unwrap_or_default();
    
    log::info!("Loaded settings - English hotkey: {}, Hebrew hotkey: {}, Mode: {:?}", 
        loaded_settings.hotkey_english, 
        loaded_settings.hotkey_hebrew,
        loaded_settings.hotkey_mode);
    
    // Separate modifier-only hotkeys from regular hotkeys
    let english_is_modifier = is_modifier_only_hotkey(&loaded_settings.hotkey_english);
    let hebrew_is_modifier = is_modifier_only_hotkey(&loaded_settings.hotkey_hebrew);
    
    log::info!("English is modifier-only: {}, Hebrew is modifier-only: {}", 
        english_is_modifier, hebrew_is_modifier);
    
    // Create hotkey configs only for non-modifier hotkeys
    let mut hotkey_configs = Vec::new();
    if !english_is_modifier {
        hotkey_configs.push(hotkeys::HotkeyConfig::new(
            &loaded_settings.hotkey_english,
            hotkeys::HotkeyAction::RecordEnglish,
            hotkeys::HotkeyMode::from(loaded_settings.hotkey_mode),
        ));
    }
    if !hebrew_is_modifier {
        hotkey_configs.push(hotkeys::HotkeyConfig::new(
            &loaded_settings.hotkey_hebrew,
            hotkeys::HotkeyAction::RecordHebrew,
            hotkeys::HotkeyMode::from(loaded_settings.hotkey_mode),
        ));
    }

    // Create hotkey manager
    let hotkey_manager = Arc::new(HotkeyManager::new());
    
    // Create keyboard listener for modifier-only hotkeys
    let keyboard_listener = Arc::new(KeyboardListener::new());

    // Build global shortcut plugin with loaded settings (only non-modifier hotkeys)
    let shortcut_plugin = build_global_shortcut_plugin(hotkey_manager.clone(), hotkey_configs);

    // Create app state
    let app_state = AppState::new(hotkey_manager.clone(), keyboard_listener.clone());
    
    // Store settings for use in setup
    let english_hotkey = loaded_settings.hotkey_english.clone();
    let hebrew_hotkey = loaded_settings.hotkey_hebrew.clone();
    let hotkey_mode = loaded_settings.hotkey_mode;

    tauri::Builder::default()
        // Must be the first plugin: enforces a single running instance. When a
        // second launch is attempted, this fires in the already-running instance
        // (and the new process exits), so we just surface the existing window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            log::info!("Second instance attempted — focusing existing window");
            focus_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(shortcut_plugin)
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app_state)
        .on_window_event(|window, event| {
            // Closing the main window with the red X (or Cmd+W) should hide it,
            // not destroy it — otherwise the window can't be re-shown from the
            // dock/tray and the user has to quit to get it back. The app keeps
            // running in the tray; real quit goes through the tray "Quit" item.
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            get_settings,
            save_settings,
            get_last_transcription,
            clear_last_transcription,
            get_transcription_history,
            delete_transcription_entry,
            clear_transcription_history,
            is_recording,
            set_autostart,
            validate_openai_key,
            validate_runpod_key,
            run_transcription_test,
            get_app_logs,
            export_logs,
            update_hotkeys,
            check_accessibility_permission,
            open_accessibility_settings,
            check_input_monitoring_permission,
            needs_input_monitoring,
            open_input_monitoring_settings,
            set_main_window_size,
            preview_indicator,
            check_for_update,
            download_and_install_update,
        ])
        .setup(move |app| {
            log::info!("Setting up AirType...");

            // Create system tray
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let show_item = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            let tray_icon_bytes = include_bytes!("../icons/32x32.png");
            let tray_icon = tauri::image::Image::from_bytes(tray_icon_bytes)?;
            let _tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        log::info!("Quit requested from tray");
                        app.exit(0);
                    }
                    "show" => {
                        focus_main_window(app);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        focus_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // Set up modifier-only hotkeys using low-level keyboard listener
            {
                let state = app.state::<AppState>();
                let keyboard_listener = state.keyboard_listener.clone();
                let app_handle_for_modifiers = app.handle().clone();
                
                // Check if English hotkey is a modifier-only key
                if let Some(modifier) = ModifierKey::from_str(&english_hotkey) {
                    log::info!("Registering modifier-only hotkey for English: {:?}", modifier);
                    let app_clone = app_handle_for_modifiers.clone();
                    let mode = hotkey_mode;
                    keyboard_listener.register_modifier_hotkey(modifier, move |_key, pressed| {
                        log::info!("English modifier callback: pressed={}", pressed);
                        let phase = app_clone.state::<AppState>().current_phase();
                        if pressed {
                            match (mode, phase) {
                                (_, RecordingPhase::Processing) => {
                                    log::info!("Modifier EN: blocked — processing");
                                }
                                (settings::HotkeyMode::Toggle, RecordingPhase::Recording) => {
                                    log::info!("Toggle EN: stopping");
                                    let _ = app_clone.emit("hotkey-event", &HotkeyEvent::RecordingStop);
                                }
                                _ => {
                                    log::info!("Modifier EN: starting");
                                    prewarm_capture(&app_clone, "en");
                                    let _ = app_clone.emit("hotkey-event", &HotkeyEvent::RecordingStart { language: "en".to_string() });
                                }
                            }
                        } else if mode == settings::HotkeyMode::Hold && phase == RecordingPhase::Recording {
                            let _ = app_clone.emit("hotkey-event", &HotkeyEvent::RecordingStop);
                        }
                    });
                }

                // Check if Hebrew hotkey is a modifier-only key
                if let Some(modifier) = ModifierKey::from_str(&hebrew_hotkey) {
                    log::info!("Registering modifier-only hotkey for Hebrew: {:?}", modifier);
                    let app_clone = app_handle_for_modifiers.clone();
                    let mode = hotkey_mode;
                    keyboard_listener.register_modifier_hotkey(modifier, move |_key, pressed| {
                        log::info!("Hebrew modifier callback: pressed={}", pressed);
                        let phase = app_clone.state::<AppState>().current_phase();
                        if pressed {
                            match (mode, phase) {
                                (_, RecordingPhase::Processing) => {
                                    log::info!("Modifier HE: blocked — processing");
                                }
                                (settings::HotkeyMode::Toggle, RecordingPhase::Recording) => {
                                    log::info!("Toggle HE: stopping");
                                    let _ = app_clone.emit("hotkey-event", &HotkeyEvent::RecordingStop);
                                }
                                _ => {
                                    log::info!("Modifier HE: starting");
                                    prewarm_capture(&app_clone, "he");
                                    let _ = app_clone.emit("hotkey-event", &HotkeyEvent::RecordingStart { language: "he".to_string() });
                                }
                            }
                        } else if mode == settings::HotkeyMode::Hold && phase == RecordingPhase::Recording {
                            let _ = app_clone.emit("hotkey-event", &HotkeyEvent::RecordingStop);
                        }
                    });
                }
                
                // Start the keyboard listener if we have any modifier hotkeys
                if ModifierKey::from_str(&english_hotkey).is_some() || ModifierKey::from_str(&hebrew_hotkey).is_some() {
                    keyboard_listener.start();
                }
            }

            // Listen for hotkey events and handle recording
            let app_handle = app.handle().clone();
            app.listen("hotkey-event", move |event| {
                log::info!("Received hotkey-event: {}", event.payload());
                let payload_str = event.payload();
                // Tauri `emit` sometimes double-serializes objects from background threads
                // into JSON strings, e.g. `"{\"type\":\"RecordingStop\"}"`. 
                // We robustly try to parse the inner string first, falling back to raw payload.
                let parsed_event: Result<HotkeyEvent, _> = match serde_json::from_str::<String>(payload_str) {
                    Ok(inner_str) => serde_json::from_str(&inner_str),
                    Err(_) => serde_json::from_str(payload_str),
                };

                match parsed_event {
                    Ok(hotkey_event) => {
                        log::info!("Parsed hotkey event: {:?}", hotkey_event);
                        let app = app_handle.clone();
                        match hotkey_event {
                            HotkeyEvent::RecordingStart { language } => {
                                log::info!("Hotkey: Start recording in {}", language);
                                let state = app.state::<AppState>();

                                // Block if processing is in flight.
                                if state.current_phase() == RecordingPhase::Processing {
                                    log::info!("Hotkey RecordingStart: blocked — processing in flight");
                                    return;
                                }

                                *state.recording_language.write() = language.clone();

                                // Start microphone if prewarm didn't already do it.
                                if state.current_phase() == RecordingPhase::Idle {
                                    match state.get_audio_capture() {
                                        Ok(capture) => {
                                            if !capture.is_recording() {
                                                capture.clear_stream_sender();
                                                if let Err(e) = capture.start_recording() {
                                                    surface_critical_error(&app, format!("Couldn't start microphone: {}", e));
                                                    return;
                                                }
                                            }
                                            let gen = state.begin_recording();
                                            let _ = app.emit("recording-started", ());
                                            emit_phase(&app, "recording", &language, gen);
                                        }
                                        Err(e) => {
                                            surface_critical_error(&app, format!("No microphone available: {}", e));
                                            return;
                                        }
                                    }
                                }

                                let gen = state.current_generation();
                                show_indicator(&app, &language, gen);
                                let _ = app.emit("language-changed", &language);

                                tauri::async_runtime::spawn(async move {
                                    let state = app.state::<AppState>();
                                    if let Err(e) = start_recording(state, app.clone(), None).await {
                                        surface_critical_error(&app, e);
                                    }
                                });
                            }
                            HotkeyEvent::RecordingStop => {
                                log::info!("Hotkey: Stop recording");
                                let state = app.state::<AppState>();
                                if state.current_phase() == RecordingPhase::Recording {
                                    tauri::async_runtime::spawn(async move {
                                        let state = app.state::<AppState>();
                                        if let Err(e) = stop_recording(state, app.clone()).await {
                                            surface_critical_error(&app, e);
                                        }
                                    });
                                }
                            }
                            HotkeyEvent::SettingsOpen => {
                                log::info!("Hotkey: Open settings");
                                focus_main_window(&app);
                                let _ = app.emit("open-settings", ());
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to parse hotkey event: {}", e);
                    }
                }
            });

            // Ensure config directory exists
            let _ = std::fs::create_dir_all(SettingsStore::get_config_dir());

            // Apply start_minimized setting + configured default window size
            let state = app.state::<AppState>();
            let settings = state.get_settings();
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
                    width: settings.window_width as f64,
                    height: settings.window_height as f64,
                }));
                if settings.start_minimized {
                    let _ = window.hide();
                }
            }

            log::info!("AirType setup complete");

            // Kick off a background update check (respects the auto_check_updates
            // setting) and notify the UI if a newer version is available.
            spawn_startup_update_check(app.handle());

            // Check Accessibility permission (required for text injection on macOS).
            // Emit event so the UI can show a persistent warning if not yet granted.
            #[cfg(target_os = "macos")]
            {
                #[link(name = "ApplicationServices", kind = "framework")]
                extern "C" {
                    fn AXIsProcessTrusted() -> bool;
                }
                let trusted = unsafe { AXIsProcessTrusted() };
                if !trusted {
                    log::warn!("Accessibility permission not granted — text injection will not work. Grant access in System Settings → Privacy & Security → Accessibility.");
                    let _ = app.emit("accessibility-permission-needed", ());
                } else {
                    log::info!("Accessibility permission: granted");
                }

                // Input Monitoring is only required for bare-modifier hotkeys.
                let s = app.state::<AppState>().get_settings();
                let uses_modifier = is_modifier_only_hotkey(&s.hotkey_english)
                    || is_modifier_only_hotkey(&s.hotkey_hebrew);
                if uses_modifier && !input_monitoring_granted() {
                    log::warn!("Input Monitoring not granted — modifier-key hotkeys will not work. Grant access in System Settings → Privacy & Security → Input Monitoring.");
                    let _ = app.emit("input-monitoring-permission-needed", ());
                }
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // On macOS, clicking the Dock icon fires Reopen. Bring the main
            // window to the front whether or not it was already visible —
            // otherwise a window hidden behind other apps stays buried.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                focus_main_window(app_handle);
            }
            #[cfg(not(target_os = "macos"))]
            { let _ = (app_handle, event); }
        });
}
