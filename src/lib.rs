//! pixel1-translator – High-Level Emulation layer for Google Pixel 1 (sailfish)
//!
//! Full cascade:
//!
//!   APK (lib/arm64-v8a/libunity.so)
//!        │
//!        ▼
//!   ApkElfExecutor  (extract → PT_LOAD map → RELA patch → JNI_OnLoad)
//!        │
//!        ▼
//!   GLES Bridge (extern "C" egl*/gl* → glow)
//!        │
//!        ▼
//!   HardwareStack + Binder + SyscallTranslator

pub mod binder;
pub mod cpu;
pub mod error;
pub mod hardware;
pub mod syscall;

pub use binder::BinderRuntime;
pub use cpu::{ApkElfExecutor, CpuRuntime, LoadedLibrary};
pub use error::{Pixel1Error, Result};
pub use hardware::HardwareStack;
pub use syscall::SyscallTranslator;

use log::info;
use std::path::Path;
use std::sync::Arc;

/// Top-level runtime that owns every subsystem
pub struct Pixel1Translator {
    pub cpu: Arc<CpuRuntime>,
    pub binder: Arc<BinderRuntime>,
    pub syscall: Arc<SyscallTranslator>,
    pub hw: Arc<HardwareStack>,
}

impl Pixel1Translator {
    pub fn new(profile_path: &str) -> Result<Self> {
        info!("Pixel1Translator: booting sailfish HLE + GLES + dynamic linker");

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

    /// Full APK → native library execution path
    ///
    /// 1. Open the APK
    /// 2. Extract lib/arm64-v8a/<engine>.so
    /// 3. Map PT_LOAD, apply relocations, hook EGL/GLES symbols
    /// 4. Call JNI_OnLoad with a mocked JavaVM
    pub fn load_and_run_apk(&self, apk_path: &Path, engine_soname: &str) -> Result<Arc<LoadedLibrary>> {
        info!(target: "hle::linker", "=== APK execution cascade: {} ===", apk_path.display());

        let executor = ApkElfExecutor::new(apk_path);

        // Discover what is inside
        let libs = executor.list_native_libs()?;
        info!(target: "hle::linker", "Native libs: {:?}", libs);

        // Load the engine (libunity.so, libminecraftpe.so, …)
        let lib = executor.bootstrap_engine(engine_soname)?;

        // Keep the executor alive inside the CPU runtime
        self.cpu.attach_linker(executor);

        info!(target: "hle::linker", "Engine {} is live @ guest 0x{:x}", engine_soname, lib.guest_base);
        Ok(lib)
    }

    /// Convenience demo that only exercises the GLES path (no real APK needed)
    pub fn demo_unity_graphics(&self) {
        self.cpu.demo_unity_graphics_bootstrap();
    }

    pub fn example_cascade_get_window_service(&self) -> Result<Option<u32>> {
        self.binder.get_system_service("WINDOW_SERVICE")
    }
}
