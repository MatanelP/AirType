use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[derive(Error, Debug)]
pub enum TranscriptionError {
    #[error("Failed to load Whisper model from {path}: {source}")]
    ModelLoadError {
        path: PathBuf,
        #[source]
        source: whisper_rs::WhisperError,
    },

    #[error("Model not loaded")]
    ModelNotLoaded,

    #[error("Failed to create Whisper state: {0}")]
    StateCreationError(#[from] whisper_rs::WhisperError),

    #[error("Transcription failed: {0}")]
    TranscriptionFailed(String),

    #[error("Invalid language code: {0}")]
    InvalidLanguage(String),

    #[error("No audio segments produced")]
    NoSegments,
}

pub type Result<T> = std::result::Result<T, TranscriptionError>;

/// Supported languages for transcription
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Language {
    English,
    Hebrew,
    Auto,
}

impl Language {
    pub fn as_whisper_code(&self) -> Option<&'static str> {
        match self {
            Language::English => Some("en"),
            Language::Hebrew => Some("he"),
            Language::Auto => None, // None triggers auto-detection
        }
    }

    pub fn from_code(code: &str) -> std::result::Result<Self, TranscriptionError> {
        match code.to_lowercase().as_str() {
            "en" | "english" => Ok(Language::English),
            "he" | "hebrew" => Ok(Language::Hebrew),
            "auto" => Ok(Language::Auto),
            _ => Err(TranscriptionError::InvalidLanguage(code.to_string())),
        }
    }
}

/// Internal state holding the loaded Whisper context
struct WhisperState {
    context: WhisperContext,
}

/// Thread-safe Whisper transcriber with lazy loading support
pub struct WhisperTranscriber {
    model_path: PathBuf,
    state: RwLock<Option<WhisperState>>,
    language: RwLock<Language>,
}

impl WhisperTranscriber {
    /// Create a new transcriber with the specified model path.
    /// The model is not loaded until first use (lazy loading).
    pub fn new(model_path: &Path) -> Self {
        Self {
            model_path: model_path.to_path_buf(),
            state: RwLock::new(None),
            language: RwLock::new(Language::Auto),
        }
    }

    /// Check if the model is currently loaded
    pub fn is_loaded(&self) -> bool {
        self.state.read().is_some()
    }

    /// Set the transcription language
    pub fn set_language(&self, lang: &str) -> Result<()> {
        let language = Language::from_code(lang)?;
        *self.language.write() = language;
        Ok(())
    }

    /// Get the current language setting
    pub fn get_language(&self) -> Language {
        self.language.read().clone()
    }

    /// Ensure the model is loaded, loading it if necessary
    fn ensure_loaded(&self) -> Result<()> {
        // Fast path: check with read lock
        if self.state.read().is_some() {
            return Ok(());
        }

        // Slow path: acquire write lock and load
        let mut state = self.state.write();

        // Double-check after acquiring write lock
        if state.is_some() {
            return Ok(());
        }

        log::info!("Loading Whisper model from: {:?}", self.model_path);

        let context = WhisperContext::new_with_params(
            self.model_path.to_str().unwrap_or(""),
            WhisperContextParameters::default(),
        )
        .map_err(|e| TranscriptionError::ModelLoadError {
            path: self.model_path.clone(),
            source: e,
        })?;

        *state = Some(WhisperState { context });
        log::info!("Whisper model loaded successfully");

        Ok(())
    }

    /// Unload the model to free memory
    pub fn unload(&self) {
        let mut state = self.state.write();
        if state.is_some() {
            log::info!("Unloading Whisper model");
            *state = None;
        }
    }

    /// Create transcription parameters with current settings
    fn create_params(&self) -> FullParams<'static, 'static> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        let lang = self.language.read();
        params.set_language(lang.as_whisper_code());

        // Enable translate to English if needed (disabled - we want original language)
        params.set_translate(false);

        // Single segment mode for cleaner output
        params.set_single_segment(false);

        // Show progress so user knows model is working
        params.set_print_progress(true);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Use all available CPU cores for faster inference
        let n_threads = std::thread::available_parallelism()
            .map(|n| n.get() as i32)
            .unwrap_or(4);
        params.set_n_threads(n_threads);

        // Token timestamps for streaming
        params.set_token_timestamps(true);

