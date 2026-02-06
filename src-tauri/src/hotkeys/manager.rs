//! Hotkey manager implementation using tauri-plugin-global-shortcut

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use thiserror::Error;

/// Errors that can occur in hotkey management
#[derive(Error, Debug)]
pub enum HotkeyError {
    #[error("Failed to parse hotkey string: {0}")]
    ParseError(String),
    #[error("Failed to register hotkey: {0}")]
    RegistrationError(String),
    #[error("Failed to unregister hotkey: {0}")]
    UnregistrationError(String),
    #[error("Unknown key code: {0}")]
    UnknownKeyCode(String),
    #[error("Hotkey not found: {0}")]
    NotFound(String),
}

/// Actions that can be triggered by hotkeys
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyAction {
    /// Start/stop recording in English
    RecordEnglish,
    /// Start/stop recording in Hebrew
    RecordHebrew,
    /// Open settings window
    OpenSettings,
}

/// Mode for how the hotkey operates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyMode {
    /// Recording is active while key is held down
    #[default]
    Hold,
    /// Press to start, press again to stop
    Toggle,
}

impl From<crate::settings::HotkeyMode> for HotkeyMode {
    fn from(mode: crate::settings::HotkeyMode) -> Self {
        match mode {
            crate::settings::HotkeyMode::Hold => HotkeyMode::Hold,
            crate::settings::HotkeyMode::Toggle => HotkeyMode::Toggle,
        }
    }
}

/// Configuration for a single hotkey
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    /// The hotkey string (e.g., "Ctrl+Shift+Space")
    pub shortcut: String,
    /// What action this hotkey triggers
    pub action: HotkeyAction,
    /// How the hotkey behaves
    pub mode: HotkeyMode,
    /// Whether this hotkey is currently enabled
    pub enabled: bool,
}

impl HotkeyConfig {
    pub fn new(shortcut: impl Into<String>, action: HotkeyAction, mode: HotkeyMode) -> Self {
        Self {
            shortcut: shortcut.into(),
            action,
            mode,
            enabled: true,
        }
    }
}

/// Events emitted by the hotkey system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum HotkeyEvent {
    /// Hotkey was pressed
    Pressed {
        action: HotkeyAction,
        shortcut: String,
    },
    /// Hotkey was released (only relevant for Hold mode)
    Released {
        action: HotkeyAction,
        shortcut: String,
    },
    /// Recording should start with specified language
    RecordingStart {
        language: String,
    },
    /// Recording should stop
    RecordingStop,
    /// Settings window should open
    SettingsOpen,
}

/// Internal state for tracking toggle mode
#[derive(Debug, Default)]
struct HotkeyState {
    /// Whether recording is currently active (for toggle mode)
    is_recording: bool,
}

/// Manager for global hotkeys
pub struct HotkeyManager {
    /// Registered hotkey configurations, keyed by shortcut string
    configs: RwLock<HashMap<String, HotkeyConfig>>,
    /// Internal state
    state: RwLock<HotkeyState>,
}

impl Default for HotkeyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HotkeyManager {
    /// Create a new hotkey manager
    pub fn new() -> Self {
        Self {
            configs: RwLock::new(HashMap::new()),
            state: RwLock::new(HotkeyState::default()),
        }
    }

    /// Add a hotkey configuration
    pub fn add_config(&self, config: HotkeyConfig) {
        let mut configs = self.configs.write();
        configs.insert(config.shortcut.clone(), config);
    }

    /// Remove a hotkey configuration
    pub fn remove_config(&self, shortcut: &str) -> Option<HotkeyConfig> {
        let mut configs = self.configs.write();
        configs.remove(shortcut)
    }

    /// Get a hotkey configuration
    pub fn get_config(&self, shortcut: &str) -> Option<HotkeyConfig> {
        let configs = self.configs.read();
        configs.get(shortcut).cloned()
    }

    /// Get all hotkey configurations
    pub fn get_all_configs(&self) -> Vec<HotkeyConfig> {
        let configs = self.configs.read();
        configs.values().cloned().collect()
    }

    /// Check if recording is currently active
    pub fn is_recording(&self) -> bool {
        self.state.read().is_recording
    }

    /// Set recording state
    pub fn set_recording(&self, recording: bool) {
        self.state.write().is_recording = recording;
    }

