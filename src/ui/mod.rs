//! State-driven GUI for Pixel1HLE
//! AppPicker → BootAnimation → ApkLoading → GameRunning + Terminal

mod app_picker;
mod boot_animation;
mod loading_screen;
mod terminal;

pub use app_picker::AppPicker;
pub use boot_animation::BootAnimation;
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

#[derive(Debug)]
pub struct UiSharedState {
    pub state: EmulatorState,
    pub loading_progress: f32,
    pub loading_status: String,
    pub available_apks: Vec<ApkEntry>,
    pub selected_apk: Option<String>,
    /// Terminal panel visible
    pub terminal_open: bool,
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
            terminal_open: true, // open by default so user sees logs
        }
    }
}

pub type SharedUi = Arc<Mutex<UiSharedState>>;

pub fn create_shared_ui() -> SharedUi {
    Arc::new(Mutex::new(UiSharedState::default()))
}
