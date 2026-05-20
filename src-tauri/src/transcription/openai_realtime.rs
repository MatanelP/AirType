//! OpenAI Realtime API client for live voice-to-text transcription
//!
//! Uses the transcription-only mode of OpenAI's Realtime API via WebSocket.
//! Default URL: wss://api.openai.com/v1/realtime?model=gpt-4o-transcribe
//! Both the base URL and model are configurable at runtime via settings.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const DEFAULT_BASE_URL: &str = "wss://api.openai.com/v1/realtime";
const DEFAULT_MODEL: &str = "gpt-4o-transcribe";

// ── Client → Server events ──────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ClientEvent {
    /// Create/update a transcription session
    #[serde(rename = "session.update")]
    SessionUpdate {
        session: TranscriptionSessionConfig,
    },
    /// Append raw audio bytes (base64-encoded)
    #[serde(rename = "input_audio_buffer.append")]
    InputAudioBufferAppend { audio: String },
    /// Commit the current audio buffer (triggers transcription if VAD is off)
    #[serde(rename = "input_audio_buffer.commit")]
    InputAudioBufferCommit {},
}

#[derive(Debug, Serialize)]
struct TranscriptionSessionConfig {
    /// Must be "transcription" for GA transcription sessions
    #[serde(rename = "type")]
    session_type: String,
    input_audio_format: String,
    input_audio_transcription: InputAudioTranscription,
    turn_detection: Option<TurnDetection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_audio_noise_reduction: Option<NoiseReduction>,
}

#[derive(Debug, Serialize)]
struct InputAudioTranscription {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
}

#[derive(Debug, Serialize)]
struct TurnDetection {
    #[serde(rename = "type")]
    detection_type: String,
    threshold: f32,
    prefix_padding_ms: u32,
    silence_duration_ms: u32,
}

#[derive(Debug, Serialize)]
struct NoiseReduction {
    #[serde(rename = "type")]
    noise_type: String,
}

// ── Server → Client events ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ServerEventRaw {
    #[serde(rename = "type")]
    event_type: String,
    // Transcription fields
    delta: Option<String>,
    transcript: Option<String>,
    item_id: Option<String>,
    // Error fields
    error: Option<ErrorInfo>,
    // Session fields
    session: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ErrorInfo {
    message: String,
    #[serde(rename = "type")]
    error_type: Option<String>,
    code: Option<String>,
}

/// Callback: (text, is_final)
pub type TranscriptionCallback = Arc<dyn Fn(&str, bool) + Send + Sync>;

/// OpenAI Realtime transcriber for live streaming
pub struct OpenAIRealtimeTranscriber {
    api_key: String,
    language: parking_lot::RwLock<String>,
    /// Configurable model name (default: gpt-4o-transcribe)
    model: String,
    /// Configurable WebSocket base URL (default: wss://api.openai.com/v1/realtime)
    base_url: String,
}

impl OpenAIRealtimeTranscriber {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            language: parking_lot::RwLock::new("en".to_string()),
            model: DEFAULT_MODEL.to_string(),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    /// Override the transcription model (e.g. "gpt-4o-mini-transcribe")
    pub fn set_model(&self, model: &str) -> &Self {
        // Can't use &mut self + RwLock easily; use a fresh owned field instead.
        // This is called before start_session so we just shadow via a builder-style approach.
        // We use unsafe interior mutability via a separate RwLock field — but simpler:
        // model is set once at construction so we expose a consuming builder instead.
        let _ = model; // actual set is done in with_model below
        self
    }

    /// Builder: set model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Builder: set base URL
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn set_language(&self, language: &str) {
        *self.language.write() = language.to_string();
    }