    /// Handle a hotkey event and return the appropriate action event
    pub fn handle_shortcut_event(
        &self,
        shortcut_str: &str,
        state: ShortcutState,
    ) -> Option<HotkeyEvent> {
        let config = self.get_config(shortcut_str)?;

        if !config.enabled {
            return None;
        }

        match config.mode {
            HotkeyMode::Hold => self.handle_hold_mode(&config, state),
            HotkeyMode::Toggle => self.handle_toggle_mode(&config, state),
        }
    }

    fn handle_hold_mode(&self, config: &HotkeyConfig, state: ShortcutState) -> Option<HotkeyEvent> {
        match state {
            ShortcutState::Pressed => {
                if !self.is_recording() {
                    self.set_recording(true);
                    Some(self.action_to_start_event(&config.action))
                } else {
                    None
                }
            }
            ShortcutState::Released => {
                if self.is_recording() {
                    self.set_recording(false);
                    Some(self.action_to_stop_event(&config.action))
                } else {
                    None
                }
            }
        }
    }

    fn handle_toggle_mode(
        &self,
        config: &HotkeyConfig,
        state: ShortcutState,
    ) -> Option<HotkeyEvent> {
        // Only respond to key press, not release
        if state != ShortcutState::Pressed {
            return None;
        }

        match &config.action {
            HotkeyAction::RecordEnglish | HotkeyAction::RecordHebrew => {
                let currently_recording = self.is_recording();
                self.set_recording(!currently_recording);

                if currently_recording {
                    Some(HotkeyEvent::RecordingStop)
                } else {
                    let language = match &config.action {
                        HotkeyAction::RecordEnglish => "en".to_string(),
                        HotkeyAction::RecordHebrew => "he".to_string(),
                        _ => "en".to_string(),
                    };
                    Some(HotkeyEvent::RecordingStart { language })
                }
            }
            HotkeyAction::OpenSettings => Some(HotkeyEvent::SettingsOpen),
        }
    }

    fn action_to_start_event(&self, action: &HotkeyAction) -> HotkeyEvent {
        match action {
            HotkeyAction::RecordEnglish => HotkeyEvent::RecordingStart {
                language: "en".to_string(),
            },
            HotkeyAction::RecordHebrew => HotkeyEvent::RecordingStart {
                language: "he".to_string(),
            },
            HotkeyAction::OpenSettings => HotkeyEvent::SettingsOpen,
        }
    }

    fn action_to_stop_event(&self, action: &HotkeyAction) -> HotkeyEvent {
        match action {
            HotkeyAction::RecordEnglish | HotkeyAction::RecordHebrew => HotkeyEvent::RecordingStop,
            HotkeyAction::OpenSettings => HotkeyEvent::SettingsOpen,
        }
    }

    /// Register a hotkey with Tauri's global shortcut system
    pub fn register_shortcut<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        config: HotkeyConfig,
    ) -> Result<(), HotkeyError> {
        let shortcut = parse_shortcut(&config.shortcut)?;

        app.global_shortcut()
            .register(shortcut)
            .map_err(|e| HotkeyError::RegistrationError(e.to_string()))?;

        self.add_config(config);
        Ok(())
    }

    /// Unregister a hotkey
    pub fn unregister_shortcut<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        shortcut_str: &str,
    ) -> Result<(), HotkeyError> {
        let shortcut = parse_shortcut(shortcut_str)?;

        app.global_shortcut()
            .unregister(shortcut)
            .map_err(|e| HotkeyError::UnregistrationError(e.to_string()))?;

        self.remove_config(shortcut_str);
        Ok(())
    }

    /// Unregister all hotkeys
    pub fn unregister_all<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), HotkeyError> {
        app.global_shortcut()
            .unregister_all()
            .map_err(|e| HotkeyError::UnregistrationError(e.to_string()))?;

        self.configs.write().clear();
        Ok(())
    }
}

