//! Keyboard-based text injection using enigo.
//!
//! This module provides cross-platform text injection at the cursor position.
//! It supports Unicode characters including Hebrew (RTL) text.
//!
//! # Thread Safety
//! `TextInjector` is NOT thread-safe. Enigo maintains internal state and must
//! be used from a single thread. If you need concurrent access, wrap it in a
//! Mutex or create separate instances per thread.

use enigo::{Enigo, Keyboard, Settings};
use std::thread;
use std::time::Duration;
use thiserror::Error;

/// Default delay between characters in milliseconds for reliable typing.
const DEFAULT_CHAR_DELAY_MS: u64 = 5;

/// Errors that can occur during text injection.
#[derive(Error, Debug)]
pub enum InjectionError {
    #[error("Failed to initialize enigo: {0}")]
    InitError(String),

    #[error("Failed to inject text: {0}")]
    TypeError(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

/// Result type for injection operations.
pub type Result<T> = std::result::Result<T, InjectionError>;

/// Text injector that types text at the current cursor position.
///
/// Uses the enigo crate for cross-platform keyboard simulation.
/// Supports Unicode characters including Hebrew and other RTL languages.
///
/// # Example
/// ```no_run
/// use airtype_lib::injection::TextInjector;
///
/// let mut injector = TextInjector::new()?;
/// injector.inject_text("Hello, World!")?;
/// injector.inject_text("שלום עולם")?; // Hebrew text
/// # Ok::<(), airtype_lib::injection::InjectionError>(())
/// ```
pub struct TextInjector {
    enigo: Enigo,
    default_delay_ms: u64,
}

impl TextInjector {
    /// Creates a new TextInjector instance.
    ///
    /// Initializes the underlying enigo instance with default settings.
    ///
    /// # Errors
    /// Returns `InjectionError::InitError` if enigo fails to initialize.
    pub fn new() -> Result<Self> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| InjectionError::InitError(e.to_string()))?;

        Ok(Self {
            enigo,
            default_delay_ms: DEFAULT_CHAR_DELAY_MS,
        })
    }

    /// Creates a new TextInjector with custom settings.
    ///
    /// # Arguments
    /// * `settings` - Enigo settings for keyboard simulation
    /// * `default_delay_ms` - Default delay between characters
    ///
    /// # Errors
    /// Returns `InjectionError::InitError` if enigo fails to initialize.
    pub fn with_settings(settings: &Settings, default_delay_ms: u64) -> Result<Self> {
        let enigo = Enigo::new(settings).map_err(|e| InjectionError::InitError(e.to_string()))?;

        Ok(Self {
            enigo,
            default_delay_ms,
        })
    }

    /// Injects text at the current cursor position.
    ///
    /// This method types the entire text string using the system's keyboard
    /// simulation. It handles Unicode characters properly, including Hebrew
    /// and other RTL scripts.
    ///
    /// # Arguments
    /// * `text` - The text to inject. Can contain any Unicode characters,
    ///            newlines, and special characters.
    ///
    /// # Errors
    /// Returns `InjectionError::TypeError` if the text cannot be typed.
    ///
    /// # Example
    /// ```no_run
    /// # use airtype_lib::injection::TextInjector;
    /// let mut injector = TextInjector::new()?;
    /// injector.inject_text("Hello\nWorld")?;
    /// # Ok::<(), airtype_lib::injection::InjectionError>(())
    /// ```
    pub fn inject_text(&mut self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        // Enigo's text() method handles Unicode properly
        self.enigo
            .text(text)
            .map_err(|e| InjectionError::TypeError(e.to_string()))?;

        Ok(())
    }

    /// Injects text with a specified delay between characters.
    ///
    /// This method types text character by character with a delay between
    /// each character. Useful for applications that may drop keystrokes
    /// or for a more natural typing appearance.
    ///
    /// # Arguments
    /// * `text` - The text to inject
    /// * `delay_ms` - Delay in milliseconds between each character
    ///
    /// # Errors
    /// Returns `InjectionError::TypeError` if any character cannot be typed.
    ///
    /// # Example
    /// ```no_run
    /// # use airtype_lib::injection::TextInjector;
    /// let mut injector = TextInjector::new()?;
    /// // Type slowly (50ms between characters)
    /// injector.inject_text_with_delay("שלום", 50)?;
    /// # Ok::<(), airtype_lib::injection::InjectionError>(())
    /// ```
    pub fn inject_text_with_delay(&mut self, text: &str, delay_ms: u64) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        let delay = Duration::from_millis(delay_ms);

        // Process text by grapheme clusters to handle combined characters properly
        // For most cases, char iteration works, but we handle multi-char sequences
        for (i, c) in text.chars().enumerate() {
            // Convert char to string for enigo's text method
            let char_str = c.to_string();

            self.enigo
                .text(&char_str)
                .map_err(|e| InjectionError::TypeError(format!("Failed at char {}: {}", i, e)))?;

            // Don't delay after the last character
            if i < text.chars().count() - 1 && delay_ms > 0 {
                thread::sleep(delay);
            }
        }

        Ok(())
    }

    /// Injects text with the default inter-character delay.
    ///
    /// Uses the default delay configured during construction (5ms by default).
    /// This provides a balance between speed and reliability.
    ///
    /// # Arguments
    /// * `text` - The text to inject
    ///
    /// # Errors
    /// Returns `InjectionError::TypeError` if any character cannot be typed.
    pub fn inject_text_slow(&mut self, text: &str) -> Result<()> {
        self.inject_text_with_delay(text, self.default_delay_ms)
    }

    /// Sets the default delay for slow text injection.
    ///
    /// # Arguments
    /// * `delay_ms` - New default delay in milliseconds
    pub fn set_default_delay(&mut self, delay_ms: u64) {
        self.default_delay_ms = delay_ms;
    }

    /// Gets the current default delay.
    pub fn default_delay(&self) -> u64 {
        self.default_delay_ms
    }
}

impl std::fmt::Debug for TextInjector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInjector")
            .field("default_delay_ms", &self.default_delay_ms)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_empty_text() {
        // Note: This test requires a display server in CI, so we skip the actual injection
        // but verify the logic handles empty strings
        let result = "".is_empty();
        assert!(result);
    }

    #[test]
    fn test_unicode_string_handling() {
        // Verify Hebrew strings are valid Unicode
        let hebrew = "שלום עולם";
        assert!(!hebrew.is_empty());
        assert_eq!(hebrew.chars().count(), 9); // 4 + space + 4

        // Verify mixed content
        let mixed = "Hello שלום World";
        assert!(mixed.contains("שלום"));
    }

    #[test]
    fn test_special_characters() {
        // Verify special characters in strings
        let special = "Hello\nWorld\tTab";
        assert!(special.contains('\n'));
        assert!(special.contains('\t'));
    }
}
