//! File + in-memory logging for GUI terminal and crash dumps

use log::{Level, LevelFilter, Log, Metadata, Record};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

const RING_CAPACITY: usize = 2000;

#[derive(Clone)]
pub struct LogLine {
    pub level: Level,
    pub target: String,
    pub message: String,
    pub timestamp: String,
}

pub struct SharedState {
    pub ring: Vec<LogLine>,
    file: Option<File>,
    log_dir: PathBuf,
}

pub type LogState = Arc<Mutex<SharedState>>;

static LOG_STATE: OnceCell<LogState> = OnceCell::new();

pub fn global_log_state() -> Option<&'static LogState> {
    LOG_STATE.get()
}

struct HleLogger {
    state: LogState,
    min_level: LevelFilter,
}

impl HleLogger {
    pub fn init(log_dir: impl Into<PathBuf>) -> LogState {
        let log_dir = log_dir.into();
        let _ = fs::create_dir_all(&log_dir);

        let log_path = log_dir.join("pixel1hle.log");
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok();

        let state = Arc::new(Mutex::new(SharedState {
            ring: Vec::with_capacity(RING_CAPACITY),
            file,
            log_dir: log_dir.clone(),
        }));

        let _ = LOG_STATE.set(state.clone());

        let logger = HleLogger {
            state: state.clone(),
            min_level: LevelFilter::Info,
        };

        let _ = log::set_boxed_logger(Box::new(logger));
        log::set_max_level(LevelFilter::Info);

        let crash_dir = log_dir.clone();
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let ts = timestamp();
            let path = crash_dir.join(format!("crash_{ts}.txt"));
            let mut body = String::new();
            body.push_str("=== Pixel1HLE CRASH REPORT ===\n");
            body.push_str(&format!("time: {ts}\n"));
            body.push_str(&format!("payload: {info}\n"));
            if let Some(loc) = info.location() {
                body.push_str(&format!(
                    "location: {}:{}:{}\n",
                    loc.file(),
                    loc.line(),
                    loc.column()
                ));
            }
            body.push_str("\n--- hint ---\n");
            body.push_str("Full trail: logs/pixel1hle.log\n");
            body.push_str("Open the in-app Terminal (key `) before the crash if possible.\n");
            let _ = fs::write(&path, &body);
            eprintln!("CRASH written to {}", path.display());
            prev(info);
        }));

        log::info!(target: "hle::log", "Logger ready → {}", log_path.display());
        state
    }
}

// Re-export init through module
pub fn init(log_dir: impl Into<PathBuf>) -> LogState {
    HleLogger::init(log_dir)
}

impl Log for HleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.min_level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let line = LogLine {
            level: record.level(),
            target: record.target().to_string(),
            message: format!("{}", record.args()),
            timestamp: timestamp(),
        };

        let formatted = format!(
            "[{}] {:>5} {}: {}\n",
            line.timestamp, line.level, line.target, line.message
        );

        // Still print so launching from a console works
        eprint!("{formatted}");

        let mut g = self.state.lock();
        if let Some(ref mut f) = g.file {
            let _ = f.write_all(formatted.as_bytes());
            let _ = f.flush();
        }
        if g.ring.len() >= RING_CAPACITY {
            g.ring.remove(0);
        }
        g.ring.push(line);
    }

    fn flush(&self) {
        if let Some(ref mut f) = self.state.lock().file {
            let _ = f.flush();
        }
    }
}

fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

pub fn snapshot_lines(state: &LogState) -> Vec<LogLine> {
    state.lock().ring.clone()
}
