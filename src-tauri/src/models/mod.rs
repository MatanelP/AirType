//! Model management module for AirType
//!
//! Handles downloading and managing Whisper models.

use crate::settings::{ModelSize, SettingsStore};
use futures_util::StreamExt;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

/// Base URL for Whisper model downloads
const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Hebrew-optimized model URL (ivrit-ai turbo — faster, ~1.6GB)
const HEBREW_MODEL_URL: &str = "https://huggingface.co/ivrit-ai/whisper-large-v3-turbo-ggml/resolve/main/ggml-model.bin";
const HEBREW_MODEL_FILENAME: &str = "ggml-ivrit-large-v3-turbo.bin";

/// Get the filename for a model size
pub fn model_filename(size: ModelSize) -> &'static str {
    match size {
        ModelSize::Tiny => "ggml-tiny.bin",
        ModelSize::Base => "ggml-base.bin",
        ModelSize::Small => "ggml-small.bin",
        ModelSize::Medium => "ggml-medium.bin",
        ModelSize::Large => "ggml-large-v3.bin",
    }
}

/// Get the download URL for a model size
pub fn model_url(size: ModelSize) -> String {
    format!("{}/{}", MODEL_BASE_URL, model_filename(size))
}

/// Get the local path for a model
pub fn model_path(size: ModelSize) -> PathBuf {
    SettingsStore::get_models_dir().join(model_filename(size))
}

/// Get the local path for the Hebrew model
pub fn hebrew_model_path() -> PathBuf {
    SettingsStore::get_models_dir().join(HEBREW_MODEL_FILENAME)
}

/// Check if a model exists locally
pub fn model_exists(size: ModelSize) -> bool {
    model_path(size).exists()
}

/// Check if the Hebrew model exists locally
pub fn hebrew_model_exists() -> bool {
    hebrew_model_path().exists()
}

/// Get approximate model size in MB for display
pub fn model_size_mb(size: ModelSize) -> u64 {
    match size {
        ModelSize::Tiny => 75,
        ModelSize::Base => 142,
        ModelSize::Small => 466,
        ModelSize::Medium => 1500,
        ModelSize::Large => 3000,
    }
}

/// Get Hebrew model size in MB
pub fn hebrew_model_size_mb() -> u64 {
    1625 // ~1.6GB for turbo GGML version
}

/// Download a model with progress reporting
pub async fn download_model(
    size: ModelSize,
    on_progress: Option<impl Fn(u64, u64) + Send + 'static>,
) -> Result<PathBuf, String> {
    let url = model_url(size);
    let path = model_path(size);
    download_from_url(&url, &path, on_progress).await
}

/// Download the Hebrew-optimized model
pub async fn download_hebrew_model(
    on_progress: Option<impl Fn(u64, u64) + Send + 'static>,
) -> Result<PathBuf, String> {
    let path = hebrew_model_path();
    download_from_url(HEBREW_MODEL_URL, &path, on_progress).await
}

/// Generic download function
async fn download_from_url(
    url: &str,
    path: &PathBuf,
    on_progress: Option<impl Fn(u64, u64) + Send + 'static>,
) -> Result<PathBuf, String> {
    // Ensure models directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create models directory: {}", e))?;
    }

    log::info!("Downloading model from {}", url);

    // Start download
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Failed to start download: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed with status: {}", response.status()));
    }

    let total_size = response.content_length().unwrap_or(0);

    // Create temp file for download
    let temp_path = path.with_extension("bin.tmp");
    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Write error: {}", e))?;

        downloaded += chunk.len() as u64;

        if let Some(ref callback) = on_progress {
            callback(downloaded, total_size);
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Flush error: {}", e))?;

    // Rename temp file to final path
    tokio::fs::rename(&temp_path, &path)
        .await
        .map_err(|e| format!("Failed to finalize download: {}", e))?;

    log::info!("Model download complete: {}", path.display());
    Ok(path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_filename() {
        assert_eq!(model_filename(ModelSize::Tiny), "ggml-tiny.bin");
        assert_eq!(model_filename(ModelSize::Base), "ggml-base.bin");
        assert_eq!(model_filename(ModelSize::Small), "ggml-small.bin");
        assert_eq!(model_filename(ModelSize::Medium), "ggml-medium.bin");
    }

    #[test]
    fn test_model_url() {
        let url = model_url(ModelSize::Base);
        assert!(url.contains("ggml-base.bin"));
        assert!(url.starts_with("https://"));
    }

    #[test]
    fn test_model_path() {
        let path = model_path(ModelSize::Base);
        assert!(path.to_string_lossy().contains("ggml-base.bin"));
    }
    
    #[test]
    fn test_hebrew_model_path() {
        let path = hebrew_model_path();
        assert!(path.to_string_lossy().contains("ggml-ivrit"));
    }
}
