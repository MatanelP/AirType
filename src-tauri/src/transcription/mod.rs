//! Transcription module for AirType
//!
//! Provides Whisper-based speech-to-text transcription with support for
//! batch and streaming modes, multiple languages, and lazy model loading.
//! Also supports OpenAI Realtime API for live streaming transcription.

mod openai_realtime;
mod whisper;

pub use openai_realtime::OpenAIRealtimeTranscriber;
pub use whisper::{
    create_shared_transcriber, Language, Result, SharedTranscriber, TranscriptionError,
    WhisperTranscriber,
};
