//! Settings module for AirType configuration persistence.

mod store;

pub use store::SettingsStore;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    /// Auto-detect language (requires multilingual model)
    Auto,
}

/// Recording and transcription mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RecordingMode {
    /// Record all audio, then transcribe at once
    #[default]
    Batch,
    /// Stream audio and transcribe in chunks (requires OpenAI API)
    Live,
}

/// Transcription engine to use
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptionEngine {
    /// Local Whisper model (free, offline)
    #[default]
    LocalWhisper,
    /// OpenAI API (requires API key, supports live streaming)
    OpenAI,
}

/// Whisper model size
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelSize {
    /// Smallest, fastest, least accurate (~75MB)
    Tiny,
    /// Good balance of speed and accuracy (~150MB)
    #[default]
    Base,
    /// More accurate but slower (~500MB)
    Small,
    /// Large model (~1GB)
    Medium,
    /// Largest model (~3GB)
    Large,
}

impl ModelSize {
    /// Get the filename for this model size
    pub fn filename(&self) -> &'static str {
        match self {
            ModelSize::Tiny => "ggml-tiny.bin",
            ModelSize::Base => "ggml-base.bin",
            ModelSize::Small => "ggml-small.bin",
            ModelSize::Medium => "ggml-medium.bin",
            ModelSize::Large => "ggml-large.bin",
        }
    }

    /// Get the multilingual filename for this model size
    pub fn multilingual_filename(&self) -> &'static str {
        match self {
            ModelSize::Tiny => "ggml-tiny.bin",
            ModelSize::Base => "ggml-base.bin",
            ModelSize::Small => "ggml-small.bin",
            ModelSize::Medium => "ggml-medium.bin",
            ModelSize::Large => "ggml-large.bin",
        }
    }
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
    /// Recording and transcription mode
    pub recording_mode: RecordingMode,
    /// Which transcription engine to use
    #[serde(default)]
    pub transcription_engine: TranscriptionEngine,
    /// OpenAI API key (required for OpenAI engine)
    #[serde(default)]
    pub openai_api_key: Option<String>,
    /// Whether to show transcription live as you speak
    pub live_transcription: bool,
    /// Custom path to Whisper model file (overrides model_size if set)
    pub model_path: Option<PathBuf>,
    /// Whisper model size to use
    pub model_size: ModelSize,
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
            recording_mode: RecordingMode::default(),
            transcription_engine: TranscriptionEngine::default(),
            openai_api_key: None,
            live_transcription: false,
            model_path: None,
            model_size: ModelSize::default(),
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
        assert_eq!(settings.recording_mode, RecordingMode::Batch);
        assert!(!settings.live_transcription);
        assert_eq!(settings.model_size, ModelSize::Base);
        assert!(!settings.start_minimized);
        assert!(!settings.start_on_login);
        assert_eq!(settings.inject_delay_ms, 10);
    }

    #[test]
    fn test_settings_serialization() {
        let settings = Settings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(settings.hotkey_english, deserialized.hotkey_english);
        assert_eq!(settings.hotkey_hebrew, deserialized.hotkey_hebrew);
    }

    #[test]
    fn test_model_size_filename() {
        assert_eq!(ModelSize::Tiny.filename(), "ggml-tiny.bin");
        assert_eq!(ModelSize::Base.filename(), "ggml-base.bin");
        assert_eq!(ModelSize::Small.filename(), "ggml-small.bin");
    }
}
