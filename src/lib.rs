//! pixel1-translator – Full HLE with UI state machine + terminal

pub mod binder;
pub mod cpu;
pub mod error;
pub mod hardware;
pub mod logging;
pub mod syscall;
pub mod ui;

pub use binder::BinderRuntime;
pub use cpu::{ApkElfExecutor, CpuRuntime, LoadedLibrary};
pub use error::{Pixel1Error, Result};
pub use hardware::HardwareStack;
pub use syscall::SyscallTranslator;
pub use ui::{EmulatorState, SharedUi};

use log::info;
use std::path::Path;
use std::sync::Arc;

pub struct Pixel1Translator {
    pub cpu: Arc<CpuRuntime>,
    pub binder: Arc<BinderRuntime>,
    pub syscall: Arc<SyscallTranslator>,
    pub hw: Arc<HardwareStack>,
}

impl Pixel1Translator {
    pub fn new(profile_path: &str) -> Result<Self> {
        info!(target: "hle", "Pixel1Translator: booting sailfish HLE + UI");

        let profile_str = std::fs::read_to_string(profile_path)
            .map_err(|e| Pixel1Error::Config(e.to_string()))?;
        let profile: hardware::HardwareProfile = serde_yaml::from_str(&profile_str)
            .map_err(|e| Pixel1Error::Config(e.to_string()))?;

        let hw = HardwareStack::from_profile(profile)?;
        hw.init_display()?;
        hw.init_audio()?;

        let cpu = Arc::new(CpuRuntime::new());
        let syscall = Arc::new(SyscallTranslator::new());

        hardware::gles_bridge::register_gles_symbols(&cpu);

        let allowlist = vec![
            "android.permission.INTERNET".into(),
            "android.permission.ACCESS_NETWORK_STATE".into(),
            "android.permission.WAKE_LOCK".into(),
            "android.permission.VIBRATE".into(),
        ];

        let binder = Arc::new(BinderRuntime::new(hw.clone(), allowlist));

        Ok(Self {
            cpu,
            binder,
            syscall,
            hw,
        })
    }

    pub fn run_ui(self) -> Result<()> {
        let width = self.hw.profile.display.width;
        let height = self.hw.profile.display.height;
        let ui_shared = self.hw.ui_shared.clone();

        let ui_clone = ui_shared.clone();
        let cpu_clone = self.cpu.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let (should_load, path, name) = {
                    let g = ui_clone.lock();
                    match &g.state {
                        EmulatorState::ApkLoading { apk_name } => {
                            if let Some(p) = &g.selected_apk {
                                (true, p.clone(), apk_name.clone())
                            } else {
                                (false, String::new(), String::new())
                            }
                        }
                        _ => (false, String::new(), String::new()),
                    }
                };

                if should_load {
                    info!(target: "hle::display", "Background loader starting for {name}");
                    let executor = ApkElfExecutor::new(&path).with_ui(ui_clone.clone());
                    let candidates = ["libunity.so", "libminecraftpe.so", "libmain.so", "libgame.so"];
                    let mut loaded = false;
                    for eng in candidates {
                        match executor.bootstrap_engine(eng) {
                            Ok(lib) => {
                                info!(target: "hle::linker", "Loaded {eng} @ 0x{:x}", lib.guest_base);
                                cpu_clone.attach_linker(executor);
                                hardware::display::DisplaySurface::set_state(
                                    &ui_clone,
                                    EmulatorState::GameRunning {
                                        apk_name: name.clone(),
                                    },
                                );
                                loaded = true;
                                break;
                            }
                            Err(e) => {
                                log::debug!(target: "hle::linker", "{eng} not found: {e}");
                            }
                        }
                    }
                    if !loaded {
                        hardware::display::DisplaySurface::update_loading_progress(
                            &ui_clone,
                            1.0,
                            "No native engine found – demo mode",
                        );
                        std::thread::sleep(std::time::Duration::from_millis(800));
                        hardware::display::DisplaySurface::set_state(
                            &ui_clone,
                            EmulatorState::GameRunning { apk_name: name },
                        );
                    }
                    break;
                }
            }
        });

        // Pass log state into the UI loop via a side channel file path;
        // the terminal reads from the global logger ring through logging::snapshot.
        // We store a handle on the UI shared state indirectly via static once the logger is init.
        hardware::display::run_ui_loop(ui_shared, width, height)
    }

    pub fn load_and_run_apk(
        &self,
        apk_path: &Path,
        engine_soname: &str,
    ) -> Result<Arc<LoadedLibrary>> {
        let executor = ApkElfExecutor::new(apk_path).with_ui(self.hw.ui_shared.clone());
        let lib = executor.bootstrap_engine(engine_soname)?;
        self.cpu.attach_linker(executor);
        Ok(lib)
    }

    pub fn demo_unity_graphics(&self) {
        self.cpu.demo_unity_graphics_bootstrap();
    }

    pub fn example_cascade_get_window_service(&self) -> Result<Option<u32>> {
        self.binder.get_system_service("WINDOW_SERVICE")
    }
}