/// Parse a hotkey string into a Shortcut
/// Supports formats like "Ctrl+Shift+Space", "CmdOrCtrl+R", "Alt+F1"
pub fn parse_shortcut(shortcut_str: &str) -> Result<Shortcut, HotkeyError> {
    let normalized = shortcut_str.trim().to_lowercase();
    let parts: Vec<&str> = normalized.split('+').map(|s| s.trim()).collect();

    if parts.is_empty() {
        return Err(HotkeyError::ParseError("Empty shortcut string".to_string()));
    }

    let mut modifiers = Modifiers::empty();
    let mut key_code = None;

    for part in parts {
        match part {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" | "option" => modifiers |= Modifiers::ALT,
            "super" | "meta" | "win" | "cmd" | "command" => modifiers |= Modifiers::META,
            "cmdorctrl" | "commandorcontrol" => {
                // On macOS use Meta (Cmd), on other platforms use Control
                #[cfg(target_os = "macos")]
                {
                    modifiers |= Modifiers::META;
                }
                #[cfg(not(target_os = "macos"))]
                {
                    modifiers |= Modifiers::CONTROL;
                }
            }
            key => {
                key_code = Some(parse_key_code(key)?);
            }
        }
    }

    let code = key_code.ok_or_else(|| {
        HotkeyError::ParseError("No key code found in shortcut string".to_string())
    })?;

    Ok(Shortcut::new(Some(modifiers), code))
}

/// Parse a key name into a Code
fn parse_key_code(key: &str) -> Result<Code, HotkeyError> {
    match key.to_lowercase().as_str() {
        // Letters
        "a" => Ok(Code::KeyA),
        "b" => Ok(Code::KeyB),
        "c" => Ok(Code::KeyC),
        "d" => Ok(Code::KeyD),
        "e" => Ok(Code::KeyE),
        "f" => Ok(Code::KeyF),
        "g" => Ok(Code::KeyG),
        "h" => Ok(Code::KeyH),
        "i" => Ok(Code::KeyI),
        "j" => Ok(Code::KeyJ),
        "k" => Ok(Code::KeyK),
        "l" => Ok(Code::KeyL),
        "m" => Ok(Code::KeyM),
        "n" => Ok(Code::KeyN),
        "o" => Ok(Code::KeyO),
        "p" => Ok(Code::KeyP),
        "q" => Ok(Code::KeyQ),
        "r" => Ok(Code::KeyR),
        "s" => Ok(Code::KeyS),
        "t" => Ok(Code::KeyT),
        "u" => Ok(Code::KeyU),
        "v" => Ok(Code::KeyV),
        "w" => Ok(Code::KeyW),
        "x" => Ok(Code::KeyX),
        "y" => Ok(Code::KeyY),
        "z" => Ok(Code::KeyZ),

        // Numbers (top row)
        "0" | "digit0" => Ok(Code::Digit0),
        "1" | "digit1" => Ok(Code::Digit1),
        "2" | "digit2" => Ok(Code::Digit2),
        "3" | "digit3" => Ok(Code::Digit3),
        "4" | "digit4" => Ok(Code::Digit4),
        "5" | "digit5" => Ok(Code::Digit5),
        "6" | "digit6" => Ok(Code::Digit6),
        "7" | "digit7" => Ok(Code::Digit7),
        "8" | "digit8" => Ok(Code::Digit8),
        "9" | "digit9" => Ok(Code::Digit9),

        // Function keys
        "f1" => Ok(Code::F1),
        "f2" => Ok(Code::F2),
        "f3" => Ok(Code::F3),
        "f4" => Ok(Code::F4),
        "f5" => Ok(Code::F5),
        "f6" => Ok(Code::F6),
        "f7" => Ok(Code::F7),
        "f8" => Ok(Code::F8),
        "f9" => Ok(Code::F9),
        "f10" => Ok(Code::F10),
        "f11" => Ok(Code::F11),
        "f12" => Ok(Code::F12),

        // Special keys
        "space" | " " => Ok(Code::Space),
        "enter" | "return" => Ok(Code::Enter),
        "tab" => Ok(Code::Tab),
        "escape" | "esc" => Ok(Code::Escape),
        "backspace" => Ok(Code::Backspace),
        "delete" | "del" => Ok(Code::Delete),
        "insert" | "ins" => Ok(Code::Insert),
        "home" => Ok(Code::Home),
        "end" => Ok(Code::End),
        "pageup" | "pgup" => Ok(Code::PageUp),
        "pagedown" | "pgdn" => Ok(Code::PageDown),

        // Arrow keys
        "up" | "arrowup" => Ok(Code::ArrowUp),
        "down" | "arrowdown" => Ok(Code::ArrowDown),
        "left" | "arrowleft" => Ok(Code::ArrowLeft),
        "right" | "arrowright" => Ok(Code::ArrowRight),

        // Punctuation
        "minus" | "-" => Ok(Code::Minus),
        "equal" | "equals" | "=" => Ok(Code::Equal),
        "bracketleft" | "[" => Ok(Code::BracketLeft),
        "bracketright" | "]" => Ok(Code::BracketRight),
        "backslash" | "\\" => Ok(Code::Backslash),
        "semicolon" | ";" => Ok(Code::Semicolon),
        "quote" | "'" => Ok(Code::Quote),
        "backquote" | "`" => Ok(Code::Backquote),
        "comma" | "," => Ok(Code::Comma),
        "period" | "." => Ok(Code::Period),
        "slash" | "/" => Ok(Code::Slash),

        // Numpad
        "numpad0" | "num0" => Ok(Code::Numpad0),
        "numpad1" | "num1" => Ok(Code::Numpad1),
        "numpad2" | "num2" => Ok(Code::Numpad2),
        "numpad3" | "num3" => Ok(Code::Numpad3),
        "numpad4" | "num4" => Ok(Code::Numpad4),
        "numpad5" | "num5" => Ok(Code::Numpad5),
        "numpad6" | "num6" => Ok(Code::Numpad6),
        "numpad7" | "num7" => Ok(Code::Numpad7),
        "numpad8" | "num8" => Ok(Code::Numpad8),
        "numpad9" | "num9" => Ok(Code::Numpad9),
        "numpadadd" | "numpadplus" => Ok(Code::NumpadAdd),
        "numpadsubtract" | "numpadminus" => Ok(Code::NumpadSubtract),
        "numpadmultiply" | "numpadstar" => Ok(Code::NumpadMultiply),
        "numpaddivide" | "numpadslash" => Ok(Code::NumpadDivide),
        "numpaddecimal" | "numpaddot" => Ok(Code::NumpadDecimal),
        "numpadenter" => Ok(Code::NumpadEnter),

        // Modifier keys as standalone keys (for single-key shortcuts)
        "altleft" | "leftalt" | "lalt" => Ok(Code::AltLeft),
        "altright" | "rightalt" | "ralt" | "altgr" => Ok(Code::AltRight),
        "controlleft" | "leftctrl" | "lctrl" => Ok(Code::ControlLeft),
        "controlright" | "rightctrl" | "rctrl" => Ok(Code::ControlRight),
        "shiftleft" | "leftshift" | "lshift" => Ok(Code::ShiftLeft),
        "shiftright" | "rightshift" | "rshift" => Ok(Code::ShiftRight),
        "metaleft" | "leftmeta" | "lmeta" | "leftsuper" | "lsuper" => Ok(Code::MetaLeft),
        "metaright" | "rightmeta" | "rmeta" | "rightsuper" | "rsuper" => Ok(Code::MetaRight),

        _ => Err(HotkeyError::UnknownKeyCode(key.to_string())),
    }
}

