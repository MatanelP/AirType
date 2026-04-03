//! Transcription module for AirType
//!
//! Provides Whisper-based speech-to-text transcription with support for
//! batch and streaming modes, multiple languages, and lazy model loading.
//! Also supports OpenAI Realtime API for live streaming transcription.

mod openai_realtime;
mod openai_batch;
mod runpod;
mod test_audio;
mod whisper;

pub use openai_batch::transcribe_english_test;
pub use openai_realtime::OpenAIRealtimeTranscriber;
pub use runpod::{transcribe_hebrew, transcribe_hebrew_wav, validate_runpod};
pub use test_audio::{english_test_wav, hebrew_test_wav};
pub use whisper::{
    create_shared_transcriber, Language, Result, SharedTranscriber, TranscriptionError,
    WhisperTranscriber,
};
