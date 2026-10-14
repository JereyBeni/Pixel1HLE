//! pixel1-translator – High-Level Emulation layer for Google Pixel 1 (sailfish)
//!
//! Architecture cascade (guest → host):
//!
//!   Android App / libunity.so (ARM64)
//!        │
//!        ▼
//!   ┌─────────────────────────────────────────────┐
//!   │  BinderRuntime  (getSystemService …)        │
//!   └────────────────────┬────────────────────────┘
//!                        │
//!                        ▼
//!   ┌─────────────────────────────────────────────┐
//!   │  HardwareStack + GLES Bridge                │
//!   │  (egl* / gl* extern "C" → glow::Context)    │
//!   └────────────────────┬────────────────────────┘
//!                        │
//!                        ▼
//!   ┌─────────────────────────────────────────────┐
//!   │  SyscallTranslator  (bionic → host)         │
//!   └────────────────────┬────────────────────────┘
//!                        │
//!                        ▼
//!   ┌─────────────────────────────────────────────┐
//!   │  CpuRuntime  (ELF + JNI hooks)              │
//!   └─────────────────────────────────────────────┘

pub mod binder;
pub mod cpu;
pub mod error;
pub mod hardware;
pub mod syscall;

pub use binder::BinderRuntime;
pub use cpu::CpuRuntime;
pub use error::{Pixel1Error, Result};
pub use hardware::HardwareStack;
pub use syscall::SyscallTranslator;

use log::info;
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
        info!("Pixel1Translator: booting sailfish HLE + GLES bridge");

        let profile_str = std::fs::read_to_string(profile_path)
            .map_err(|e| Pixel1Error::Config(e.to_string()))?;
        let profile: hardware::HardwareProfile = serde_yaml::from_str(&profile_str)
            .map_err(|e| Pixel1Error::Config(e.to_string()))?;

        let hw = HardwareStack::from_profile(profile)?;
        hw.init_display()?;
        hw.init_audio()?;

        let cpu = Arc::new(CpuRuntime::new());
        let syscall = Arc::new(SyscallTranslator::new());

        // Register the GLES/EGL symbols so any future ELF load of libGLESv2.so
        // can be redirected straight into our extern "C" table.
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

    /// Full Unity-style graphics bootstrap demonstration
    pub fn demo_unity_graphics(&self) {
        self.cpu.demo_unity_graphics_bootstrap();
    }

    pub fn example_cascade_get_window_service(&self) -> Result<Option<u32>> {
        self.binder.get_system_service("WINDOW_SERVICE")
    }
}