/// Convert a Shortcut back to a string representation
pub fn shortcut_to_string(shortcut: &Shortcut) -> String {
    let mut parts = Vec::new();

    let mods = shortcut.mods;
    if mods.contains(Modifiers::CONTROL) {
        parts.push("Ctrl");
    }
    if mods.contains(Modifiers::SHIFT) {
        parts.push("Shift");
    }
    if mods.contains(Modifiers::ALT) {
        parts.push("Alt");
    }
    if mods.contains(Modifiers::META) {
        #[cfg(target_os = "macos")]
        parts.push("Cmd");
        #[cfg(not(target_os = "macos"))]
        parts.push("Super");
    }

    parts.push(code_to_string(shortcut.key));
    parts.join("+")
}

fn code_to_string(code: Code) -> &'static str {
    match code {
        Code::KeyA => "A",
        Code::KeyB => "B",
        Code::KeyC => "C",
        Code::KeyD => "D",
        Code::KeyE => "E",
        Code::KeyF => "F",
        Code::KeyG => "G",
        Code::KeyH => "H",
        Code::KeyI => "I",
        Code::KeyJ => "J",
        Code::KeyK => "K",
        Code::KeyL => "L",
        Code::KeyM => "M",
        Code::KeyN => "N",
        Code::KeyO => "O",
        Code::KeyP => "P",
        Code::KeyQ => "Q",
        Code::KeyR => "R",
        Code::KeyS => "S",
        Code::KeyT => "T",
        Code::KeyU => "U",
        Code::KeyV => "V",
        Code::KeyW => "W",
        Code::KeyX => "X",
        Code::KeyY => "Y",
        Code::KeyZ => "Z",
        Code::Digit0 => "0",
        Code::Digit1 => "1",
        Code::Digit2 => "2",
        Code::Digit3 => "3",
        Code::Digit4 => "4",
        Code::Digit5 => "5",
        Code::Digit6 => "6",
        Code::Digit7 => "7",
        Code::Digit8 => "8",
        Code::Digit9 => "9",
        Code::F1 => "F1",
        Code::F2 => "F2",
        Code::F3 => "F3",
        Code::F4 => "F4",
        Code::F5 => "F5",
        Code::F6 => "F6",
        Code::F7 => "F7",
        Code::F8 => "F8",
        Code::F9 => "F9",
        Code::F10 => "F10",
        Code::F11 => "F11",
        Code::F12 => "F12",
        Code::Space => "Space",
        Code::Enter => "Enter",
        Code::Tab => "Tab",
        Code::Escape => "Escape",
        Code::Backspace => "Backspace",
        Code::Delete => "Delete",
        Code::Insert => "Insert",
        Code::Home => "Home",
        Code::End => "End",
        Code::PageUp => "PageUp",
        Code::PageDown => "PageDown",
        Code::ArrowUp => "Up",
        Code::ArrowDown => "Down",
        Code::ArrowLeft => "Left",
        Code::ArrowRight => "Right",
        // Modifier keys as standalone
        Code::AltLeft => "LeftAlt",
        Code::AltRight => "RightAlt",
        Code::ControlLeft => "LeftCtrl",
        Code::ControlRight => "RightCtrl",
        Code::ShiftLeft => "LeftShift",
        Code::ShiftRight => "RightShift",
        Code::MetaLeft => "LeftSuper",
        Code::MetaRight => "RightSuper",
        _ => "Unknown",
    }
}

