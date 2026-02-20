//! Audio capture module using cpal for cross-platform microphone input.
//!
//! Captures audio at 16kHz mono format required by Whisper, with automatic
//! resampling from the device's native sample rate if needed.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleRate, Stream, StreamConfig};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;

use super::buffer::{AudioBuffer, TARGET_SAMPLE_RATE};

/// Errors that can occur during audio capture
#[derive(Error, Debug)]
pub enum AudioError {
    #[error("No audio input device found")]
    NoInputDevice,

    #[error("Failed to get default input config: {0}")]
    ConfigError(String),

    #[error("Failed to build audio stream: {0}")]
    StreamBuildError(String),

    #[error("Failed to start audio stream: {0}")]
    StreamStartError(String),

    #[error("Recording is not active")]
    NotRecording,

    #[error("Recording is already active")]
    AlreadyRecording,
}

/// Audio capture state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureState {
    Idle,
    Recording,
}

/// Audio capture configuration
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Chunk size for streaming mode (samples)
    pub chunk_size: usize,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            chunk_size: super::buffer::DEFAULT_CHUNK_SIZE,
        }
    }
}

/// Main audio capture struct - manages recording from microphone
pub struct AudioCapture {
    device: Device,
    buffer: AudioBuffer,
    stream: Mutex<Option<Stream>>,
    is_recording: Arc<AtomicBool>,
    device_sample_rate: u32,
    device_channels: u16,
    /// Optional sender for streaming audio chunks to an external consumer (e.g. OpenAI)
    stream_tx: Arc<Mutex<Option<mpsc::Sender<Vec<f32>>>>>,
}

// Stream is not Send/Sync but we manage it safely with mutex
unsafe impl Send for AudioCapture {}
unsafe impl Sync for AudioCapture {}

impl AudioCapture {
    /// Create a new AudioCapture using the default input device
    pub fn new() -> Result<Self, AudioError> {
        Self::with_config(CaptureConfig::default())
    }

    /// Create a new AudioCapture with custom configuration
    pub fn with_config(config: CaptureConfig) -> Result<Self, AudioError> {
        let host = cpal::default_host();

        let device = host
            .default_input_device()
            .ok_or(AudioError::NoInputDevice)?;

        let supported_config = device
            .default_input_config()
            .map_err(|e| AudioError::ConfigError(e.to_string()))?;

        let device_sample_rate = supported_config.sample_rate().0;
        let device_channels = supported_config.channels();

        log::info!(
            "Audio device: {} ({}Hz, {} channels)",
            device.name().unwrap_or_else(|_| "Unknown".to_string()),
            device_sample_rate,
            device_channels
        );

        Ok(Self {
            device,
            buffer: AudioBuffer::with_chunk_size(config.chunk_size),
            stream: Mutex::new(None),
            is_recording: Arc::new(AtomicBool::new(false)),
            device_sample_rate,
            device_channels,
            stream_tx: Arc::new(Mutex::new(None)),
        })
    }

