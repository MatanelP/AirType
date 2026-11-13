//! Conservative suppression of filler text returned for digital silence.

const HEBREW_FILLER_PHRASES: &[&str] = &["תודה", "תודה רבה"];

/// Text matching alone does not establish whether a phrase was spoken.
pub fn is_hebrew_filler(text: &str) -> bool {
    let normalized = normalize_filler(text);
    !normalized.is_empty()
        && HEBREW_FILLER_PHRASES
            .iter()
            .any(|phrase| normalize_filler(phrase) == normalized)
}

fn normalize_filler(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_ascii_punctuation() && !matches!(c, '،' | '؛' | '׃' | '״' | '׳' | '…'))
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether capture evidence permits discarding this filler-only result.
/// At 16 kHz each window spans 10 ms. Every captured sample must be finite
/// digital zero; any nonzero signal may contain speech, however quiet or brief.
/// Empty or nonfinite data is unknown evidence and must be kept.
pub fn should_discard_hebrew_filler(text: &str, language: &str, samples: &[f32]) -> bool {
    language == "he"
        && is_hebrew_filler(text)
        && !samples.is_empty()
        && samples.chunks(160).all(|window| {
            window
                .iter()
                .all(|sample| sample.is_finite() && *sample == 0.0)
        })
}

#[cfg(test)]
mod tests {
    use super::{is_hebrew_filler, should_discard_hebrew_filler};

    #[test]
    fn matches_only_the_existing_filler_phrases() {
        for text in [
            "תודה",
            "תודה.",
            "תודה רבה",
            "תודה רבה.",
            "  תודה רבה!  ",
            "תודה  רבה",
        ] {
            assert!(is_hebrew_filler(text), "{text:?}");
        }
        for text in ["", "תודה על העזרה", "שלום", "תודה רבה על הכל", "hello"]
        {
            assert!(!is_hebrew_filler(text), "{text:?}");
        }
    }

    #[test]
    fn keeps_one_second_of_speech_followed_by_twenty_nine_seconds_of_silence() {
        let mut samples = vec![0.1; 16_000];
        samples.resize(30 * 16_000, 0.0);
        assert!(!should_discard_hebrew_filler("תודה", "he", &samples));
    }

    #[test]
    fn keeps_quiet_and_arbitrarily_small_nonzero_signals() {
        for amplitude in [
            0.0005,
            0.0001,
            -0.0001,
            f32::MIN_POSITIVE,
            f32::from_bits(1),
        ] {
            assert!(
                !should_discard_hebrew_filler("תודה רבה.", "he", &[amplitude; 320]),
                "amplitude={amplitude:?}"
            );
        }
    }

    #[test]
    fn keeps_brief_quiet_speech_surrounded_by_silence() {
        let mut samples = vec![0.0; 32_000];
        samples[16_000..16_080].fill(0.0001);
        assert!(!should_discard_hebrew_filler("תודה", "he", &samples));
    }

    #[test]
    fn keeps_a_nonzero_sample_at_any_capture_window_boundary() {
        for index in [0, 159, 160, 319, 320] {
            let mut samples = vec![0.0; 321];
            samples[index] = f32::from_bits(1);
            assert!(
                !should_discard_hebrew_filler("תודה", "he", &samples),
                "index={index}"
            );
        }
    }

    #[test]
    fn discards_only_filler_on_nonempty_digital_silence() {
        for text in ["תודה", "תודה.", "תודה רבה!", "  תודה  רבה… "] {
            for length in [1, 159, 160, 161, 16_000] {
                assert!(should_discard_hebrew_filler(text, "he", &vec![0.0; length]));
            }
        }
    }

    #[test]
    fn keeps_empty_or_nonfinite_capture_data() {
        assert!(!should_discard_hebrew_filler("תודה", "he", &[]));
        for unknown in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(!should_discard_hebrew_filler("תודה", "he", &[unknown]));
            assert!(!should_discard_hebrew_filler(
                "תודה",
                "he",
                &[0.0, unknown, 0.0]
            ));
        }
    }

    #[test]
    fn keeps_nonfiller_transcripts_and_other_languages_even_on_digital_silence() {
        for text in [
            "",
            " ",
            ".",
            "שלום",
            "תודה על העזרה",
            "תודה רבה על הכל",
            "hello",
        ] {
            assert!(!should_discard_hebrew_filler(text, "he", &[0.0; 160]));
        }
        for language in ["en", "fr", "", "HE"] {
            assert!(!should_discard_hebrew_filler("תודה", language, &[0.0; 160]));
        }
    }
}