/// Build the global shortcut plugin with the given hotkey manager
///
/// This function creates a configured global shortcut plugin that integrates
/// with the HotkeyManager and emits events to the frontend.
pub fn build_global_shortcut_plugin<R: Runtime>(
    manager: Arc<HotkeyManager>,
    default_configs: Vec<HotkeyConfig>,
) -> tauri::plugin::TauriPlugin<R> {
    // Collect shortcuts to register
    let shortcuts: Vec<Shortcut> = default_configs
        .iter()
        .filter(|c| c.enabled)
        .filter_map(|c| parse_shortcut(&c.shortcut).ok())
        .collect();

    // Add configs to manager
    for config in default_configs {
        manager.add_config(config);
    }

    let manager_clone = Arc::clone(&manager);

    let mut builder = tauri_plugin_global_shortcut::Builder::new();

    // Register each shortcut
    for shortcut in &shortcuts {
        log::info!("Registering global shortcut: {:?}", shortcut);
        builder = builder
            .with_shortcut(shortcut.clone())
            .expect("Failed to register shortcut");
    }

    builder
        .with_handler(move |app, shortcut, event| {
            let shortcut_str = shortcut_to_string(shortcut);

            log::debug!("Hotkey event: {} - {:?}", shortcut_str, event.state());

            if let Some(hotkey_event) =
                manager_clone.handle_shortcut_event(&shortcut_str, event.state())
            {
                // Emit the event to the frontend
                if let Err(e) = app.emit("hotkey-event", &hotkey_event) {
                    log::error!("Failed to emit hotkey event: {}", e);
                }

                // Also emit specific events for convenience
                match &hotkey_event {
                    HotkeyEvent::RecordingStart { .. } => {
                        let _ = app.emit("recording-started", ());
                    }
                    HotkeyEvent::RecordingStop => {
                        let _ = app.emit("recording-stopped", ());
                    }
                    HotkeyEvent::SettingsOpen => {
                        let _ = app.emit("settings-open", ());
                    }
                    _ => {}
                }
            }
        })
        .build()
}

