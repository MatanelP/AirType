//! AirType - Voice to Text Desktop Application
//!
//! A cross-platform desktop app for live voice-to-text transcription.
//! Press a global hotkey anywhere to record voice, which is transcribed
//! and inserted at the cursor position.

use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Listener, Manager, State,
};

pub mod audio;
pub mod hotkeys;
pub mod injection;
pub mod models;
pub mod settings;
pub mod transcription;

use audio::AudioCapture;
use hotkeys::{
    build_global_shortcut_plugin, is_modifier_only_hotkey, HotkeyEvent, HotkeyManager,
    KeyboardListener, ModifierKey,
};
use injection::TextInjector;
use settings::{ModelSize, RecordingMode, Settings, SettingsStore, TranscriptionEngine};
use transcription::{
    english_test_wav, hebrew_test_wav, transcribe_english_test, transcribe_hebrew,
    transcribe_hebrew_wav, validate_runpod, OpenAIRealtimeTranscriber, WhisperTranscriber,
};

/// Application state shared across all Tauri commands
pub struct AppState {
    /// Audio capture instance
    pub audio: RwLock<Option<Arc<AudioCapture>>>,
    /// Whisper transcriber (lazy loaded)
    pub transcriber: RwLock<Option<Arc<WhisperTranscriber>>>,
    /// OpenAI Realtime session audio sender (when using OpenAI engine)
    pub openai_audio_tx: RwLock<Option<tokio::sync::mpsc::Sender<Vec<f32>>>>,
    /// Current recording language
    pub recording_language: RwLock<String>,
    /// Hotkey manager
    pub hotkey_manager: Arc<HotkeyManager>,
    /// Low-level keyboard listener for modifier-only hotkeys
    pub keyboard_listener: Arc<KeyboardListener>,
    /// Settings store
    pub settings_store: RwLock<SettingsStore>,
    /// Whether currently recording
    pub is_recording: RwLock<bool>,
    /// Last transcription result
    pub last_transcription: RwLock<String>,
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
        Self {
            audio: RwLock::new(audio),
            transcriber: RwLock::new(None),
            openai_audio_tx: RwLock::new(None),
            recording_language: RwLock::new("en".to_string()),
            hotkey_manager,
            keyboard_listener,
            settings_store: RwLock::new(settings_store),
            is_recording: RwLock::new(false),
            last_transcription: RwLock::new(String::new()),
        }
    }

    /// Get the current settings
    pub fn get_settings(&self) -> Settings {
        self.settings_store.read().get()
    }

    /// Get the model path based on settings
    pub fn get_model_path(&self) -> Option<PathBuf> {
        let settings = self.get_settings();
        if let Some(path) = settings.model_path {
            Some(path)
        } else {
            let models_dir = SettingsStore::get_models_dir();
            // Always use multilingual model to support both English and Hebrew
            let filename = settings.model_size.filename();
            let path = models_dir.join(filename);
            if path.exists() {
                Some(path)
            } else {
                None
            }
        }
    }

    /// Get the model path for a specific language
    /// Uses the same selected model for all languages
    pub fn get_model_path_for_language(&self, _language: &str) -> Option<PathBuf> {
        self.get_model_path()
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

    /// Ensure transcriber is loaded for a specific language
    pub fn ensure_transcriber_for_language(&self, language: &str) -> Result<(), String> {
        let model_path = self
            .get_model_path_for_language(language)
            .ok_or_else(|| "No model file found. Please download a Whisper model.".to_string())?;

        // Check if we need to reload (different model)
        let mut transcriber_guard = self.transcriber.write();
        
        // Always create a new transcriber with the correct model for the language
        let transcriber = WhisperTranscriber::new(&model_path);
        let _ = transcriber.set_language(language);
        *transcriber_guard = Some(Arc::new(transcriber));
        Ok(())
    }

    /// Ensure transcriber is loaded
    pub fn ensure_transcriber(&self) -> Result<(), String> {
        let mut transcriber_guard = self.transcriber.write();
        if transcriber_guard.is_some() {
            return Ok(());
        }

        let model_path = self
            .get_model_path()
            .ok_or_else(|| "No model file found. Please download a Whisper model.".to_string())?;

        let transcriber = WhisperTranscriber::new(&model_path);

        // Default to English, language is set per-recording via hotkey
        let _ = transcriber.set_language("en");

        *transcriber_guard = Some(Arc::new(transcriber));
        Ok(())
    }
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// Start recording audio
#[tauri::command]
async fn start_recording(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("Starting recording (post-capture setup)...");

    let settings = state.get_settings();
    let language = state.recording_language.read().clone();

    // Ensure capture is active. When triggered from a hotkey, capture has
    // already been started synchronously in the hotkey callback so that the
    // microphone begins collecting samples with sub-perceptual latency.
    // When triggered from the UI, start it here.
    let capture = state.get_audio_capture()?;
    if !capture.is_recording() {
        capture.clear_stream_sender();
        capture
            .start_recording()
            .map_err(|e| format!("Failed to start recording: {}", e))?;
        let _ = app.emit("recording-started", ());
    }
    *state.is_recording.write() = true;

    match settings.transcription_engine {
        TranscriptionEngine::OpenAI => {
            if language == "he" {
                // Hebrew: just capture audio, will use RunPod ivrit-ai batch API on stop
                let _rp_key = settings
                    .runpod_api_key
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| {
                        "RunPod API key not set. Go to Settings to add it.".to_string()
                    })?;
                let _rp_endpoint = settings
                    .runpod_endpoint_id
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| {
                        "RunPod Endpoint ID not set. Go to Settings to add it.".to_string()
                    })?;

                log::info!("Recording in progress (RunPod ivrit-ai for Hebrew)");
            } else {
                // English: OpenAI Realtime API for live streaming
                let api_key = settings
                    .openai_api_key
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| {
                        "OpenAI API key not set. Go to Settings → Transcription Engine."
                            .to_string()
                    })?;

                let transcriber = OpenAIRealtimeTranscriber::new(&api_key);
                transcriber.set_language(&language);

                let app_for_callback = app.clone();
                let callback = Arc::new(move |text: &str, is_final: bool| {
                    if is_final && !text.is_empty() {
                        log::info!("OpenAI transcription (final): {}", text);
                        let _ = app_for_callback.emit("transcription-complete", text);
                        let text_owned = text.to_string();
                        let _ = std::thread::spawn(move || {
                            if let Ok(mut injector) = TextInjector::new() {
                                let _ = injector.inject_text_clipboard(&text_owned);
                            }
                        });
                    } else if !text.is_empty() {
                        log::debug!("OpenAI transcription (delta): {}", text);
                        let _ = app_for_callback.emit("transcription-delta", text);
                    }
                });

                let audio_tx = transcriber.start_session(callback).await?;

                // Forward any samples already captured while the WebSocket was
                // being established, then stream subsequent samples live.
                capture.set_stream_sender_with_flush(audio_tx.clone());

                *state.openai_audio_tx.write() = Some(audio_tx);

                log::info!("Recording in progress (OpenAI Realtime)");
            }
        }
        TranscriptionEngine::LocalWhisper => {
            // Local Whisper path: transcriber is pre-loaded in the hotkey
            // handler. Re-check here in case this command came from the UI.
            state.ensure_transcriber()?;
            log::info!("Recording in progress (Local Whisper)");
        }
    }

    Ok(())
}

