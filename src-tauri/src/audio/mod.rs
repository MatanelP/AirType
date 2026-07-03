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

/// Root-mean-square amplitude of the samples (0.0 = digital silence, ~0.1+ for
/// normal speech). Used to distinguish a genuinely-spoken short phrase from a
/// model hallucination on near-silent audio.
pub fn rms_energy(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

#[cfg(test)]
mod tests {
    use super::rms_energy;

    #[test]
    fn silence_is_low_energy() {
        assert_eq!(rms_energy(&[]), 0.0);
        assert!(rms_energy(&[0.0; 1000]) < 0.001);
        assert!(rms_energy(&[0.0005; 1000]) < 0.02); // faint room tone
    }

    #[test]
    fn speech_is_high_energy() {
        // A ~0.2 amplitude tone stands well above the silence threshold.
        let tone: Vec<f32> = (0..1000).map(|i| 0.2 * (i as f32 * 0.3).sin()).collect();
        assert!(rms_energy(&tone) > 0.05);
    }
}
