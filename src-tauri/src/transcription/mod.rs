//! Transcription module for AirType
//!
//! Provides batch speech-to-text transcription using RunPod and OpenAI APIs.

mod hebrew_filler;
mod openai_batch;
mod runpod;
mod test_audio;

pub use hebrew_filler::{is_hebrew_filler, should_discard_hebrew_filler};
pub use openai_batch::transcribe_english;
pub use runpod::{encode_wav, transcribe_audio, transcribe_audio_wav, transcribe_hebrew, transcribe_hebrew_wav, validate_runpod};
pub use test_audio::{english_test_wav, hebrew_test_wav};
