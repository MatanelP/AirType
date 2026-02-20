//! OpenAI Realtime API client for live voice-to-text transcription
//!
//! Uses the transcription-only mode of OpenAI's Realtime API via WebSocket.
//! URL: wss://api.openai.com/v1/realtime?intent=transcription
//! Model: gpt-4o-transcribe (streams incremental deltas)

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const OPENAI_REALTIME_URL: &str = "wss://api.openai.com/v1/realtime?intent=transcription";

// ── Client → Server events ──────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ClientEvent {
    /// Create a transcription session
    #[serde(rename = "transcription_session.update")]
    TranscriptionSessionUpdate {
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
}

impl OpenAIRealtimeTranscriber {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            language: parking_lot::RwLock::new("en".to_string()),
        }
    }

    pub fn set_language(&self, language: &str) {
        *self.language.write() = language.to_string();
    }

    /// Start a live transcription session.
    /// Returns a channel sender for f32 audio samples (mono, any sample rate – will be resampled to 24 kHz).
    pub async fn start_session(
        &self,
        on_transcription: TranscriptionCallback,
    ) -> Result<mpsc::Sender<Vec<f32>>, String> {
        let language = self.language.read().clone();

        // Build WS request with auth
        let request = http::Request::builder()
            .uri(OPENAI_REALTIME_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("OpenAI-Beta", "realtime=v1")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", tokio_tungstenite::tungstenite::handshake::client::generate_key())
            .header("Host", "api.openai.com")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .body(())
            .map_err(|e| format!("Failed to build request: {}", e))?;

        let (ws_stream, _) = connect_async(request)
            .await
            .map_err(|e| format!("Failed to connect to OpenAI Realtime: {}", e))?;

        log::info!("Connected to OpenAI Realtime API (transcription mode)");

        let (mut write, mut read) = ws_stream.split();

        // Configure transcription session
        let session_config = ClientEvent::TranscriptionSessionUpdate {
            session: TranscriptionSessionConfig {
                input_audio_format: "pcm16".to_string(),
                input_audio_transcription: InputAudioTranscription {
                    model: "gpt-4o-transcribe".to_string(),
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
                                "error" => {
                                    if let Some(err) = ev.error {
                                        log::error!(
                                            "OpenAI Realtime error: {} (code={:?})",
                                            err.message,
                                            err.code
                                        );
                                    }
                                }
                                "transcription_session.created" | "transcription_session.updated" => {
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
                        log::error!("WebSocket read error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        });

        // ── Writer task: send audio chunks as base64 pcm16 ──
        tokio::spawn(async move {
            while let Some(samples) = audio_rx.recv().await {
                // Convert f32 → i16 PCM (24 kHz expected by API; caller must resample if needed)
                let pcm16: Vec<u8> = samples
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
                }
            }

            // Commit any remaining audio when the channel closes (recording stopped)
            let commit = ClientEvent::InputAudioBufferCommit {};
            if let Ok(json) = serde_json::to_string(&commit) {
                let _ = write.send(Message::Text(json)).await;
            }
            let _ = write.close().await;
        });

        Ok(audio_tx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_config_serialization() {
        let config = ClientEvent::TranscriptionSessionUpdate {
            session: TranscriptionSessionConfig {
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
        assert!(json.contains("transcription_session.update"));
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