    /// Start recording audio from the microphone
    pub fn start_recording(&self) -> Result<(), AudioError> {
        if self.is_recording.load(Ordering::SeqCst) {
            return Err(AudioError::AlreadyRecording);
        }

        // Clear any previous samples
        self.buffer.clear();

        let config = StreamConfig {
            channels: self.device_channels,
            sample_rate: SampleRate(self.device_sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let buffer = self.buffer.clone();
        let is_recording = self.is_recording.clone();
        let source_rate = self.device_sample_rate;
        let channels = self.device_channels as usize;
        let stream_tx = self.stream_tx.clone();

        // Create resampler state for linear interpolation
        let resample_ratio = TARGET_SAMPLE_RATE as f64 / source_rate as f64;
        let resample_state = Arc::new(Mutex::new(ResampleState::new()));

        let stream = self
            .device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !is_recording.load(Ordering::SeqCst) {
                        return;
                    }

                    // Convert to mono if needed
                    let mono_samples: Vec<f32> = if channels > 1 {
                        data.chunks(channels)
                            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                            .collect()
                    } else {
                        data.to_vec()
                    };

                    // Resample to 16kHz if needed
                    let resampled = if source_rate != TARGET_SAMPLE_RATE {
                        let mut state = resample_state.lock();
                        resample_linear(&mono_samples, resample_ratio, &mut state)
                    } else {
                        mono_samples
                    };

                    buffer.push_samples(&resampled);

                    // Stream to external consumer if set
                    if let Some(tx) = stream_tx.lock().as_ref() {
                        let _ = tx.try_send(resampled);
                    }
                },
                move |err| {
                    log::error!("Audio stream error: {}", err);
                },
                None,
            )
            .map_err(|e| AudioError::StreamBuildError(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioError::StreamStartError(e.to_string()))?;

        self.is_recording.store(true, Ordering::SeqCst);
        *self.stream.lock() = Some(stream);

        log::info!("Recording started");
        Ok(())
    }

    /// Stop recording and return all captured samples
    pub fn stop_recording(&self) -> Result<Vec<f32>, AudioError> {
        if !self.is_recording.load(Ordering::SeqCst) {
            return Err(AudioError::NotRecording);
        }

        self.is_recording.store(false, Ordering::SeqCst);

        // Drop the stream to stop recording
        *self.stream.lock() = None;

        let samples = self.buffer.take_samples();
        log::info!(
            "Recording stopped, captured {} samples ({:.2}s)",
            samples.len(),
            samples.len() as f32 / TARGET_SAMPLE_RATE as f32
        );

        Ok(samples)
    }

    /// Set an external streaming sender for live audio forwarding.
    /// Audio chunks (16kHz mono f32) will be sent through this channel during recording.
    pub fn set_stream_sender(&self, tx: mpsc::Sender<Vec<f32>>) {
        *self.stream_tx.lock() = Some(tx);
    }

    /// Get current samples without stopping recording (for live preview)
    pub fn get_samples(&self) -> Vec<f32> {
        self.buffer.get_samples()
    }

    /// Take a chunk of samples for streaming transcription
    /// Returns None if not enough samples are buffered
    pub fn take_chunk(&self) -> Option<Vec<f32>> {
        self.buffer.take_chunk()
    }

    /// Clear the sample buffer
    pub fn clear_buffer(&self) {
        self.buffer.clear();
    }

    /// Check if currently recording
    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::SeqCst)
    }

    /// Get the current capture state
    pub fn state(&self) -> CaptureState {
        if self.is_recording() {
            CaptureState::Recording
        } else {
            CaptureState::Idle
        }
    }

    /// Get the number of buffered samples
    pub fn buffered_samples(&self) -> usize {
        self.buffer.len()
    }

    /// Get the duration of buffered audio in seconds
    pub fn buffered_duration(&self) -> f32 {
        self.buffer.duration_secs()
    }

    /// Check if a chunk is available for streaming
    pub fn has_chunk(&self) -> bool {
        self.buffer.has_chunk()
    }

    /// Get the device name
    pub fn device_name(&self) -> String {
        self.device.name().unwrap_or_else(|_| "Unknown".to_string())
    }

    /// Get the native sample rate of the audio device
    pub fn device_sample_rate(&self) -> u32 {
        self.device_sample_rate
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        if self.is_recording.load(Ordering::SeqCst) {
            self.is_recording.store(false, Ordering::SeqCst);
            *self.stream.lock() = None;
        }
    }
}

/// State for linear interpolation resampling
struct ResampleState {
    /// Fractional position in source samples
    position: f64,
    /// Last sample for interpolation across buffer boundaries
    last_sample: f32,
}

impl ResampleState {
    fn new() -> Self {
        Self {
            position: 0.0,
            last_sample: 0.0,
        }
    }
}

/// Simple linear interpolation resampler
/// This is adequate for speech and avoids external dependencies
fn resample_linear(input: &[f32], ratio: f64, state: &mut ResampleState) -> Vec<f32> {
    if input.is_empty() {
        return Vec::new();
    }

    let output_len = ((input.len() as f64) * ratio).ceil() as usize;
    let mut output = Vec::with_capacity(output_len);

    // Include the last sample from previous buffer for interpolation
    let extended_input: Vec<f32> = std::iter::once(state.last_sample)
        .chain(input.iter().copied())
        .collect();

    while state.position < input.len() as f64 {
        let idx = state.position as usize;
        let frac = state.position - idx as f64;

        // Interpolate between samples (offset by 1 due to prepended last_sample)
        let sample = if idx + 1 < extended_input.len() {
            let s0 = extended_input[idx];
            let s1 = extended_input[idx + 1];
            s0 + (s1 - s0) * frac as f32
        } else {
            extended_input[extended_input.len() - 1]
        };

        output.push(sample);
        state.position += 1.0 / ratio;
    }

    // Adjust position for next buffer
    state.position -= input.len() as f64;
    state.last_sample = *input.last().unwrap_or(&0.0);

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_linear_downsample() {
        // Test downsampling from 48kHz to 16kHz (ratio = 1/3)
        let mut state = ResampleState::new();
        let input: Vec<f32> = (0..48).map(|i| i as f32 / 48.0).collect();
        let ratio = 16000.0 / 48000.0;

        let output = resample_linear(&input, ratio, &mut state);

        // Output should be roughly 1/3 the size
        assert!(output.len() >= 15 && output.len() <= 17);
    }

    #[test]
    fn test_resample_linear_upsample() {
        // Test upsampling from 8kHz to 16kHz (ratio = 2)
        let mut state = ResampleState::new();
        let input: Vec<f32> = (0..8).map(|i| i as f32 / 8.0).collect();
        let ratio = 16000.0 / 8000.0;

        let output = resample_linear(&input, ratio, &mut state);

        // Output should be roughly 2x the size
        assert!(output.len() >= 15 && output.len() <= 17);
    }

    #[test]
    fn test_resample_linear_same_rate() {
        let mut state = ResampleState::new();
        let input: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let ratio = 1.0;

        let output = resample_linear(&input, ratio, &mut state);

        assert_eq!(output.len(), input.len());
    }
}
