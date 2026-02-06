//! Settings persistence layer.

use super::Settings;
use anyhow::{Context, Result};
use parking_lot::RwLock;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

const APP_NAME: &str = "airtype";
const SETTINGS_FILENAME: &str = "settings.json";
const MODELS_DIR_NAME: &str = "models";

/// Thread-safe settings store with automatic persistence.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    settings: Arc<RwLock<Settings>>,
    config_dir: PathBuf,
}

impl SettingsStore {
    /// Create a new settings store, loading existing settings or creating defaults.
    pub fn new() -> Result<Self> {
        let config_dir = Self::get_config_dir();

        // Ensure config directory exists
        fs::create_dir_all(&config_dir)
            .with_context(|| format!("Failed to create config directory: {:?}", config_dir))?;

        // Ensure models directory exists
        let models_dir = Self::get_models_dir();
        fs::create_dir_all(&models_dir)
            .with_context(|| format!("Failed to create models directory: {:?}", models_dir))?;

        // Load settings or use defaults
        let settings = Self::load_from_path(&config_dir).unwrap_or_else(|e| {
            log::warn!("Failed to load settings, using defaults: {}", e);
            Settings::default()
        });

        let store = Self {
            settings: Arc::new(RwLock::new(settings)),
            config_dir,
        };

        // Save to ensure file exists with current defaults
        if let Err(e) = store.save_internal() {
            log::warn!("Failed to save initial settings: {}", e);
        }

        Ok(store)
    }

    /// Get the platform-specific config directory.
    /// - Linux: ~/.config/airtype/
    /// - macOS: ~/Library/Application Support/airtype/
    /// - Windows: %APPDATA%/airtype/
    pub fn get_config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_NAME)
    }

    /// Get the models directory within the config directory.
    pub fn get_models_dir() -> PathBuf {
        Self::get_config_dir().join(MODELS_DIR_NAME)
    }

    /// Get the path to the settings file.
    pub fn get_settings_path() -> PathBuf {
        Self::get_config_dir().join(SETTINGS_FILENAME)
    }

    /// Load settings from disk.
    pub fn load(&self) -> Result<Settings> {
        Self::load_from_path(&self.config_dir)
    }

    /// Load settings from a specific config directory.
    fn load_from_path(config_dir: &PathBuf) -> Result<Settings> {
        let settings_path = config_dir.join(SETTINGS_FILENAME);

        let contents = fs::read_to_string(&settings_path)
            .with_context(|| format!("Failed to read settings file: {:?}", settings_path))?;

        let settings: Settings =
            serde_json::from_str(&contents).with_context(|| "Failed to parse settings JSON")?;

        Ok(settings)
    }

    /// Save the provided settings to disk.
    pub fn save(&self, settings: &Settings) -> Result<()> {
        let settings_path = self.config_dir.join(SETTINGS_FILENAME);

        let json = serde_json::to_string_pretty(settings)
            .with_context(|| "Failed to serialize settings")?;

        fs::write(&settings_path, json)
            .with_context(|| format!("Failed to write settings file: {:?}", settings_path))?;

        Ok(())
    }

    /// Save current settings to disk.
    fn save_internal(&self) -> Result<()> {
        let settings = self.settings.read();
        self.save(&settings)
    }

    /// Get a clone of the current settings.
    pub fn get(&self) -> Settings {
        self.settings.read().clone()
    }

    /// Update settings and persist to disk.
    pub fn update(&self, settings: Settings) -> Result<()> {
        // Save to disk first
        self.save(&settings)?;

        // Update in-memory settings
        *self.settings.write() = settings;

        Ok(())
    }

    /// Update settings using a closure, then persist.
    pub fn update_with<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&mut Settings),
    {
        let mut settings = self.get();
        f(&mut settings);
        self.update(settings)
    }

    /// Reset settings to defaults.
    pub fn reset(&self) -> Result<()> {
        self.update(Settings::default())
    }

    /// Get the effective model path (custom path or default based on model size).
    pub fn get_effective_model_path(&self) -> PathBuf {
        let settings = self.settings.read();

        if let Some(ref custom_path) = settings.model_path {
            custom_path.clone()
        } else {
            Self::get_models_dir().join(settings.model_size.filename())
        }
    }
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new().expect("Failed to create settings store")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{HotkeyMode, ModelSize};
    use tempfile::TempDir;

    fn with_temp_config<F>(f: F)
    where
        F: FnOnce(PathBuf),
    {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(APP_NAME);
        fs::create_dir_all(&config_dir).unwrap();
        f(config_dir);
    }

    #[test]
    fn test_config_dir_exists() {
        let config_dir = SettingsStore::get_config_dir();
        assert!(config_dir.ends_with(APP_NAME));
    }

    #[test]
    fn test_models_dir() {
        let models_dir = SettingsStore::get_models_dir();
        assert!(models_dir.ends_with(MODELS_DIR_NAME));
    }

    #[test]
    fn test_settings_roundtrip() {
        with_temp_config(|config_dir| {
            let settings_path = config_dir.join(SETTINGS_FILENAME);

            let settings = Settings {
                hotkey_english: "Alt+E".to_string(),
                hotkey_hebrew: "Alt+H".to_string(),
                hotkey_mode: HotkeyMode::Toggle,
                recording_mode: super::super::RecordingMode::Live,
                live_transcription: true,
                model_path: Some(PathBuf::from("/custom/model.bin")),
                model_size: ModelSize::Small,
                start_minimized: true,
                start_on_login: true,
                inject_delay_ms: 50,
            };

            // Write settings
            let json = serde_json::to_string_pretty(&settings).unwrap();
            fs::write(&settings_path, json).unwrap();

            // Read settings back
            let contents = fs::read_to_string(&settings_path).unwrap();
            let loaded: Settings = serde_json::from_str(&contents).unwrap();

            assert_eq!(loaded.hotkey_english, "Alt+E");
            assert_eq!(loaded.hotkey_hebrew, "Alt+H");
            assert_eq!(loaded.hotkey_mode, HotkeyMode::Toggle);
            assert!(loaded.live_transcription);
            assert_eq!(loaded.model_size, ModelSize::Small);
            assert!(loaded.start_minimized);
            assert!(loaded.start_on_login);
            assert_eq!(loaded.inject_delay_ms, 50);
        });
    }

    #[test]
    fn test_default_settings_serialization() {
        let settings = Settings::default();
        let json = serde_json::to_string_pretty(&settings).unwrap();

        // Verify JSON contains expected fields
        assert!(json.contains("hotkey_english"));
        assert!(json.contains("hotkey_hebrew"));
        assert!(json.contains("hotkey_mode"));
        assert!(json.contains("recording_mode"));
        assert!(json.contains("live_transcription"));
        assert!(json.contains("model_size"));
        assert!(json.contains("start_minimized"));
        assert!(json.contains("start_on_login"));
        assert!(json.contains("inject_delay_ms"));
    }
}
