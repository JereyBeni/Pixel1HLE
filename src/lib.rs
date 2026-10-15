//! pixel1-translator – Full HLE with UI (desktop + Android NativeActivity)
//! Engines: Unity, Minecraft PE, Adobe AIR (`libcom_adobe_air.so`)

pub mod binder;
pub mod cpu;
pub mod error;
pub mod hardware;
pub mod logging;
pub mod platform;
pub mod profile_embed;
pub mod syscall;
pub mod ui;

pub use binder::BinderRuntime;
pub use cpu::{ApkElfExecutor, CpuRuntime, LoadedLibrary};
pub use error::{Pixel1Error, Result};
pub use hardware::HardwareStack;
pub use syscall::SyscallTranslator;
pub use ui::{EmulatorState, SharedUi};

use log::info;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
struct RootConfig {
    hardware: hardware::HardwareProfile,
    #[serde(default)]
    permissions: PermissionsCfg,
}

#[derive(Debug, Deserialize, Default)]
struct PermissionsCfg {
    #[serde(default)]
    allowlist: Vec<String>,
}

pub struct Pixel1Translator {
    pub cpu: Arc<CpuRuntime>,
    pub binder: Arc<BinderRuntime>,
    pub syscall: Arc<SyscallTranslator>,
    pub hw: Arc<HardwareStack>,
}

impl Pixel1Translator {
    pub fn new(profile_path: &str) -> Result<Self> {
        info!(target: "hle", "Pixel1Translator: booting sailfish HLE + UI");
        let _ = platform::ensure_sandbox_dirs();
        let profile_str = std::fs::read_to_string(profile_path).unwrap_or_else(|_| {
            info!(target: "hle", "Profile file missing – using embedded sailfish defaults");
            profile_embed::SAILFISH_YAML.to_string()
        });
        Self::from_yaml(&profile_str)
    }

    pub fn from_yaml(profile_str: &str) -> Result<Self> {
        let _ = platform::ensure_sandbox_dirs();
        let root: RootConfig = serde_yaml::from_str(profile_str)
            .map_err(|e| Pixel1Error::Config(e.to_string()))?;

        let hw = HardwareStack::from_profile(root.hardware)?;
        hw.init_display()?;
        let _ = hw.init_audio();

        let cpu = Arc::new(CpuRuntime::new());
        let syscall = Arc::new(SyscallTranslator::new());
        hardware::gles_bridge::register_gles_symbols(&cpu);

        let allowlist = if root.permissions.allowlist.is_empty() {
            vec![
                "android.permission.INTERNET".into(),
                "android.permission.ACCESS_NETWORK_STATE".into(),
                "android.permission.WAKE_LOCK".into(),
                "android.permission.VIBRATE".into(),
            ]
        } else {
            root.permissions.allowlist
        };

        let binder = Arc::new(BinderRuntime::new(hw.clone(), allowlist));

        Ok(Self {
            cpu,
            binder,
            syscall,
            hw,
        })
    }

    pub fn run_ui(self) -> Result<()> {
        self.run_ui_with_event_loop(
            #[cfg(target_os = "android")]
            None,
            #[cfg(not(target_os = "android"))]
            None,
        )
    }

    pub fn run_ui_with_event_loop(
        self,
        #[cfg(target_os = "android")] android_app: Option<android_activity::AndroidApp>,
        #[cfg(not(target_os = "android"))] _android_app: Option<()>,
    ) -> Result<()> {
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
                    info!(target: "hle::display", "Background loader for {name}");
                    let executor = ApkElfExecutor::new(&path).with_ui(ui_clone.clone());

                    let result = executor.bootstrap_any_engine().or_else(|_| {
                        let mut last_err = None;
                        for eng in cpu::linker::ENGINE_CANDIDATES {
                            match executor.bootstrap_engine(eng) {
                                Ok(lib) => return Ok(lib),
                                Err(e) => last_err = Some(e),
                            }
                        }
                        Err(last_err.unwrap_or_else(|| Pixel1Error::ElfLoad("no engine".into())))
                    });

                    match result {
                        Ok(lib) => {
                            info!(
                                target: "hle::linker",
                                "Loaded {} @ 0x{:x} (AIR={})",
                                lib.soname,
                                lib.guest_base,
                                lib.is_adobe_air
                            );
                            cpu_clone.attach_linker(executor);
                            hardware::display::DisplaySurface::set_state(
                                &ui_clone,
                                EmulatorState::GameRunning {
                                    apk_name: name.clone(),
                                },
                            );
                        }
                        Err(e) => {
                            log::warn!(target: "hle::linker", "Engine load failed: {e}");
                            hardware::display::DisplaySurface::update_loading_progress(
                                &ui_clone,
                                1.0,
                                &format!("No engine – demo ({e})"),
                            );
                            std::thread::sleep(std::time::Duration::from_millis(800));
                            hardware::display::DisplaySurface::set_state(
                                &ui_clone,
                                EmulatorState::GameRunning { apk_name: name },
                            );
                        }
                    }
                    break;
                }
            }
        });

        #[cfg(target_os = "android")]
        {
            hardware::display::run_ui_loop_android(ui_shared, width, height, android_app)
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = _android_app;
            hardware::display::run_ui_loop(ui_shared, width, height)
        }
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
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("Pixel1HLE"),
    );

    log::info!(target: "hle", "android_main – pure Rust NativeActivity");
    let _ = platform::ensure_sandbox_dirs();

    match Pixel1Translator::from_yaml(profile_embed::SAILFISH_YAML) {
        Ok(rt) => {
            if let Err(e) = rt.run_ui_with_event_loop(Some(app)) {
                log::error!(target: "hle", "UI failed: {e}");
            }
        }
        Err(e) => log::error!(target: "hle", "Boot failed: {e}"),
    }
}
