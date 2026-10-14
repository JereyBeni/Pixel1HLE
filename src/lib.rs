//! pixel1-translator – High-Level Emulation layer for Google Pixel 1 (sailfish)
//!
//! Architecture cascade (guest → host):
//!
//!   Android App (ARM64 .so + Java)
//!        │
//!        ▼
//!   ┌─────────────────────────────────────────────┐
//!   │  BinderRuntime  (getSystemService, transact)│
//!   │         │                                    │
//!   │         ▼                                    │
//!   │  HardwareStack  (hasSystemFeature, mocks)   │
//!   └─────────────────────────────────────────────┘
//!        │
//!        ▼
//!   ┌─────────────────────────────────────────────┐
//!   │  SyscallTranslator  (bionic → host POSIX)   │
//!   └─────────────────────────────────────────────┘
//!        │
//!        ▼
//!   ┌─────────────────────────────────────────────┐
//!   │  CpuRuntime  (ELF loader + JNI hooks)       │
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
        info!("Pixel1Translator: booting sailfish HLE");

        // Load profile
        let profile_str = std::fs::read_to_string(profile_path)
            .map_err(|e| Pixel1Error::Config(e.to_string()))?;
        let profile: hardware::HardwareProfile = serde_yaml::from_str(&profile_str)
            .map_err(|e| Pixel1Error::Config(e.to_string()))?;

        let hw = HardwareStack::from_profile(profile)?;
        hw.init_display()?;
        hw.init_audio()?;

        let cpu = Arc::new(CpuRuntime::new());
        let syscall = Arc::new(SyscallTranslator::new());

        // allowlist from the yaml (simplified – real code would parse the full document)
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

    /// Example of a full cascade: app calls getSystemService("WINDOW_SERVICE")
    pub fn example_cascade_get_window_service(&self) -> Result<Option<u32>> {
        // 1. Binder layer
        let handle = self.binder.get_system_service("WINDOW_SERVICE")?;

        // 2. If we got a handle, any subsequent transact() would go through Binder
        //    and eventually touch the DisplaySurface inside HardwareStack.

        // 3. If the app later does a syscall (e.g. mmap for a graphic buffer),
        //    it hits SyscallTranslator which may talk back to CpuRuntime.

        Ok(handle)
    }
}
