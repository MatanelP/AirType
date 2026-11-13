//! OpenAI batch transcription helper for English audio.

use reqwest::header::AUTHORIZATION;
use serde_json::Value;

use super::punctuation::SPOKEN_PUNCTUATION_PROMPT;

const OPENAI_TRANSCRIPTION_URL: &str = "https://api.openai.com/v1/audio/transcriptions";

fn transcription_fields(spoken_punctuation: bool) -> Vec<(&'static str, &'static str)> {
    let mut fields = vec![
        ("model", "whisper-1"),
        ("language", "en"),
        ("response_format", "json"),
    ];
    if spoken_punctuation {
        fields.push(("prompt", SPOKEN_PUNCTUATION_PROMPT));
    }
    fields
}

/// Transcribe English audio with OpenAI's batch transcription API.
/// When enabled, give Whisper examples of spelled-out punctuation commands.
pub async fn transcribe_english(
    api_key: &str,
    wav_bytes: &[u8],
    spoken_punctuation: bool,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let file_part = reqwest::multipart::Part::bytes(wav_bytes.to_vec())
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| format!("Invalid audio payload: {}", e))?;

    let mut form = reqwest::multipart::Form::new();
    for (name, value) in transcription_fields(spoken_punctuation) {
        form = form.text(name, value);
    }
    let form = form.part("file", file_part);

    log::info!("Sending English audio to OpenAI batch API...");

    let response = client
        .post(OPENAI_TRANSCRIPTION_URL)
        .header(AUTHORIZATION, format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("OpenAI request failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed reading OpenAI response: {}", e))?;

    if !status.is_success() {
        if let Ok(json) = serde_json::from_str::<Value>(&body) {
            let error = json.get("error");
            let code = error
                .and_then(|e| e.get("code"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let message = error
                .and_then(|e| e.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or(body.trim());

            if status.as_u16() == 429 || code == "insufficient_quota" {
                return Err(format!("OpenAI needs billing or credits: {}", message));
            }

            return Err(format!("OpenAI API error ({}): {}", status, message));
        }

        return Err(format!("OpenAI API error ({}): {}", status, body));
    }

    if let Ok(json) = serde_json::from_str::<Value>(&body) {
        if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
            let text = text.trim().to_string();
            log::info!("OpenAI English transcription: {}", text);
            return Ok(text);
        }
    }

    let text = body.trim().to_string();
    if text.is_empty() {
        return Err("OpenAI returned empty transcription".to_string());
    }

    log::info!("OpenAI English transcription: {}", text);
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spoken_punctuation_disabled_preserves_openai_fields() {
        assert_eq!(
            transcription_fields(false),
            vec![
                ("model", "whisper-1"),
                ("language", "en"),
                ("response_format", "json"),
            ]
        );
    }

    #[test]
    fn spoken_punctuation_enabled_adds_openai_vocabulary_context() {
        let fields = transcription_fields(true);
        let prompt = fields
            .iter()
            .find(|(name, _)| *name == "prompt")
            .expect("enabled dictation must send spoken punctuation context")
            .1;
        let prompt = prompt.to_ascii_lowercase();
        for command in [
            "comma",
            "period",
            "dot",
            "full stop",
            "question mark",
            "exclamation mark",
            "colon",
            "semicolon",
            "new line",
            "new paragraph",
            "literal",
        ] {
            assert!(prompt.contains(command), "missing command: {command}");
        }
        assert_eq!(
            fields
                .iter()
                .filter(|(name, _)| *name == "model")
                .copied()
                .collect::<Vec<_>>(),
            vec![("model", "whisper-1")]
        );
    }
}
