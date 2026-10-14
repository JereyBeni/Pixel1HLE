//! Complete Hardware / Software Stack for sailfish
//! Display + UI state machine, Touch, Audio, Storage, Battery + GLES HLE

mod audio;
mod battery;
mod display;
mod gles_bridge;
mod storage;

pub use audio::AudioPipeline;
pub use battery::BatteryMock;
pub use display::{self, DisplaySurface};
pub use gles_bridge;
pub use storage::SparseStorage;

use crate::error::Result;
use crate::ui::{self, SharedUi};
use log::info;
use parking_lot::RwLock;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct HardwareProfile {
    pub display: DisplayCfg,
    pub touch: TouchCfg,
    pub audio: AudioCfg,
    pub storage: StorageCfg,
    pub battery: BatteryCfg,
    pub camera: FeatureCfg,
    pub fingerprint: FeatureCfg,
    pub nfc: FeatureCfg,
    pub bluetooth: FeatureCfg,
    pub wifi: FeatureCfg,
    pub gps: FeatureCfg,
}

#[derive(Debug, Deserialize)]
pub struct DisplayCfg {
    pub width: u32,
    pub height: u32,
    pub ppi: u32,
    pub refresh_hz: u32,
    pub present: bool,
}

#[derive(Debug, Deserialize)]
pub struct TouchCfg {
    pub multi_touch: bool,
    pub max_pointers: u32,
    pub present: bool,
}

#[derive(Debug, Deserialize)]
pub struct AudioCfg {
    pub present: bool,
    pub jack_3_5mm: bool,
    pub speaker: bool,
}

#[derive(Debug, Deserialize)]
pub struct StorageCfg {
    pub internal_gb: u32,
    pub present: bool,
}

#[derive(Debug, Deserialize)]
pub struct BatteryCfg {
    pub present: bool,
    pub capacity_mah: u32,
    pub force_full_and_charging: bool,
}

#[derive(Debug, Deserialize)]
pub struct FeatureCfg {
    pub present: bool,
}

pub struct HardwareStack {
    pub profile: HardwareProfile,
    pub display: RwLock<Option<DisplaySurface>>,
    pub audio: RwLock<Option<AudioPipeline>>,
    pub storage: SparseStorage,
    pub battery: BatteryMock,
    pub ui_shared: SharedUi,
}

impl HardwareStack {
    pub fn from_profile(profile: HardwareProfile) -> Result<Arc<Self>> {
        info!("Hardware: building sailfish stack (UI + GLES HLE)");

        let storage = SparseStorage::new(profile.storage.internal_gb)?;
        let battery = BatteryMock::new(profile.battery.force_full_and_charging);
        let ui_shared = ui::create_shared_ui();

        let stack = Arc::new(Self {
            profile,
            display: RwLock::new(None),
            audio: RwLock::new(None),
            storage,
            battery,
            ui_shared,
        });

        Ok(stack)
    }

    pub fn has_feature(&self, feature: &str) -> bool {
        match feature {
            "android.hardware.camera"
            | "android.hardware.camera.any"
            | "android.hardware.camera.autofocus" => self.profile.camera.present,
            "android.hardware.fingerprint" => self.profile.fingerprint.present,
            "android.hardware.nfc" => self.profile.nfc.present,
            "android.hardware.bluetooth" => self.profile.bluetooth.present,
            "android.hardware.wifi" => self.profile.wifi.present,
            "android.hardware.location"
            | "android.hardware.location.gps" => self.profile.gps.present,
            "android.hardware.touchscreen"
            | "android.hardware.touchscreen.multitouch" => self.profile.touch.present,
            "android.hardware.audio.output" => self.profile.audio.present,
            "android.hardware.screen.landscape"
            | "android.hardware.screen.portrait" => self.profile.display.present,
            "android.hardware.opengles.aep"
            | "android.software.opengles.deqp.level" => true,
            _ => false,
        }
    }

    pub fn init_display(&self) -> Result<()> {
        if !self.profile.display.present {
            return Ok(());
        }
        let surface = DisplaySurface::new(
            self.profile.display.width,
            self.profile.display.height,
            self.profile.display.refresh_hz,
            self.ui_shared.clone(),
        )?;
        *self.display.write() = Some(surface);
        Ok(())
    }

    pub fn init_audio(&self) -> Result<()> {
        if !self.profile.audio.present {
            return Ok(());
        }
        let pipeline = AudioPipeline::new()?;
        *self.audio.write() = Some(pipeline);
        Ok(())
    }
}
