//! Compact HyperHLE-style UI shell

mod app_picker;
mod boot_animation;
mod file_explorer;
mod loading_screen;
mod terminal;

pub use app_picker::{AppInfo, AppPicker};
pub use boot_animation::BootAnimation;
pub use file_explorer::FileExplorer;
pub use loading_screen::LoadingScreen;
pub use terminal::TerminalPanel;

use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub enum EmulatorState {
    AppPicker,
    BootAnimation { apk_name: String },
    ApkLoading { apk_name: String },
    GameRunning { apk_name: String },
}

impl Default for EmulatorState {
    fn default() -> Self {
        EmulatorState::AppPicker
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFilterLevel {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone)]
pub struct EmulatorSettings {
    /// Android API level target (16 = 4.1 … 25 = 7.1)
    pub target_api: i32,
    pub log_level: LogFilterLevel,
    pub active_engine: String,
    /// GLES thread-guard optimizer (serializes GL calls)
    pub gles_thread_guard: bool,
    pub show_settings: bool,
}

impl Default for EmulatorSettings {
    fn default() -> Self {
        Self {
            target_api: 25,
            log_level: LogFilterLevel::Info,
            active_engine: "auto".into(),
            gles_thread_guard: true,
            show_settings: false,
        }
    }
}

#[derive(Debug)]
pub struct UiSharedState {
    pub state: EmulatorState,
    pub loading_progress: f32,
    pub loading_status: String,
    pub available_apks: Vec<ApkEntry>,
    pub selected_apk: Option<String>,
    pub terminal_open: bool,
    pub picker_page: usize,
    pub explorer: FileExplorer,
    pub show_explorer: bool,
    pub settings: EmulatorSettings,
    /// Android SAF: watch apps/ for a newly imported package
    pub pending_saf: bool,
    pub saf_started_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ApkEntry {
    pub path: String,
    pub name: String,
    pub size_mb: f32,
}

impl Default for UiSharedState {
    fn default() -> Self {
        Self {
            state: EmulatorState::AppPicker,
            loading_progress: 0.0,
            loading_status: String::new(),
            available_apks: Vec::new(),
            selected_apk: None,
            terminal_open: false,
            picker_page: 0,
            explorer: FileExplorer::default(),
            show_explorer: true,
            settings: EmulatorSettings::default(),
            pending_saf: false,
            saf_started_ms: 0,
        }
    }
}

pub type SharedUi = Arc<Mutex<UiSharedState>>;

pub fn create_shared_ui() -> SharedUi {
    Arc::new(Mutex::new(UiSharedState::default()))
}
