//! Audio capture module for AirType.
//!
//! Provides cross-platform audio capture functionality using cpal,
//! with automatic resampling to 16kHz mono format required by Whisper.
//!
//! # Example
//!
//! ```no_run
//! use airtype_lib::audio::{AudioCapture, AudioError};
//!
//! fn main() -> Result<(), AudioError> {
//!     let capture = AudioCapture::new()?;
//!     
//!     // Start recording
//!     capture.start_recording()?;
//!     
//!     // ... wait for user to stop ...
//!     
//!     // Stop and get samples
//!     let samples = capture.stop_recording()?;
//!     println!("Captured {} samples", samples.len());
//!     
//!     Ok(())
//! }
//! ```

mod buffer;
mod capture;

pub use buffer::{AudioBuffer, DEFAULT_CHUNK_SIZE, TARGET_SAMPLE_RATE};
pub use capture::{AudioCapture, AudioError, CaptureConfig, CaptureState};