/// Stop recording and get transcription
#[tauri::command]
async fn stop_recording(state: State<'_, AppState>, app: AppHandle) -> Result<String, String> {
    log::info!("Stopping recording...");

    // Check if recording
    if !*state.is_recording.read() {
        return Err("Not recording".to_string());
    }

    let settings = state.get_settings();

    // Get audio samples and stop capture
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

    *state.is_recording.write() = false;
    let _ = app.emit("recording-stopped", ());

    let language = state.recording_language.read().clone();

    match settings.transcription_engine {
        TranscriptionEngine::OpenAI => {
            if language == "he" {
                // Hebrew: use HuggingFace ivrit-ai batch API
                if samples.is_empty() {
                    log::warn!("No audio samples captured");
                    hide_indicator(&app);
                    return Ok(String::new());
                }

                log::info!("Captured {} audio samples for Hebrew", samples.len());
                let _ = app.emit("transcribing", ());
                indicator_transcribing(&app);

                let rp_key = settings.runpod_api_key
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| "RunPod API key not set".to_string())?;
                let rp_endpoint = settings.runpod_endpoint_id
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| "RunPod Endpoint ID not set".to_string())?;

                let transcription = transcribe_hebrew(&rp_key, &rp_endpoint, &samples).await?;

                log::info!("Hebrew transcription: {}", transcription);
                *state.last_transcription.write() = transcription.clone();
                let _ = app.emit("transcription-complete", &transcription);

                if !transcription.is_empty() {
                    log::info!("Injecting text: {}", transcription);
                    let inject_result = tokio::task::spawn_blocking(move || {
                        let mut injector =
                            TextInjector::new().map_err(|e| format!("Failed to create injector: {}", e))?;
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

                indicator_done(&app);
                Ok(state.last_transcription.read().clone())
            } else {
                // English: OpenAI Realtime handles transcription via callback
                state.openai_audio_tx.write().take();
                capture.clear_stream_sender();
                log::info!("Recording stopped (OpenAI). Transcription handled via WebSocket callback.");
                Ok(String::new())
            }
        }
        TranscriptionEngine::LocalWhisper => {
            if samples.is_empty() {
                log::warn!("No audio samples captured");
                return Ok(String::new());
            }

            log::info!("Captured {} audio samples", samples.len());

            // Transcribe - update indicator to show transcribing state
            let _ = app.emit("transcribing", ());
            indicator_transcribing(&app);

            let transcription = {
                let transcriber_guard = state.transcriber.read();
                let transcriber = transcriber_guard
                    .as_ref()
                    .ok_or_else(|| "Transcriber not initialized".to_string())?;

                transcriber
                    .transcribe(&samples)
                    .map_err(|e| format!("Transcription failed: {}", e))?
            };

            log::info!("Transcription: {}", transcription);

            // Store last transcription
            *state.last_transcription.write() = transcription.clone();

            // Emit completion event
            let _ = app.emit("transcription-complete", &transcription);

            // Inject text at cursor if not empty
            if !transcription.is_empty() {
                log::info!("Injecting text: {}", transcription);

                let inject_result = tokio::task::spawn_blocking(move || {
                    let mut injector =
                        TextInjector::new().map_err(|e| format!("Failed to create injector: {}", e))?;
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

            Ok(state.last_transcription.read().clone())
        }
    }
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

/// Run a bundled transcription test against the configured paid endpoints.
#[tauri::command]
async fn run_transcription_test(language: String, state: State<'_, AppState>) -> Result<String, String> {
    let settings = state.get_settings();

    match language.to_lowercase().as_str() {
        "en" => {
            let api_key = settings
                .openai_api_key
                .filter(|k| !k.is_empty())
                .ok_or_else(|| "OpenAI API key not set".to_string())?;
            transcribe_english_test(&api_key, english_test_wav()).await
        }
        "he" => {
            let rp_key = settings
                .runpod_api_key
                .filter(|k| !k.is_empty())
                .ok_or_else(|| "RunPod API key not set".to_string())?;
            let rp_endpoint = settings
                .runpod_endpoint_id
                .filter(|k| !k.is_empty())
                .ok_or_else(|| "RunPod Endpoint ID not set".to_string())?;
            transcribe_hebrew_wav(&rp_key, &rp_endpoint, hebrew_test_wav()).await
        }
        other => Err(format!("Unsupported test language: {}", other)),
    }
}

/// Set recording mode
#[tauri::command]
fn set_mode(mode: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut settings = state.get_settings();
    settings.recording_mode = match mode.to_lowercase().as_str() {
        "live" => RecordingMode::Live,
        "batch" => RecordingMode::Batch,
        _ => return Err(format!("Unknown mode: {}", mode)),
    };

    let store = state.settings_store.read();
    store.update(settings).map_err(|e| e.to_string())
}

/// Get last transcription
#[tauri::command]
fn get_last_transcription(state: State<'_, AppState>) -> String {
    state.last_transcription.read().clone()
}

/// Check if model exists
#[tauri::command]
fn check_model_exists(state: State<'_, AppState>) -> bool {
    state.get_model_path().is_some()
}

/// Get model directory path
#[tauri::command]
fn get_models_dir() -> String {
    SettingsStore::get_models_dir()
        .to_string_lossy()
        .to_string()
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

/// Get model status for all sizes
#[tauri::command]
fn get_model_status() -> Vec<serde_json::Value> {
    use serde_json::json;
    
    vec![
        json!({
            "size": "tiny",
            "name": "Tiny",
            "description": "Fastest, least accurate",
            "size_mb": models::model_size_mb(ModelSize::Tiny),
            "downloaded": models::model_exists(ModelSize::Tiny)
        }),
        json!({
            "size": "base",
            "name": "Base",
            "description": "Balanced speed/accuracy",
            "size_mb": models::model_size_mb(ModelSize::Base),
            "downloaded": models::model_exists(ModelSize::Base)
        }),
        json!({
            "size": "small",
            "name": "Small",
            "description": "Better accuracy",
            "size_mb": models::model_size_mb(ModelSize::Small),
            "downloaded": models::model_exists(ModelSize::Small)
        }),
        json!({
            "size": "medium",
            "name": "Medium",
            "description": "High accuracy",
            "size_mb": models::model_size_mb(ModelSize::Medium),
            "downloaded": models::model_exists(ModelSize::Medium)
        }),
        json!({
            "size": "large",
            "name": "Large",
            "description": "Best accuracy, slowest",
            "size_mb": models::model_size_mb(ModelSize::Large),
            "downloaded": models::model_exists(ModelSize::Large)
        }),
    ]
}

/// Download a specific model
#[tauri::command]
async fn download_model(app: AppHandle, size: String) -> Result<String, String> {
    let model_size = match size.as_str() {
        "tiny" => ModelSize::Tiny,
        "base" => ModelSize::Base,
        "small" => ModelSize::Small,
        "medium" => ModelSize::Medium,
        "large" => ModelSize::Large,
        _ => return Err(format!("Unknown model size: {}", size)),
    };
    
    // Check if already downloaded
    if models::model_exists(model_size) {
        return Ok(models::model_path(model_size).to_string_lossy().to_string());
    }
    
    let app_handle = app.clone();
    let size_for_progress = size.clone();
    let path = models::download_model(model_size, Some(move |downloaded, total| {
        let progress = if total > 0 {
            (downloaded as f64 / total as f64 * 100.0) as u32
        } else {
            0
        };
        let _ = app_handle.emit("model-download-progress", serde_json::json!({
            "size": size_for_progress,
            "downloaded": downloaded,
            "total": total,
            "progress": progress
        }));
    }))
    .await?;
    
    let _ = app.emit("model-download-complete", &size);
    Ok(path.to_string_lossy().to_string())
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
    if *state.is_recording.read() {
        return;
    }
    *state.recording_language.write() = language.to_string();

    let capture = match state.get_audio_capture() {
        Ok(c) => c,
        Err(e) => {
            log::error!("prewarm_capture: no audio device: {}", e);
            return;
        }
    };
    if capture.is_recording() {
        return;
    }
    capture.clear_stream_sender();
    match capture.start_recording() {
        Ok(_) => {
            *state.is_recording.write() = true;
            let _ = app.emit("recording-started", ());
            log::info!("Mic capture pre-warmed (language={})", language);
        }
        Err(e) => {
            log::error!("prewarm_capture: failed to start mic: {}", e);
        }
    }
}

/// Show the floating indicator window at bottom center of screen
fn show_indicator<R: tauri::Runtime>(app: &AppHandle<R>, language: &str) {
    log::info!("Showing indicator for language: {}", language);
    
    let _lang = language.to_string();
    let app_clone = app.clone();
    
    // Emit event first (global emit works better)
    let _ = app.emit("indicator-show", serde_json::json!({ "language": language }));
    
    // Run window operations on main thread to avoid X11 threading issues
    let _ = app.run_on_main_thread(move || {
        if let Some(indicator) = app_clone.get_webview_window("indicator") {
            // Position at bottom center of primary monitor
            if let Ok(Some(monitor)) = indicator.primary_monitor() {
                let size = monitor.size();
                let scale = monitor.scale_factor();
                let logical_w = size.width as f64 / scale;
                let logical_h = size.height as f64 / scale;
                
                // Window is 160x48 (includes padding for the shadow + pill)
                let x = (logical_w - 160.0) / 2.0;
                let y = logical_h - 48.0 - 60.0;
                
                log::info!("Indicator position: ({}, {})", x, y);
                let _ = indicator.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
            }
            
            // Show window
            let _ = indicator.show();
        }
    });
}

/// Update indicator to show "Transcribing" state
fn indicator_transcribing<R: tauri::Runtime>(app: &AppHandle<R>) {
    log::info!("Indicator: Transcribing...");
    let _ = app.emit("indicator-transcribing", ());
}

/// Show "Done!" state on indicator
fn indicator_done<R: tauri::Runtime>(app: &AppHandle<R>) {
    log::info!("Indicator: Done!");
    let _ = app.emit("indicator-done", ());
}

/// Hide the floating indicator window
fn hide_indicator<R: tauri::Runtime>(app: &AppHandle<R>) {
    log::info!("Hiding indicator");
    indicator_done(app);
    
    // Hide after delay, on main thread
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let _ = app_clone.emit("indicator-hide", ());
        
        let app_inner = app_clone.clone();
        let _ = app_clone.run_on_main_thread(move || {
            if let Some(indicator) = app_inner.get_webview_window("indicator") {
                let _ = indicator.hide();
            }
        });
    });
}

// ============================================================================
// App Entry Point
// ============================================================================

use tauri_plugin_autostart::MacosLauncher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

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
        .plugin(tauri_plugin_opener::init())
        .plugin(shortcut_plugin)
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            get_settings,
            save_settings,
            set_mode,
            get_last_transcription,
            check_model_exists,
            get_models_dir,
            is_recording,
            set_autostart,
            get_model_status,
            download_model,
            validate_openai_key,
            validate_runpod_key,
            run_transcription_test,
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
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
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
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
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
                        if pressed {
                            // Start the microphone synchronously on the listener
                            // thread so that audio capture begins within
                            // milliseconds of the physical keypress. Any
                            // buffered samples will be forwarded to the
                            // transcription session once it is set up.
                            prewarm_capture(&app_clone, "en");
                        }
                        let event = if pressed {
                            HotkeyEvent::RecordingStart { language: "en".to_string() }
                        } else if mode == settings::HotkeyMode::Hold {
                            HotkeyEvent::RecordingStop
                        } else {
                            return; // Toggle mode only responds to press
                        };
                        log::info!("Emitting hotkey event: {:?}", event);
                        let _ = app_clone.emit("hotkey-event", serde_json::to_string(&event).unwrap());
                    });
                }
                
                // Check if Hebrew hotkey is a modifier-only key
                if let Some(modifier) = ModifierKey::from_str(&hebrew_hotkey) {
                    log::info!("Registering modifier-only hotkey for Hebrew: {:?}", modifier);
                    let app_clone = app_handle_for_modifiers.clone();
                    let mode = hotkey_mode;
                    keyboard_listener.register_modifier_hotkey(modifier, move |_key, pressed| {
                        log::info!("Hebrew modifier callback: pressed={}", pressed);
                        if pressed {
                            prewarm_capture(&app_clone, "he");
                        }
                        let event = if pressed {
                            HotkeyEvent::RecordingStart { language: "he".to_string() }
                        } else if mode == settings::HotkeyMode::Hold {
                            HotkeyEvent::RecordingStop
                        } else {
                            return; // Toggle mode only responds to press
                        };
                        log::info!("Emitting hotkey event: {:?}", event);
                        let _ = app_clone.emit("hotkey-event", serde_json::to_string(&event).unwrap());
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
                match serde_json::from_str::<HotkeyEvent>(event.payload()) {
                    Ok(hotkey_event) => {
                        log::info!("Parsed hotkey event: {:?}", hotkey_event);
                        let app = app_handle.clone();
                        match hotkey_event {
                            HotkeyEvent::RecordingStart { language } => {
                                log::info!("Hotkey: Start recording in {}", language);
                                let state = app.state::<AppState>();
                                if !*state.is_recording.read() {
                                    // Store current recording language
                                    *state.recording_language.write() = language.clone();

                                    // Start the microphone *immediately* so the
                                    // OS begins capturing audio within a few ms
                                    // of the hotkey press. The transcriber /
                                    // network session is set up afterwards; any
                                    // audio captured in the meantime is
                                    // buffered and flushed to the session once
                                    // it is ready.
                                    match state.get_audio_capture() {
                                        Ok(capture) => {
                                            if !capture.is_recording() {
                                                capture.clear_stream_sender();
                                                if let Err(e) = capture.start_recording() {
                                                    log::error!(
                                                        "Failed to start mic capture: {}",
                                                        e
                                                    );
                                                    let _ = app.emit("error", e.to_string());
                                                    return;
                                                }
                                                *state.is_recording.write() = true;
                                                let _ = app.emit("recording-started", ());
                                            }
                                        }
                                        Err(e) => {
                                            log::error!("No audio capture: {}", e);
                                            let _ = app.emit("error", e);
                                            return;
                                        }
                                    }

                                    // Show indicator after mic is live
                                    show_indicator(&app, &language);

                                    let settings = state.get_settings();

                                    // For local whisper, load the correct model for the language
                                    if settings.transcription_engine == TranscriptionEngine::LocalWhisper {
                                        if let Err(e) = state.ensure_transcriber_for_language(&language) {
                                            log::error!("Failed to load transcriber: {}", e);
                                            let _ = app.emit("error", e);
                                            hide_indicator(&app);
                                            return;
                                        }
                                    }

                                    let _ = app.emit("language-changed", &language);

                                    tauri::async_runtime::spawn(async move {
                                        let state = app.state::<AppState>();
                                        if let Err(e) = start_recording(state, app.clone()).await {
                                            log::error!("Failed to start recording: {}", e);
                                            let _ = app.emit("error", e);
                                            hide_indicator(&app);
                                        }
                                    });
                                }
                            }
                            HotkeyEvent::RecordingStop => {
                                log::info!("Hotkey: Stop recording");
                                let state = app.state::<AppState>();
                                if *state.is_recording.read() {
                                    // Update indicator to transcribing state (use global emit)
                                    indicator_transcribing(&app);
                                    
                                    tauri::async_runtime::spawn(async move {
                                        let state = app.state::<AppState>();
                                        if let Err(e) = stop_recording(state, app.clone()).await {
                                            log::error!("Failed to stop recording: {}", e);
                                            let _ = app.emit("error", e);
                                        }
                                        // Hide indicator after transcription completes
                                        hide_indicator(&app);
                                    });
                                }
                            }
                            HotkeyEvent::SettingsOpen => {
                                log::info!("Hotkey: Open settings");
                                if let Some(window) = app.get_webview_window("main") {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
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

            // Ensure config directories exist
            let _ = std::fs::create_dir_all(SettingsStore::get_config_dir());
            let _ = std::fs::create_dir_all(SettingsStore::get_models_dir());

            // Apply start_minimized setting
            let state = app.state::<AppState>();
            let settings = state.get_settings();
            if settings.start_minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            log::info!("AirType setup complete");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
