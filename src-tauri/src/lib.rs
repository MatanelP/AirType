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
pub mod hotkeys;
pub mod injection;
pub mod log_capture;
pub mod settings;
pub mod transcription;

use audio::AudioCapture;
use hotkeys::{
    build_global_shortcut_plugin, is_modifier_only_hotkey, HotkeyEvent, HotkeyManager,
    KeyboardListener, ModifierKey,
};
use injection::TextInjector;
use settings::{EnglishEndpointType, Settings, SettingsStore};
use transcription::{
    encode_wav, english_test_wav, hebrew_test_wav, transcribe_audio, transcribe_audio_wav,
    transcribe_english, transcribe_hebrew_wav, validate_runpod,
};

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
// Tauri Commands
// ============================================================================

/// Start recording audio
#[tauri::command]
async fn start_recording(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("Starting recording (post-capture setup)...");

    let language = state.recording_language.read().clone();

    // Ensure capture is active.
    let capture = state.get_audio_capture()?;
    if !capture.is_recording() {
        capture.clear_stream_sender();
        capture
            .start_recording()
            .map_err(|e| format!("Failed to start recording: {}", e))?;
        let _ = app.emit("recording-started", ());
    }
    *state.is_recording.write() = true;

    log::info!("Recording in progress for language: {}", language);

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

    if samples.is_empty() {
        log::warn!("No audio samples captured");
        hide_indicator(&app);
        return Ok(String::new());
    }

    let language = state.recording_language.read().clone();
    log::info!("Captured {} audio samples for {} (Batch)", samples.len(), language);
    let _ = app.emit("transcribing", ());
    indicator_transcribing(&app);

    let transcription = if language == "he" {
        let rp_key = settings.runpod_api_key
            .filter(|k| !k.is_empty())
            .ok_or_else(|| "Hebrew default RunPod API key not set. Go to Settings to add it.".to_string())?;
        let rp_endpoint = settings.runpod_endpoint_id
            .filter(|k| !k.is_empty())
            .ok_or_else(|| "Hebrew default RunPod Endpoint ID not set. Go to Settings to add it.".to_string())?;
        
        transcribe_audio(&rp_key, &rp_endpoint, &samples, &language).await?
    } else {
        match settings.english_endpoint_type {
            EnglishEndpointType::SharedRunPod => {
                let rp_key = settings.runpod_api_key
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| "Default RunPod API key not set. Go to Settings to add it.".to_string())?;
                let rp_endpoint = settings.runpod_endpoint_id
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| "Default RunPod Endpoint ID not set. Go to Settings to add it.".to_string())?;
                
                transcribe_audio(&rp_key, &rp_endpoint, &samples, &language).await?
            }
            EnglishEndpointType::CustomRunPod => {
                let rp_key = settings.english_custom_runpod_api_key
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| "Custom RunPod API key for English not set. Go to Settings to add it.".to_string())?;
                let rp_endpoint = settings.english_custom_runpod_endpoint_id
                    .filter(|k| !k.is_empty())
                    .ok_or_else(|| "Custom RunPod Endpoint ID for English not set. Go to Settings to add it.".to_string())?;
                
                transcribe_audio(&rp_key, &rp_endpoint, &samples, &language).await?
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

    indicator_done(&app);
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
            if pressed {
                prewarm_capture(&app_en, "en");
            }
            let event = if pressed {
                HotkeyEvent::RecordingStart { language: "en".to_string() }
            } else if mode == settings::HotkeyMode::Hold {
                HotkeyEvent::RecordingStop
            } else {
                return;
            };
            let _ = app_en.emit("hotkey-event", &event);
        });
        log::info!("Registered modifier English hotkey: {:?}", modifier);
    }

    if let Some(modifier) = ModifierKey::from_str(&hebrew_hotkey) {
        let mode = hotkey_mode;
        let app_he = app.clone();
        keyboard_listener.register_modifier_hotkey(modifier, move |_key, pressed| {
            if pressed {
                prewarm_capture(&app_he, "he");
            }
            let event = if pressed {
                HotkeyEvent::RecordingStart { language: "he".to_string() }
            } else if mode == settings::HotkeyMode::Hold {
                HotkeyEvent::RecordingStop
            } else {
                return;
            };
            let _ = app_he.emit("hotkey-event", &event);
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
                    transcribe_audio_wav(&rp_key, &rp_endpoint, &english_test_wav(), "en").await
                }
                settings::EnglishEndpointType::CustomRunPod => {
                    let rp_key = settings.english_custom_runpod_api_key
                        .filter(|k| !k.is_empty())
                        .ok_or_else(|| "Custom RunPod API key for English not set".to_string())?;
                    let rp_endpoint = settings.english_custom_runpod_endpoint_id
                        .filter(|k| !k.is_empty())
                        .ok_or_else(|| "Custom RunPod Endpoint ID for English not set".to_string())?;
                    transcribe_audio_wav(&rp_key, &rp_endpoint, &english_test_wav(), "en").await
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
        .plugin(tauri_plugin_opener::init())
        .plugin(shortcut_plugin)
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            get_settings,
            save_settings,
            get_last_transcription,
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
                        let _ = app_clone.emit("hotkey-event", &event);
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
                        let _ = app_clone.emit("hotkey-event", &event);
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

                                // Store current recording language
                                *state.recording_language.write() = language.clone();

                                // Start the microphone if not already running
                                // (prewarm_capture may have already started it
                                // and set is_recording = true).
                                if !*state.is_recording.read() {
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
                                            }
                                            *state.is_recording.write() = true;
                                            let _ = app.emit("recording-started", ());
                                        }
                                        Err(e) => {
                                            log::error!("No audio capture: {}", e);
                                            let _ = app.emit("error", e);
                                            return;
                                        }
                                    }
                                }

                                // Show indicator after mic is live
                                show_indicator(&app, &language);

                                let _ = app.emit("language-changed", &language);

                                // Spawn the session setup (WebSocket connect, etc.).
                                // start_recording is idempotent — it handles
                                // the case where mic capture is already running.
                                tauri::async_runtime::spawn(async move {
                                    let state = app.state::<AppState>();
                                    if let Err(e) = start_recording(state, app.clone()).await {
                                        log::error!("Failed to start recording: {}", e);
                                        let _ = app.emit("error", e);
                                        hide_indicator(&app);
                                    }
                                });
                            }
                            HotkeyEvent::RecordingStop => {
                                log::info!("Hotkey: Stop recording");
                                let state = app.state::<AppState>();
                                if *state.is_recording.read() {
                                    // Update indicator to transcribing state (use global emit)
                                    indicator_transcribing(&app);
                                    
                                    tauri::async_runtime::spawn(async move {
                                        let state = app.state::<AppState>();
                                        match stop_recording(state, app.clone()).await {
                                            Ok(transcription) => {
                                                if !transcription.is_empty() {
                                                    hide_indicator(&app);
                                                } else {
                                                    hide_indicator(&app);
                                                }
                                            }
                                            Err(e) => {
                                                log::error!("Failed to stop recording: {}", e);
                                                let _ = app.emit("error", e);
                                                hide_indicator(&app);
                                            }
                                        }
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

            // Ensure config directory exists
            let _ = std::fs::create_dir_all(SettingsStore::get_config_dir());

            // Apply start_minimized setting
            let state = app.state::<AppState>();
            let settings = state.get_settings();
            if settings.start_minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            log::info!("AirType setup complete");

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
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