/// Get default hotkey configurations
pub fn default_hotkey_configs() -> Vec<HotkeyConfig> {
    vec![
        HotkeyConfig::new(
            "Ctrl+Shift+E",
            HotkeyAction::RecordEnglish,
            HotkeyMode::Hold,
        ),
        HotkeyConfig::new(
            "Ctrl+Shift+H",
            HotkeyAction::RecordHebrew,
            HotkeyMode::Hold,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_shortcut_simple() {
        let shortcut = parse_shortcut("Ctrl+Shift+Space").unwrap();
        assert!(shortcut.mods.contains(Modifiers::CONTROL));
        assert!(shortcut.mods.contains(Modifiers::SHIFT));
        assert_eq!(shortcut.key, Code::Space);
    }

    #[test]
    fn test_parse_shortcut_case_insensitive() {
        let shortcut = parse_shortcut("ctrl+SHIFT+a").unwrap();
        assert!(shortcut.mods.contains(Modifiers::CONTROL));
        assert!(shortcut.mods.contains(Modifiers::SHIFT));
        assert_eq!(shortcut.key, Code::KeyA);
    }

    #[test]
    fn test_parse_shortcut_single_key() {
        let shortcut = parse_shortcut("F1").unwrap();
        assert_eq!(shortcut.key, Code::F1);
    }

    #[test]
    fn test_parse_shortcut_cmdorctrl() {
        let shortcut = parse_shortcut("CmdOrCtrl+R").unwrap();
        #[cfg(target_os = "macos")]
        assert!(shortcut.mods.contains(Modifiers::META));
        #[cfg(not(target_os = "macos"))]
        assert!(shortcut.mods.contains(Modifiers::CONTROL));
        assert_eq!(shortcut.key, Code::KeyR);
    }

    #[test]
    fn test_parse_shortcut_invalid() {
        let result = parse_shortcut("Ctrl+InvalidKey");
        assert!(result.is_err());
    }

    #[test]
    fn test_hotkey_manager_hold_mode() {
        let manager = HotkeyManager::new();
        manager.add_config(HotkeyConfig::new(
            "Ctrl+Shift+E",
            HotkeyAction::RecordEnglish,
            HotkeyMode::Hold,
        ));

        // Press should start recording
        let event = manager.handle_shortcut_event("Ctrl+Shift+E", ShortcutState::Pressed);
        assert!(matches!(event, Some(HotkeyEvent::RecordingStart { .. })));
        assert!(manager.is_recording());

        // Release should stop recording
        let event = manager.handle_shortcut_event("Ctrl+Shift+E", ShortcutState::Released);
        assert!(matches!(event, Some(HotkeyEvent::RecordingStop)));
        assert!(!manager.is_recording());
    }

    #[test]
    fn test_hotkey_manager_toggle_mode() {
        let manager = HotkeyManager::new();
        manager.add_config(HotkeyConfig::new(
            "Ctrl+Shift+H",
            HotkeyAction::RecordHebrew,
            HotkeyMode::Toggle,
        ));

        // First press should start recording
        let event = manager.handle_shortcut_event("Ctrl+Shift+H", ShortcutState::Pressed);
        assert!(matches!(event, Some(HotkeyEvent::RecordingStart { .. })));
        assert!(manager.is_recording());

        // Release should do nothing in toggle mode
        let event = manager.handle_shortcut_event("Ctrl+Shift+H", ShortcutState::Released);
        assert!(event.is_none());
        assert!(manager.is_recording());

        // Second press should stop recording
        let event = manager.handle_shortcut_event("Ctrl+Shift+H", ShortcutState::Pressed);
        assert!(matches!(event, Some(HotkeyEvent::RecordingStop)));
        assert!(!manager.is_recording());
    }

    #[test]
    fn test_hotkey_config_disabled() {
        let manager = HotkeyManager::new();
        let mut config = HotkeyConfig::new(
            "Ctrl+Shift+E",
            HotkeyAction::RecordEnglish,
            HotkeyMode::Hold,
        );
        config.enabled = false;
        manager.add_config(config);

        // Disabled hotkey should not trigger events
        let event = manager.handle_shortcut_event("Ctrl+Shift+E", ShortcutState::Pressed);
        assert!(event.is_none());
    }

    #[test]
    fn test_shortcut_roundtrip() {
        let original = "Ctrl+Shift+Space";
        let shortcut = parse_shortcut(original).unwrap();
        let result = shortcut_to_string(&shortcut);
        assert_eq!(result, "Ctrl+Shift+Space");
    }
}
