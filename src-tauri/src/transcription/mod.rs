//! Transcription module for AirType
//!
//! Provides batch speech-to-text transcription using RunPod and OpenAI APIs.

mod openai_batch;
mod runpod;
mod test_audio;

pub use openai_batch::transcribe_english;
pub use runpod::{encode_wav, transcribe_audio, transcribe_audio_wav, transcribe_hebrew, transcribe_hebrew_wav, validate_runpod};
pub use test_audio::{english_test_wav, hebrew_test_wav};

/// Hebrew "thank you"-style phrases the speech model commonly hallucinates from
/// silence or unclear audio. When the transcript is only one of these, we treat
/// it as if nothing was said (no injection, no history entry).
const HEBREW_FILLER_PHRASES: &[&str] = &["תודה", "תודה רבה"];

/// Returns true if `text` is only a Hebrew filler the model hallucinates on
/// silence (e.g. "תודה", "תודה רבה", with any surrounding punctuation/whitespace)
/// and should be discarded.
pub fn is_hebrew_filler(text: &str) -> bool {
    let normalized = normalize_filler(text);
    !normalized.is_empty()
        && HEBREW_FILLER_PHRASES
            .iter()
            .any(|p| normalize_filler(p) == normalized)
}

/// Strip punctuation and collapse whitespace so "תודה.", "תודה רבה!", and
/// "תודה  רבה" all compare equal to their base phrase.
fn normalize_filler(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_ascii_punctuation() && !matches!(c, '،' | '؛' | '׃' | '״' | '׳' | '…'))
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::is_hebrew_filler;

    #[test]
    fn discards_bare_and_punctuated_fillers() {
        for s in ["תודה", "תודה.", "תודה רבה", "תודה רבה.", "  תודה רבה!  ", "תודה  רבה"] {
            assert!(is_hebrew_filler(s), "should discard: {s:?}");
        }
    }

    #[test]
    fn keeps_real_transcripts() {
        for s in ["", "תודה על העזרה", "שלום", "תודה רבה על הכל", "hello"] {
            assert!(!is_hebrew_filler(s), "should keep: {s:?}");
        }
    }
}
