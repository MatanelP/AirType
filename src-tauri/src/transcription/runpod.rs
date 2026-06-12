//! RunPod Serverless API client for transcription
//!
//! Uses ivrit-ai's official RunPod Serverless deployment with the
//! ivrit-ai/whisper-large-v3-turbo-ct2 model. Pay-per-second pricing.
//! Supports both Hebrew (ivrit-ai optimised) and English via language param.
//! See: https://github.com/ivrit-ai/runpod-serverless

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

const RUNPOD_API_BASE: &str = "https://api.runpod.ai/v2";

/// RunPod /run request payload
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
    task: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_options: Option<OutputOptions>,
}

#[derive(Serialize)]
struct OutputOptions {
    word_timestamps: bool,
    extra_data: bool,
}

/// Response from POST /run (async job submission)
#[derive(Deserialize, Debug)]
struct RunPodJobResponse {
    id: String,
    status: String,
}

/// Response from GET /status/{id}
#[derive(Deserialize, Debug)]
struct RunPodStatusResponse {
    status: String,
    output: Option<serde_json::Value>,
    error: Option<String>,
}

/// Transcribe audio using ivrit-ai on RunPod Serverless.
///
/// `on_status` is called with `"warming_up"` when the worker is cold-starting
/// (job is IN_QUEUE) and `"processing"` when the worker is actively transcribing
/// (job is IN_PROGRESS). Pass `|_| {}` if you don't need status updates.
pub async fn transcribe_audio<F>(
    api_key: &str,
    endpoint_id: &str,
    audio_samples: &[f32],
    language: &str,
    on_status: F,
) -> Result<String, String>
where
    F: Fn(&str) + Send,
{
    let wav_bytes = encode_wav(audio_samples, 16000);
    transcribe_audio_wav(api_key, endpoint_id, &wav_bytes, language, on_status).await
}

/// Transcribe Hebrew audio (convenience wrapper, no status callback).
pub async fn transcribe_hebrew(
    api_key: &str,
    endpoint_id: &str,
    audio_samples: &[f32],
) -> Result<String, String> {
    transcribe_audio(api_key, endpoint_id, audio_samples, "he", |_| {}).await
}

