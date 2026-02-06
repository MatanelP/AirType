//! Transcription module for AirType
//!
//! Provides Whisper-based speech-to-text transcription with support for
//! batch and streaming modes, multiple languages, and lazy model loading.

mod whisper;

pub use whisper::{
    create_shared_transcriber, Language, Result, SharedTranscriber, TranscriptionError,
    WhisperTranscriber,
};
