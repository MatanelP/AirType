//! HuggingFace Inference API client for Hebrew transcription
//!
//! Uses openai/whisper-large-v3 via HuggingFace's hf-inference provider
//! with Hebrew language hint. Pay-per-use, runs on HF's GPUs.

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

const HF_INFERENCE_URL: &str =
    "https://router.huggingface.co/hf-inference/models/openai/whisper-large-v3";

/// Transcribe audio using HuggingFace's Inference API with whisper-large-v3.
///
/// # Arguments
/// * `api_key` - HuggingFace API token (hf_...)
/// * `audio_samples` - f32 mono samples at 16kHz
///
/// # Returns
/// Transcribed Hebrew text
pub async fn transcribe_hebrew(api_key: &str, audio_samples: &[f32]) -> Result<String, String> {
    let wav_bytes = encode_wav(audio_samples, 16000);

    log::info!(
        "Sending {:.1}s of audio to HuggingFace whisper-large-v3 (Hebrew)...",
        audio_samples.len() as f64 / 16000.0
    );

    let client = reqwest::Client::new();
    let response = client
        .post(HF_INFERENCE_URL)
        .header(AUTHORIZATION, format!("Bearer {}", api_key))
        .header(CONTENT_TYPE, "audio/wav")
        .body(wav_bytes)
        .send()
        .await
        .map_err(|e| format!("HuggingFace request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("HuggingFace API error ({}): {}", status, body));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse HuggingFace response: {}", e))?;

    // Response format: { "text": "transcribed text" }
    let text = result
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    log::info!("HuggingFace transcription: {}", text);
    Ok(text)
}

/// Validate a HuggingFace API key by making a simple request
pub async fn validate_hf_key(api_key: &str) -> bool {
    let client = reqwest::Client::new();
    let response = client
        .get("https://huggingface.co/api/whoami-v2")
        .header(AUTHORIZATION, format!("Bearer {}", api_key))
        .send()
        .await;

    match response {
        Ok(r) => r.status().is_success(),
        Err(_) => false,
    }
}

/// Encode f32 audio samples as a WAV file in memory
fn encode_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let num_samples = samples.len() as u32;
    let bytes_per_sample = 2u16; // 16-bit PCM
    let num_channels = 1u16;
    let data_size = num_samples * bytes_per_sample as u32;
    let file_size = 36 + data_size;

    let mut buf = Vec::with_capacity(file_size as usize + 8);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    buf.extend_from_slice(&num_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * num_channels as u32 * bytes_per_sample as u32;
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    let block_align = num_channels * bytes_per_sample;
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&(bytes_per_sample * 8).to_le_bytes()); // bits per sample

    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    for &sample in samples {
        let s = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
        buf.extend_from_slice(&s.to_le_bytes());
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_wav_header() {
        let samples = vec![0.0f32; 16000]; // 1 second of silence
        let wav = encode_wav(&samples, 16000);

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        // Data size = 16000 samples * 2 bytes = 32000
        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size, 32000);
    }
}
