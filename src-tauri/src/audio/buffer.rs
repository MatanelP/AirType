//! Audio sample buffer for storing recorded audio data.
//!
//! Provides a thread-safe buffer that can operate in two modes:
//! - Batch mode: Collect all samples until recording stops
//! - Streaming mode: Periodically drain chunks for live transcription

use parking_lot::Mutex;
use std::sync::Arc;

/// Target sample rate for Whisper (16kHz)
pub const TARGET_SAMPLE_RATE: u32 = 16000;

/// Default chunk size for streaming mode (~0.5 seconds of audio)
pub const DEFAULT_CHUNK_SIZE: usize = 8000;

/// Thread-safe audio sample buffer
#[derive(Clone)]
pub struct AudioBuffer {
    inner: Arc<Mutex<BufferInner>>,
}

struct BufferInner {
    samples: Vec<f32>,
    chunk_size: usize,
}

impl AudioBuffer {
    /// Create a new audio buffer with default chunk size
    pub fn new() -> Self {
        Self::with_chunk_size(DEFAULT_CHUNK_SIZE)
    }

    /// Create a new audio buffer with specified chunk size for streaming
    pub fn with_chunk_size(chunk_size: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BufferInner {
                samples: Vec::with_capacity(TARGET_SAMPLE_RATE as usize * 30), // Pre-allocate for 30s
                chunk_size,
            })),
        }
    }

    /// Push samples into the buffer
    pub fn push_samples(&self, samples: &[f32]) {
        let mut inner = self.inner.lock();
        inner.samples.extend_from_slice(samples);
    }

    /// Get all samples without clearing the buffer
    pub fn get_samples(&self) -> Vec<f32> {
        let inner = self.inner.lock();
        inner.samples.clone()
    }

    /// Take all samples and clear the buffer
    pub fn take_samples(&self) -> Vec<f32> {
        let mut inner = self.inner.lock();
        std::mem::take(&mut inner.samples)
    }

    /// Take a chunk of samples if enough are available (for streaming mode)
    /// Returns None if not enough samples are buffered
    pub fn take_chunk(&self) -> Option<Vec<f32>> {
        let mut inner = self.inner.lock();
        let chunk_size = inner.chunk_size;
        if inner.samples.len() >= chunk_size {
            let chunk: Vec<f32> = inner.samples.drain(..chunk_size).collect();
            Some(chunk)
        } else {
            None
        }
    }

    /// Take all available samples as a chunk, even if less than chunk_size
    /// Useful for flushing remaining samples at the end of recording
    pub fn flush(&self) -> Vec<f32> {
        self.take_samples()
    }

    /// Clear all samples from the buffer
    pub fn clear(&self) {
        let mut inner = self.inner.lock();
        inner.samples.clear();
    }

    /// Get the current number of samples in the buffer
    pub fn len(&self) -> usize {
        let inner = self.inner.lock();
        inner.samples.len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get duration of buffered audio in seconds
    pub fn duration_secs(&self) -> f32 {
        self.len() as f32 / TARGET_SAMPLE_RATE as f32
    }

    /// Check if a full chunk is available for streaming
    pub fn has_chunk(&self) -> bool {
        let inner = self.inner.lock();
        inner.samples.len() >= inner.chunk_size
    }

    /// Set the chunk size for streaming mode
    pub fn set_chunk_size(&self, chunk_size: usize) {
        let mut inner = self.inner.lock();
        inner.chunk_size = chunk_size;
    }
}

impl Default for AudioBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_get_samples() {
        let buffer = AudioBuffer::new();
        let samples = vec![0.1, 0.2, 0.3, 0.4, 0.5];

        buffer.push_samples(&samples);

        assert_eq!(buffer.len(), 5);
        assert_eq!(buffer.get_samples(), samples);
    }

    #[test]
    fn test_take_samples_clears_buffer() {
        let buffer = AudioBuffer::new();
        buffer.push_samples(&[0.1, 0.2, 0.3]);

        let taken = buffer.take_samples();

        assert_eq!(taken, vec![0.1, 0.2, 0.3]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_take_chunk() {
        let buffer = AudioBuffer::with_chunk_size(3);
        buffer.push_samples(&[0.1, 0.2, 0.3, 0.4, 0.5]);

        let chunk = buffer.take_chunk();

        assert_eq!(chunk, Some(vec![0.1, 0.2, 0.3]));
        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn test_take_chunk_not_enough_samples() {
        let buffer = AudioBuffer::with_chunk_size(10);
        buffer.push_samples(&[0.1, 0.2, 0.3]);

        assert!(buffer.take_chunk().is_none());
        assert_eq!(buffer.len(), 3);
    }

    #[test]
    fn test_duration_secs() {
        let buffer = AudioBuffer::new();
        buffer.push_samples(&vec![0.0; TARGET_SAMPLE_RATE as usize]);

        assert!((buffer.duration_secs() - 1.0).abs() < 0.001);
    }
}
