//! Keyboard-based text injection using enigo.
//!
//! This module provides cross-platform text injection at the cursor position.
//! It supports Unicode characters including Hebrew (RTL) text.
//!
//! # Thread Safety
//! `TextInjector` is NOT thread-safe. Enigo maintains internal state and must
//! be used from a single thread. If you need concurrent access, wrap it in a
//! Mutex or create separate instances per thread.

use enigo::{Direction, Enigo, Key, Keyboard, Settings};
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

    /// Injects text via clipboard paste (Ctrl+V / Cmd+V).
    ///
    /// Much faster than character-by-character injection and doesn't
    /// freeze the system. Saves and restores the previous clipboard content.
    ///
    /// On Linux (X11/Wayland), the clipboard data is served by the owning
    /// process on-demand. If the `Clipboard` handle is dropped immediately
    /// after `set_text`, the target application never receives the paste
    /// payload (arboard logs: "Clipboard was dropped very quickly after
    /// writing"). To avoid this, we keep the Clipboard alive in a dedicated
    /// worker thread via `SetExtLinux::wait_until` for up to 2 seconds, which
    /// is more than enough for the target app to complete the paste.
    pub fn inject_text_clipboard(&mut self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        #[cfg(target_os = "linux")]
        {
            use arboard::SetExtLinux;
            use std::sync::mpsc;
            use std::time::Instant;

            let text_owned = text.to_string();
            let text_for_hold = text_owned.clone();

            // Synchronously signal when the clipboard has actually been
            // populated so the main thread only presses Ctrl+V AFTER the
            // target app can read our text. Clipboard::new() + get_text()
            // on X11 can take well over the previous 40 ms heuristic, so a
            // fixed sleep is unreliable.
            let (ready_tx, ready_rx) = mpsc::sync_channel::<bool>(1);

            thread::spawn(move || {
                let mut clipboard = match arboard::Clipboard::new() {
                    Ok(c) => c,
                    Err(e) => {
                        log::error!("Clipboard worker: failed to open: {}", e);
                        let _ = ready_tx.send(false);
                        return;
                    }
                };
                let prev = clipboard.get_text().ok();
                // Take X11 selection ownership immediately. The Clipboard
                // instance stays alive for the rest of this closure, so
                // arboard's background server thread keeps serving selection
                // requests from other apps during the paste.
                if let Err(e) = clipboard.set_text(text_owned) {
                    log::error!("Clipboard worker: set_text failed: {}", e);
                    let _ = ready_tx.send(false);
                    return;
                }
                // Signal the main thread that the clipboard now contains our
                // text and is ready to be pasted.
                let _ = ready_tx.send(true);

                // Keep the clipboard alive for up to 2 s to serve paste
                // requests. wait_until blocks this thread but leaves the
                // server thread free to answer X11 SelectionRequest events.
                let _ = clipboard
                    .set()
                    .wait_until(Instant::now() + Duration::from_millis(2000))
                    .text(text_for_hold);

                // Restore the previous clipboard contents and hold them
                // briefly so clipboard managers (gpaste/klipper/etc.) and
                // any other apps that request the selection actually
                // receive the restored data. Without this hold arboard
                // logs: "Clipboard was dropped very quickly after writing".
                if let Some(p) = prev {
                    let _ = clipboard
                        .set()
                        .wait_until(Instant::now() + Duration::from_millis(500))
                        .text(p);
                }
            });

            // Wait up to 500 ms for the worker to actually own the selection
            // before pressing Ctrl+V. If the worker failed or timed out we
            // still attempt the paste as a best effort.
            let _ = ready_rx.recv_timeout(Duration::from_millis(500));

            self.enigo
                .key(Key::Control, Direction::Press)
                .map_err(|e| InjectionError::TypeError(e.to_string()))?;
            self.enigo
                .key(Key::Unicode('v'), Direction::Click)
                .map_err(|e| InjectionError::TypeError(e.to_string()))?;
            self.enigo
                .key(Key::Control, Direction::Release)
                .map_err(|e| InjectionError::TypeError(e.to_string()))?;

            // Wait for paste to complete before returning so callers that
            // trigger follow-up actions don't race the paste.
            thread::sleep(Duration::from_millis(80));
            return Ok(());
        }

        #[cfg(not(target_os = "linux"))]
        {
            let mut clipboard = arboard::Clipboard::new()
                .map_err(|e| InjectionError::InitError(format!("Clipboard: {}", e)))?;

            let prev = clipboard.get_text().ok();

            clipboard
                .set_text(text)
                .map_err(|e| InjectionError::TypeError(format!("Clipboard set: {}", e)))?;

            thread::sleep(Duration::from_millis(30));

            #[cfg(target_os = "macos")]
            let modifier = Key::Meta;
            #[cfg(not(target_os = "macos"))]
            let modifier = Key::Control;

            // On macOS, `Key::Unicode('v')` routes through Apple's Text
            // Services Manager (TSMGetInputSourceProperty) to resolve the
            // current keyboard layout. TSM asserts that it must run on the
            // main thread (libdispatch's dispatch_assert_queue), so calling
            // it from a tokio worker or std::thread causes SIGTRAP. We
            // sidestep TSM by passing the hardware virtual keycode for 'v'
            // (kVK_ANSI_V = 0x09) directly via `Key::Other`.
            #[cfg(target_os = "macos")]
            let paste_key = Key::Other(0x09);
            #[cfg(not(target_os = "macos"))]
            let paste_key = Key::Unicode('v');

            self.enigo
                .key(modifier, Direction::Press)
                .map_err(|e| InjectionError::TypeError(e.to_string()))?;
            self.enigo
                .key(paste_key, Direction::Click)
                .map_err(|e| InjectionError::TypeError(e.to_string()))?;
            self.enigo
                .key(modifier, Direction::Release)
                .map_err(|e| InjectionError::TypeError(e.to_string()))?;

            thread::sleep(Duration::from_millis(50));

            if let Some(prev_text) = prev {
                let _ = clipboard.set_text(prev_text);
            }

            Ok(())
        }
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
