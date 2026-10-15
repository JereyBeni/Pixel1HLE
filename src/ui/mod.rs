//! State-driven GUI – compact HyperHLE shell

mod app_picker;
mod boot_animation;
mod file_explorer;
mod loading_screen;
mod terminal;

pub use app_picker::AppPicker;
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

#[derive(Debug, Clone)]
pub struct EmulatorSettings {
    pub target_api: i32,
    pub log_level: LogFilterLevel,
    pub active_engine: String,
    pub show_settings: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFilterLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl Default for EmulatorSettings {
    fn default() -> Self {
        Self {
            target_api: 25,
            log_level: LogFilterLevel::Info,
            active_engine: "auto".into(),
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
        }
    }
}

pub type SharedUi = Arc<Mutex<UiSharedState>>;

pub fn create_shared_ui() -> SharedUi {
    Arc::new(Mutex::new(UiSharedState::default()))
}
