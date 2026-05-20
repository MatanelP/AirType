//! In-memory log capture for in-app log viewer.
//!
//! Installs a custom `log::Log` implementation that writes to a ring buffer
//! alongside the normal env_logger output. The buffer is capped at MAX_ENTRIES
//! to avoid unbounded memory growth.

use log::{Log, Metadata, Record};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ENTRIES: usize = 500;

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: u64, // Unix ms
    pub level: String,
    pub target: String,
    pub message: String,
}

/// Global ring-buffer of captured log entries.
static BUFFER: OnceLock<Arc<Mutex<VecDeque<LogEntry>>>> = OnceLock::new();

fn buffer() -> &'static Arc<Mutex<VecDeque<LogEntry>>> {
    BUFFER.get_or_init(|| Arc::new(Mutex::new(VecDeque::with_capacity(MAX_ENTRIES))))
}

/// Combined logger: forwards to env_logger and captures into the ring buffer.
struct CaptureLogger {
    inner: env_logger::Logger,
}

impl Log for CaptureLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        if self.inner.enabled(record.metadata()) {
            // Forward to env_logger (writes to stderr)
            self.inner.log(record);

            // Capture into ring buffer
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            let entry = LogEntry {
                timestamp: ts,
                level: record.level().to_string(),
                target: record.target().to_string(),
                message: record.args().to_string(),
            };

            let mut buf = buffer().lock();
            if buf.len() >= MAX_ENTRIES {
                buf.pop_front();
            }
            buf.push_back(entry);
        }
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

/// Initialise the capture logger. Call once at app startup instead of `env_logger::init()`.
pub fn init(filter: &str) {
    let inner = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(filter),
    )
    .build();

    let max_level = inner.filter();
    let logger = CaptureLogger { inner };

    // `set_boxed_logger` takes ownership; ignore "already set" errors on hot-reload.
    let _ = log::set_boxed_logger(Box::new(logger));
    log::set_max_level(max_level);
}

/// Return a snapshot of all captured log entries (oldest first).
pub fn get_entries() -> Vec<LogEntry> {
    buffer().lock().iter().cloned().collect()
}

/// Clear all captured entries.
pub fn clear() {
    buffer().lock().clear();
}