/// Transcribe WAV bytes using ivrit-ai on RunPod Serverless.
///
/// Submits a job with POST /run, then polls GET /status/{id} until the job
/// completes, fails, or times out (120 s). `on_status` receives:
/// - `"warming_up"` — worker is cold-starting (IN_QUEUE)
/// - `"processing"` — worker is actively transcribing (IN_PROGRESS)
pub async fn transcribe_audio_wav<F>(
    api_key: &str,
    endpoint_id: &str,
    wav_bytes: &[u8],
    language: &str,
    on_status: F,
) -> Result<String, String>
where
    F: Fn(&str) + Send,
{
    let blob = BASE64.encode(wav_bytes);

    let data_len = wav_bytes.len().saturating_sub(44);
    let duration = data_len as f64 / 2.0 / 16000.0;
    log::info!("Sending {:.1}s of {} audio to RunPod endpoint {}...", duration, language, endpoint_id);

    let initial_prompt = if language == "en" {
        Some("This is an English transcription. Please write the output in English script, using proper grammar and spelling.".to_string())
    } else {
        None
    };

    let task = if language == "en" {
        "translate" // forces Whisper output to English
    } else {
        "transcribe"
    };

    let payload = RunPodRequest {
        input: RunPodInput {
            model: "ivrit-ai/whisper-large-v3-turbo-ct2".to_string(),
            engine: "faster-whisper".to_string(),
            streaming: false,
            transcribe_args: TranscribeArgs {
                blob,
                language: language.to_string(),
                task: Some(task.to_string()),
                initial_prompt,
                output_options: Some(OutputOptions {
                    word_timestamps: false,
                    extra_data: false,
                }),
            },
        },
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // Submit job asynchronously
    let run_url = format!("{}/{}/run", RUNPOD_API_BASE, endpoint_id);
    let job: RunPodJobResponse = client
        .post(&run_url)
        .header(AUTHORIZATION, format!("Bearer {}", api_key))
        .header(CONTENT_TYPE, "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("RunPod submit failed: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse RunPod job response: {}", e))?;

    log::info!("RunPod job submitted: {} (initial status: {})", job.id, job.status);

    // Fast-path: job already completed synchronously (rare but possible on warm workers)
    if job.status == "COMPLETED" {
        log::info!("RunPod job {} completed immediately", job.id);
        // We don't have output here from the /run response, fall through to status poll
    }

    // Poll for completion
    let status_url = format!("{}/{}/status/{}", RUNPOD_API_BASE, endpoint_id, job.id);
    let start = std::time::Instant::now();
    let mut last_emitted = String::new();

    loop {
        if start.elapsed().as_secs() > 120 {
            return Err("RunPod job timed out after 120 seconds".to_string());
        }

        let resp = client
            .get(&status_url)
            .header(AUTHORIZATION, format!("Bearer {}", api_key))
            .send()
            .await
            .map_err(|e| format!("RunPod status poll failed: {}", e))?;

        let status_resp: RunPodStatusResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse RunPod status response: {}", e))?;

        log::info!("RunPod job {} status: {}", job.id, status_resp.status);

        match status_resp.status.as_str() {
            "IN_QUEUE" => {
                if last_emitted != "warming_up" {
                    on_status("warming_up");
                    last_emitted = "warming_up".to_string();
                }
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            }
            "IN_PROGRESS" => {
                if last_emitted != "processing" {
                    on_status("processing");
                    last_emitted = "processing".to_string();
                }
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            }
            "COMPLETED" => {
                if let Some(error) = status_resp.error {
                    return Err(format!("RunPod job error: {}", error));
                }
                let text = extract_text_from_output(&status_resp.output)?;
                log::info!("RunPod {} transcription: {}", language, text);
                return Ok(text);
            }
            "FAILED" | "CANCELLED" => {
                return Err(format!(
                    "RunPod job {} (status: {})",
                    status_resp.error.unwrap_or_else(|| "unknown error".to_string()),
                    status_resp.status,
                ));
            }
            other => {
                log::warn!("Unknown RunPod status: {}", other);
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            }
        }
    }
}

/// Transcribe Hebrew WAV bytes (convenience wrapper, no status callback).
pub async fn transcribe_hebrew_wav(
    api_key: &str,
    endpoint_id: &str,
    wav_bytes: &[u8],
) -> Result<String, String> {
    transcribe_audio_wav(api_key, endpoint_id, wav_bytes, "he", |_| {}).await
}

/// Extract transcription text from RunPod output
fn extract_text_from_output(output: &Option<serde_json::Value>) -> Result<String, String> {
    let output = output
        .as_ref()
        .ok_or_else(|| "RunPod returned no output".to_string())?;

    // RunPod wraps non-streaming output in an array: [{"result": [[...segments...]]}]
    let doc = if let Some(arr) = output.as_array() {
        arr.first().unwrap_or(output)
    } else {
        output
    };

    // Format: { "result": [[{...segment...}]] }
    if let Some(result) = doc.get("result") {
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
pub fn encode_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
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
        // RunPod wraps output in an array
        let output = serde_json::json!([{
            "result": [[
                {"text": " שלום עולם ", "start": 0.0, "end": 1.0},
                {"text": " מה שלומך ", "start": 1.0, "end": 2.0}
            ]]
        }]);
        let text = extract_text_from_output(&Some(output)).unwrap();
        assert_eq!(text, "שלום עולם מה שלומך");
    }

    #[test]
    fn test_extract_text_unwrapped() {
        // Also handle non-array format
        let output = serde_json::json!({
            "result": [[
                {"text": " טסט ", "start": 0.0, "end": 1.0}
            ]]
        });
        let text = extract_text_from_output(&Some(output)).unwrap();
        assert_eq!(text, "טסט");
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
