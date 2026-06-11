//! Transcription module for AirType
//!
//! Provides batch speech-to-text transcription using RunPod and OpenAI APIs.

mod openai_batch;
mod runpod;
mod test_audio;

pub use openai_batch::transcribe_english;
pub use runpod::{encode_wav, transcribe_audio, transcribe_audio_wav, transcribe_hebrew, transcribe_hebrew_wav, validate_runpod};
pub use test_audio::{english_test_wav, hebrew_test_wav};
