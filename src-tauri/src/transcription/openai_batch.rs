//! OpenAI batch transcription helper for bundled test audio.

use reqwest::header::AUTHORIZATION;
use serde_json::Value;

const OPENAI_TRANSCRIPTION_URL: &str = "https://api.openai.com/v1/audio/transcriptions";

/// Transcribe English audio with OpenAI's batch transcription API.
pub async fn transcribe_english_test(api_key: &str, wav_bytes: &[u8]) -> Result<String, String> {
    let client = reqwest::Client::new();
    let file_part = reqwest::multipart::Part::bytes(wav_bytes.to_vec())
        .file_name("english-test.wav")
        .mime_str("audio/wav")
        .map_err(|e| format!("Invalid audio payload: {}", e))?;

    let form = reqwest::multipart::Form::new()
        .text("model", "gpt-4o-transcribe")
        .text("language", "en")
        .text("response_format", "json")
        .part("file", file_part);

    log::info!("Sending bundled English test audio to OpenAI...");

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
                return Err(format!(
                    "OpenAI test needs billing or credits: {}",
                    message
                ));
            }

            return Err(format!("OpenAI API error ({}): {}", status, message));
        }

        return Err(format!("OpenAI API error ({}): {}", status, body));
    }

    if let Ok(json) = serde_json::from_str::<Value>(&body) {
        if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
            let text = text.trim().to_string();
            log::info!("OpenAI English test transcription: {}", text);
            return Ok(text);
        }
    }

    let text = body.trim().to_string();
    if text.is_empty() {
        return Err("OpenAI returned empty transcription".to_string());
    }

    log::info!("OpenAI English test transcription: {}", text);
    Ok(text)
}
