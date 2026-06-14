//! Transcription history — a small, disk-backed list of past transcriptions.

use anyhow::{Context, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const HISTORY_FILENAME: &str = "history.json";
/// Keep the history bounded so the file never grows without limit.
const MAX_ENTRIES: usize = 100;

/// A single past transcription shown in the history list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionEntry {
    /// Unique id — creation time in milliseconds since the epoch.
    pub id: u64,
    /// The transcribed text.
    pub text: String,
    /// BCP-47 language code ("en" | "he").
    pub language: String,
    /// Creation time in seconds since the epoch.
    pub timestamp: i64,
}

/// Thread-safe, disk-backed transcription history (most recent first).
#[derive(Clone)]
pub struct HistoryStore {
    entries: Arc<RwLock<Vec<TranscriptionEntry>>>,
    path: PathBuf,
}

impl HistoryStore {
    /// Create the store, loading any existing history from
    /// `config_dir/history.json`. A missing or unreadable file yields an empty
    /// history rather than failing startup.
    pub fn new(config_dir: PathBuf) -> Self {
        let path = config_dir.join(HISTORY_FILENAME);
        let entries = Self::load(&path).unwrap_or_else(|e| {
            log::warn!("Failed to load transcription history, starting empty: {}", e);
            Vec::new()
        });
        log::info!("History store init: {} entries from {:?}", entries.len(), path);
        Self {
            entries: Arc::new(RwLock::new(entries)),
            path,
        }
    }

    fn load(path: &PathBuf) -> Result<Vec<TranscriptionEntry>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let contents =
            fs::read_to_string(path).with_context(|| format!("Failed to read history: {:?}", path))?;
        serde_json::from_str(&contents).context("Failed to parse history JSON")
    }

    fn persist(&self) {
        let entries = self.entries.read();
        match serde_json::to_string_pretty(&*entries) {
            Ok(json) => {
                if let Err(e) = fs::write(&self.path, json) {
                    log::warn!("Failed to persist history: {}", e);
                }
            }
            Err(e) => log::warn!("Failed to serialize history: {}", e),
        }
    }

    /// Prepend a new entry, cap the list to `MAX_ENTRIES`, persist, and return
    /// the stored entry (so callers can broadcast it to the UI).
    pub fn add(&self, text: &str, language: &str) -> TranscriptionEntry {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let entry = TranscriptionEntry {
            id: now.as_millis() as u64,
            text: text.to_string(),
            language: language.to_string(),
            timestamp: now.as_secs() as i64,
        };
        {
            let mut entries = self.entries.write();
            entries.insert(0, entry.clone());
            entries.truncate(MAX_ENTRIES);
        }
        self.persist();
        entry
    }

    /// All entries, most recent first.
    pub fn all(&self) -> Vec<TranscriptionEntry> {
        self.entries.read().clone()
    }

    /// Remove a single entry by id.
    pub fn delete(&self, id: u64) {
        self.entries.write().retain(|e| e.id != id);
        self.persist();
    }

    /// Remove all entries.
    pub fn clear(&self) {
        self.entries.write().clear();
        self.persist();
    }
}