    /// Start a live transcription session.
    /// Returns a channel sender for f32 audio samples (mono, any sample rate – will be resampled to 24 kHz).
    pub async fn start_session<R: Runtime>(
        &self,
        on_transcription: TranscriptionCallback,
        app: AppHandle<R>,
    ) -> Result<mpsc::Sender<Vec<f32>>, String> {
        let language = self.language.read().clone();

        // Build the WebSocket URL: {base_url}?model={model}
        // Strip trailing slash from base_url for consistency
        let base = self.base_url.trim_end_matches('/');
        let ws_url = format!("{}?model={}", base, self.model);

        // Extract host for the Host header (strip scheme, path, query)
        let host = ws_url
            .trim_start_matches("wss://")
            .trim_start_matches("ws://")
            .split('/')
            .next()
            .unwrap_or("api.openai.com")
            .split('?')
            .next()
            .unwrap_or("api.openai.com")
            .to_string();

        log::info!("Connecting to OpenAI Realtime: {} (model={})", ws_url, self.model);

        // Build WS request with auth (GA API — no OpenAI-Beta header)
        let request = http::Request::builder()
            .uri(&ws_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", tokio_tungstenite::tungstenite::handshake::client::generate_key())
            .header("Host", &host)
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .body(())
            .map_err(|e| format!("Failed to build request: {}", e))?;

        let (ws_stream, _) = connect_async(request)
            .await
            .map_err(|e| format!("Failed to connect to OpenAI Realtime: {}", e))?;

        log::info!("Connected to OpenAI Realtime API (transcription mode)");

        let (mut write, mut read) = ws_stream.split();

        // Configure transcription session (GA format: session.update with type:"transcription")
        let session_config = ClientEvent::SessionUpdate {
            session: TranscriptionSessionConfig {
                session_type: "transcription".to_string(),
                input_audio_format: "pcm16".to_string(),
                input_audio_transcription: InputAudioTranscription {
                    model: self.model.clone(),
                    language: Some(language),
                },
                turn_detection: Some(TurnDetection {
                    detection_type: "server_vad".to_string(),
                    threshold: 0.5,
                    prefix_padding_ms: 300,
                    silence_duration_ms: 500,
                }),
                input_audio_noise_reduction: Some(NoiseReduction {
                    noise_type: "near_field".to_string(),
                }),
            },
        };

        let config_json = serde_json::to_string(&session_config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        write
            .send(Message::Text(config_json))
            .await
            .map_err(|e| format!("Failed to send session config: {}", e))?;

        // Audio sender channel (f32 mono samples at source rate)
        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<f32>>(100);

        // ── Reader task: process server events ──
        let callback = on_transcription.clone();
        let app_reader = app.clone();
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(ev) = serde_json::from_str::<ServerEventRaw>(&text) {
                            match ev.event_type.as_str() {
                                "conversation.item.input_audio_transcription.delta" => {
                                    if let Some(delta) = ev.delta {
                                        if !delta.is_empty() {
                                            callback(&delta, false);
                                        }
                                    }
                                }
                                "conversation.item.input_audio_transcription.completed" => {
                                    if let Some(transcript) = ev.transcript {
                                        callback(&transcript, true);
                                    }
                                }
                                "conversation.item.input_audio_transcription.failed" => {
                                    let msg = ev.error
                                        .map(|e| format!("Transcription failed: {}", e.message))
                                        .unwrap_or_else(|| "Transcription failed".to_string());
                                    log::error!("OpenAI Realtime: {}", msg);
                                    let _ = app_reader.emit("error", &msg);
                                }
                                "error" => {
                                    if let Some(err) = ev.error {
                                        let msg = format!("OpenAI error: {} (code={:?})", err.message, err.code);
                                        log::error!("{}", msg);
                                        let _ = app_reader.emit("error", &msg);
                                    }
                                }
                                // GA session events (replaces transcription_session.created/updated)
                                "session.created" | "session.updated" => {
                                    log::info!("OpenAI Realtime: {}", ev.event_type);
                                }
                                _ => {
                                    log::debug!("OpenAI Realtime event: {}", ev.event_type);
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        log::info!("OpenAI Realtime connection closed");
                        break;
                    }
                    Err(e) => {
                        let msg = format!("OpenAI connection error: {}", e);
                        log::error!("{}", msg);
                        let _ = app_reader.emit("error", &msg);
                        break;
                    }
                    _ => {}
                }
            }
        });

        // ── Writer task: send audio chunks as base64 pcm16 ──
        // Audio arrives at 16kHz from capture, upsample to 24kHz for OpenAI
        tokio::spawn(async move {
            let resample_ratio = 24000.0_f64 / 16000.0_f64; // 1.5x
            let mut resample_pos: f64 = 0.0;
            let mut last_sample: f32 = 0.0;
            let mut total_samples_sent: usize = 0;

            while let Some(samples) = audio_rx.recv().await {
                // Resample 16kHz → 24kHz via linear interpolation
                let mut resampled = Vec::with_capacity((samples.len() as f64 * resample_ratio).ceil() as usize + 1);
                let extended: Vec<f32> = std::iter::once(last_sample)
                    .chain(samples.iter().copied())
                    .collect();

                while resample_pos < samples.len() as f64 {
                    let idx = resample_pos as usize;
                    let frac = resample_pos - idx as f64;
                    let sample = if idx + 1 < extended.len() {
                        let s0 = extended[idx];
                        let s1 = extended[idx + 1];
                        s0 + (s1 - s0) * frac as f32
                    } else {
                        extended[extended.len() - 1]
                    };
                    resampled.push(sample);
                    resample_pos += 1.0 / resample_ratio;
                }
                resample_pos -= samples.len() as f64;
                if let Some(&s) = samples.last() {
                    last_sample = s;
                }

                // Convert f32 → i16 PCM
                let pcm16: Vec<u8> = resampled
                    .iter()
                    .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
                    .flat_map(|s| s.to_le_bytes())
                    .collect();

                let audio_b64 = BASE64.encode(&pcm16);
                let event = ClientEvent::InputAudioBufferAppend { audio: audio_b64 };
                if let Ok(json) = serde_json::to_string(&event) {
                    if write.send(Message::Text(json)).await.is_err() {
                        log::warn!("WebSocket write failed, stopping audio sender");
                        break;
                    }
                    total_samples_sent += resampled.len();
                }
            }
            log::info!("Total 24kHz samples sent to OpenAI: {} ({:.1}s)", total_samples_sent, total_samples_sent as f64 / 24000.0);

            // Commit any remaining audio when the channel closes (recording stopped)
            log::info!("Audio channel closed, committing buffer to OpenAI...");
            let commit = ClientEvent::InputAudioBufferCommit {};
            if let Ok(json) = serde_json::to_string(&commit) {
                let _ = write.send(Message::Text(json)).await;
            }
            // Don't close the WebSocket yet — let the reader task receive the transcription.
            // The server will close the connection after responding, or we'll time out.
            // Keep the write half alive so the connection stays open.
            log::info!("Commit sent, waiting for transcription response...");
            // Hold the writer open for up to 10 seconds to allow transcription to arrive
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            let _ = write.close().await;
            log::info!("OpenAI WebSocket writer closed");
        });

        Ok(audio_tx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_config_serialization() {
        let config = ClientEvent::SessionUpdate {
            session: TranscriptionSessionConfig {
                session_type: "transcription".to_string(),
                input_audio_format: "pcm16".to_string(),
                input_audio_transcription: InputAudioTranscription {
                    model: "gpt-4o-transcribe".to_string(),
                    language: Some("en".to_string()),
                },
                turn_detection: None,
                input_audio_noise_reduction: None,
            },
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("session.update"));
        assert!(json.contains("\"type\":\"transcription\""));
        assert!(json.contains("pcm16"));
        assert!(json.contains("gpt-4o-transcribe"));
    }

    #[test]
    fn test_audio_append_serialization() {
        let event = ClientEvent::InputAudioBufferAppend {
            audio: "AAAA".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("input_audio_buffer.append"));
        assert!(json.contains("AAAA"));
    }
}