        params
    }

    /// Transcribe audio data (batch mode - waits for complete audio)
    ///
    /// # Arguments
    /// * `audio` - Audio samples as f32 at 16kHz mono
    ///
    /// # Returns
    /// The transcribed text
    pub fn transcribe(&self, audio: &[f32]) -> Result<String> {
        self.ensure_loaded()?;

        let state_guard = self.state.read();
        let whisper_state = state_guard
            .as_ref()
            .ok_or(TranscriptionError::ModelNotLoaded)?;

        let mut wh_state = whisper_state.context.create_state()?;
        let params = self.create_params();

        log::info!("Starting transcription of {} samples ({:.1}s audio)...", audio.len(), audio.len() as f64 / 16000.0);
        wh_state
            .full(params, audio)
            .map_err(|e| TranscriptionError::TranscriptionFailed(e.to_string()))?;
        log::info!("Transcription inference complete");

        // Collect all segments
        let num_segments = wh_state.full_n_segments();
        if num_segments == 0 {
            return Ok(String::new());
        }

        let mut result = String::new();
        for i in 0..num_segments {
            if let Some(segment) = wh_state.get_segment(i) {
                if let Ok(text) = segment.to_str_lossy() {
                    result.push_str(&text);
                }
            }
        }

        Ok(result.trim().to_string())
    }

    /// Transcribe audio for live/streaming mode
    /// Returns partial results suitable for real-time display
    ///
    /// # Arguments
    /// * `audio` - Audio samples as f32 at 16kHz mono (accumulated buffer)
    ///
    /// # Returns
    /// The transcribed text so far
    pub fn transcribe_streaming(&self, audio: &[f32]) -> Result<String> {
        self.ensure_loaded()?;

        let state_guard = self.state.read();
        let whisper_state = state_guard
            .as_ref()
            .ok_or(TranscriptionError::ModelNotLoaded)?;

        let mut wh_state = whisper_state.context.create_state()?;
        let mut params = self.create_params();

        // Streaming-specific settings
        params.set_single_segment(true);
        // Suppress blank/silence for faster partial results
        params.set_suppress_blank(true);

        wh_state
            .full(params, audio)
            .map_err(|e| TranscriptionError::TranscriptionFailed(e.to_string()))?;

        let num_segments = wh_state.full_n_segments();
        if num_segments == 0 {
            return Ok(String::new());
        }

        // For streaming, return the latest segment
        let mut result = String::new();
        for i in 0..num_segments {
            if let Some(segment) = wh_state.get_segment(i) {
                if let Ok(text) = segment.to_str_lossy() {
                    result.push_str(&text);
                }
            }
        }

        Ok(result.trim().to_string())
    }

    /// Pre-load the model (useful to avoid latency on first transcription)
    pub fn preload(&self) -> Result<()> {
        self.ensure_loaded()
    }
}

// Ensure WhisperTranscriber can be shared across threads
unsafe impl Send for WhisperTranscriber {}
unsafe impl Sync for WhisperTranscriber {}

/// Thread-safe handle to WhisperTranscriber
pub type SharedTranscriber = Arc<WhisperTranscriber>;

/// Create a new shared transcriber instance
pub fn create_shared_transcriber(model_path: &Path) -> SharedTranscriber {
    Arc::new(WhisperTranscriber::new(model_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_code() {
        assert_eq!(Language::from_code("en").unwrap(), Language::English);
        assert_eq!(Language::from_code("EN").unwrap(), Language::English);
        assert_eq!(Language::from_code("english").unwrap(), Language::English);
        assert_eq!(Language::from_code("he").unwrap(), Language::Hebrew);
        assert_eq!(Language::from_code("hebrew").unwrap(), Language::Hebrew);
        assert_eq!(Language::from_code("auto").unwrap(), Language::Auto);
        assert!(Language::from_code("invalid").is_err());
    }

    #[test]
    fn test_language_whisper_code() {
        assert_eq!(Language::English.as_whisper_code(), Some("en"));
        assert_eq!(Language::Hebrew.as_whisper_code(), Some("he"));
        assert_eq!(Language::Auto.as_whisper_code(), None);
    }

    #[test]
    fn test_transcriber_not_loaded_initially() {
        let transcriber = WhisperTranscriber::new(Path::new("/nonexistent/model.bin"));
        assert!(!transcriber.is_loaded());
    }

    #[test]
    fn test_set_language() {
        let transcriber = WhisperTranscriber::new(Path::new("/nonexistent/model.bin"));
        assert!(transcriber.set_language("en").is_ok());
        assert_eq!(transcriber.get_language(), Language::English);

        assert!(transcriber.set_language("he").is_ok());
        assert_eq!(transcriber.get_language(), Language::Hebrew);

        assert!(transcriber.set_language("invalid").is_err());
    }
}
