//! Text injection module for AirType.
//!
//! This module provides cross-platform text injection at the cursor position
//! using keyboard simulation. It supports Unicode text including Hebrew and
//! other RTL languages.
//!
//! # Architecture
//! The module is built on the `enigo` crate for cross-platform keyboard
//! simulation. It provides a high-level `TextInjector` API that handles:
//! - Unicode character support (including Hebrew)
//! - Configurable typing delays for reliability
//! - Proper error handling
//!
//! # Thread Safety
//! `TextInjector` is NOT thread-safe due to enigo's internal state.
//! For concurrent access, wrap in `parking_lot::Mutex` or use separate instances.
//!
//! # Example
//! ```no_run
//! use airtype_lib::injection::{TextInjector, InjectionError};
//!
//! fn inject_transcription(text: &str) -> Result<(), InjectionError> {
//!     let mut injector = TextInjector::new()?;
//!     
//!     // Fast injection for short text
//!     if text.len() < 100 {
//!         injector.inject_text(text)?;
//!     } else {
//!         // Slower injection for longer text (more reliable)
//!         injector.inject_text_with_delay(text, 10)?;
//!     }
//!     
//!     Ok(())
//! }
//! ```

mod keyboard;

pub use keyboard::{InjectionError, Result, TextInjector};

/// Convenience function to inject text without managing a TextInjector instance.
///
/// Creates a temporary TextInjector, injects the text, and drops it.
/// For multiple injections, prefer creating and reusing a TextInjector.
///
/// # Arguments
/// * `text` - The text to inject at the cursor position
///
/// # Errors
/// Returns `InjectionError` if initialization or injection fails.
///
/// # Example
/// ```no_run
/// use airtype_lib::injection::inject_text;
///
/// inject_text("Quick transcription")?;
/// # Ok::<(), airtype_lib::injection::InjectionError>(())
/// ```
pub fn inject_text(text: &str) -> Result<()> {
    let mut injector = TextInjector::new()?;
    injector.inject_text(text)
}

/// Convenience function to inject text with a delay between characters.
///
/// Creates a temporary TextInjector, injects the text with delay, and drops it.
///
/// # Arguments
/// * `text` - The text to inject at the cursor position
/// * `delay_ms` - Delay in milliseconds between each character
///
/// # Errors
/// Returns `InjectionError` if initialization or injection fails.
pub fn inject_text_with_delay(text: &str, delay_ms: u64) -> Result<()> {
    let mut injector = TextInjector::new()?;
    injector.inject_text_with_delay(text, delay_ms)
}
