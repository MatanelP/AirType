//! RunPod Serverless API client for Hebrew transcription
//!
//! Uses ivrit-ai's official RunPod Serverless deployment with the
//! ivrit-ai/whisper-large-v3-turbo-ct2 model. Pay-per-second pricing.
//! See: https://github.com/ivrit-ai/runpod-serverless

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

const RUNPOD_API_BASE: &str = "https://api.runpod.ai/v2";

/// RunPod /runsync request payload
#[derive(Serialize)]
struct RunPodRequest {
    input: RunPodInput,
}

#[derive(Serialize)]
struct RunPodInput {
    model: String,
    engine: String,
    streaming: bool,
    transcribe_args: TranscribeArgs,
}

#[derive(Serialize)]
struct TranscribeArgs {
    blob: String,
    language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_options: Option<OutputOptions>,
}

#[derive(Serialize)]
struct OutputOptions {
    word_timestamps: bool,
    extra_data: bool,
}

/// RunPod /runsync response
#[derive(Deserialize, Debug)]
struct RunPodResponse {
    status: String,
    output: Option<serde_json::Value>,
    error: Option<String>,
}

/// Transcribe audio using ivrit-ai on RunPod Serverless.
///
/// # Arguments
/// * `api_key` - RunPod API key
/// * `endpoint_id` - RunPod endpoint ID for the ivrit-ai deployment
/// * `audio_samples` - f32 mono samples at 16kHz
///
/// # Returns
/// Transcribed Hebrew text
pub async fn transcribe_hebrew(
    api_key: &str,
    endpoint_id: &str,
    audio_samples: &[f32],
) -> Result<String, String> {
    let wav_bytes = encode_wav(audio_samples, 16000);
    let blob = BASE64.encode(&wav_bytes);

    let duration = audio_samples.len() as f64 / 16000.0;
    log::info!(
        "Sending {:.1}s of audio to RunPod ivrit-ai endpoint {}...",
        duration,
        endpoint_id
    );

    let payload = RunPodRequest {
        input: RunPodInput {
            model: "ivrit-ai/whisper-large-v3-turbo-ct2".to_string(),
            engine: "faster-whisper".to_string(),
            streaming: false,
            transcribe_args: TranscribeArgs {
                blob,
                language: "he".to_string(),
                output_options: Some(OutputOptions {
                    word_timestamps: false,
                    extra_data: false,
                }),
            },
        },
    };

    let url = format!("{}/{}/runsync", RUNPOD_API_BASE, endpoint_id);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {}", api_key))
        .header(CONTENT_TYPE, "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("RunPod request failed: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("RunPod API error ({}): {}", status, body));
    }

    let result: RunPodResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse RunPod response: {}", e))?;

    if let Some(error) = result.error {
        return Err(format!("RunPod job error: {}", error));
    }

    if result.status != "COMPLETED" {
        return Err(format!(
            "RunPod job did not complete (status: {}). Try again — worker may be cold-starting.",
            result.status
        ));
    }

    // Parse output: { "result": [[{"text": "...", ...}]] }
    let text = extract_text_from_output(&result.output)?;

    log::info!("RunPod ivrit-ai transcription: {}", text);
    Ok(text)
}

/// Extract transcription text from RunPod output
fn extract_text_from_output(output: &Option<serde_json::Value>) -> Result<String, String> {
    let output = output
        .as_ref()
        .ok_or_else(|| "RunPod returned no output".to_string())?;

    // Non-streaming output format: { "result": [[{...segment...}]] }
    if let Some(result) = output.get("result") {
        let mut full_text = String::new();
        if let Some(outer_arr) = result.as_array() {
            for inner in outer_arr {
                if let Some(segments) = inner.as_array() {
                    for seg in segments {
                        if let Some(text) = seg.get("text").and_then(|t| t.as_str()) {
                            full_text.push_str(text.trim());
                            full_text.push(' ');
                        }
                    }
                }
            }
        }
        let text = full_text.trim().to_string();
        if text.is_empty() {
            return Err("RunPod returned empty transcription".to_string());
        }
        return Ok(text);
    }

    // Direct text field (alternative format)
    if let Some(text) = output.get("text").and_then(|t| t.as_str()) {
        return Ok(text.trim().to_string());
    }

    Err(format!("Unexpected RunPod output format: {}", output))
}

/// Validate a RunPod API key by checking the /health endpoint
pub async fn validate_runpod(api_key: &str, endpoint_id: &str) -> bool {
    if api_key.is_empty() || endpoint_id.is_empty() {
        return false;
    }

    let url = format!("{}/{}/health", RUNPOD_API_BASE, endpoint_id);

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    let response = client
        .get(&url)
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
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    buf.extend_from_slice(&num_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * num_channels as u32 * bytes_per_sample as u32;
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    let block_align = num_channels * bytes_per_sample;
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&(bytes_per_sample * 8).to_le_bytes());

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
        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size, 32000);
    }

    #[test]
    fn test_extract_text_basic() {
        let output = serde_json::json!({
            "result": [[
                {"text": " שלום עולם ", "start": 0.0, "end": 1.0},
                {"text": " מה שלומך ", "start": 1.0, "end": 2.0}
            ]]
        });
        let text = extract_text_from_output(&Some(output)).unwrap();
        assert_eq!(text, "שלום עולם מה שלומך");
    }

    #[test]
    fn test_extract_text_empty() {
        let output = serde_json::json!({ "result": [[]] });
        assert!(extract_text_from_output(&Some(output)).is_err());
    }

    #[test]
    fn test_extract_text_none() {
        assert!(extract_text_from_output(&None).is_err());
    }
}
