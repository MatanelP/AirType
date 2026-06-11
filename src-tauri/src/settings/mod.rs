//! Settings module for AirType configuration persistence.

mod store;

pub use store::SettingsStore;

use serde::{Deserialize, Serialize};

/// How the hotkey triggers recording
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HotkeyMode {
    /// Hold the key to record, release to stop
    #[default]
    Hold,
    /// Press to start recording, press again to stop
    Toggle,
}

/// Language for transcription
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    English,
    Hebrew,
}

/// Which endpoint type to use for English transcription
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EnglishEndpointType {
    /// Use the same RunPod endpoint as Hebrew (default)
    #[default]
    SharedRunPod,
    /// Use a different, specific RunPod endpoint for English
    CustomRunPod,
    /// Use OpenAI batch API for English
    OpenAI,
}

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Hotkey combination for English recording (e.g., "Ctrl+Shift+E")
    pub hotkey_english: String,
    /// Hotkey combination for Hebrew recording (e.g., "Ctrl+Shift+H")
    pub hotkey_hebrew: String,
    /// How the hotkey triggers recording
    pub hotkey_mode: HotkeyMode,

    /// Default RunPod Endpoint ID for Hebrew (and English by default)
    #[serde(default)]
    pub runpod_endpoint_id: Option<String>,
    /// Default RunPod API key
    #[serde(default)]
    pub runpod_api_key: Option<String>,

    /// English transcription endpoint mode
    #[serde(default)]
    pub english_endpoint_type: EnglishEndpointType,
    
    /// Optional separate RunPod Endpoint ID for English (when english_endpoint_type is CustomRunPod)
    #[serde(default)]
    pub english_custom_runpod_endpoint_id: Option<String>,
    /// Optional separate RunPod API key for English
    #[serde(default)]
    pub english_custom_runpod_api_key: Option<String>,
    
    /// OpenAI API key (required when english_endpoint_type is OpenAI)
    #[serde(default)]
    pub english_openai_api_key: Option<String>,

    /// Whether to start the app minimized to tray
    pub start_minimized: bool,
    /// Whether to start the app on system login
    pub start_on_login: bool,
    /// Delay in milliseconds between injected characters
    pub inject_delay_ms: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey_english: "Ctrl+Shift+E".to_string(),
            hotkey_hebrew: "Ctrl+Shift+H".to_string(),
            hotkey_mode: HotkeyMode::default(),
            
            runpod_endpoint_id: None,
            runpod_api_key: None,
            
            english_endpoint_type: EnglishEndpointType::default(),
            english_custom_runpod_endpoint_id: None,
            english_custom_runpod_api_key: None,
            english_openai_api_key: None,
            
            start_minimized: false,
            start_on_login: false,
            inject_delay_ms: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.hotkey_english, "Ctrl+Shift+E");
        assert_eq!(settings.hotkey_hebrew, "Ctrl+Shift+H");
        assert_eq!(settings.hotkey_mode, HotkeyMode::Hold);
        assert!(!settings.start_minimized);
        assert!(!settings.start_on_login);
        assert_eq!(settings.inject_delay_ms, 10);
        assert_eq!(settings.english_endpoint_type, EnglishEndpointType::SharedRunPod);
    }

    #[test]
    fn test_settings_serialization() {
        let settings = Settings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(settings.hotkey_english, deserialized.hotkey_english);
        assert_eq!(settings.hotkey_hebrew, deserialized.hotkey_hebrew);
    }
}

