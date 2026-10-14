//! Android Framework & Binder IPC
//! Mock Binder driver. Traps getSystemService() strings and spins services lazily.
//! Strict deny-by-default: if the service is marked absent in the profile or the
//! app did not declare the permission → SecurityException / null.

use crate::error::{Pixel1Error, Result};
use crate::hardware::HardwareStack;
use log::{debug, info, warn};
use parking_lot::RwLock;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
pub enum ServicePresence {
    Present,
    Absent,
}

#[derive(Debug, Clone)]
pub struct BinderService {
    pub name: String,
    pub presence: ServicePresence,
    // opaque handle the guest will receive
    pub handle: u32,
}

/// Core Binder runtime
pub struct BinderRuntime {
    services: RwLock<HashMap<String, BinderService>>,
    next_handle: RwLock<u32>,
    /// permissions the current app declared in its AndroidManifest.xml
    declared_perms: RwLock<Vec<String>>,
    /// global allowlist from profile
    allowlist: Vec<String>,
    hw: Arc<HardwareStack>,
}

impl BinderRuntime {
    pub fn new(hw: Arc<HardwareStack>, allowlist: Vec<String>) -> Self {
        let mut services = HashMap::new();

        // Pre-register the classic system services with presence from profile
        // (in real code we would parse the yaml fully; here we hardcode the critical ones)
        let initial = [
            ("WINDOW_SERVICE", ServicePresence::Present),
            ("AUDIO_SERVICE", ServicePresence::Present),
            ("POWER_SERVICE", ServicePresence::Present),
            ("BATTERY_SERVICE", ServicePresence::Present),
            ("LOCATION_SERVICE", ServicePresence::Absent),
            ("CAMERA_SERVICE", ServicePresence::Absent),
            ("FINGERPRINT_SERVICE", ServicePresence::Absent),
        ];

        let mut handle = 1u32;
        for (name, presence) in initial {
            services.insert(
                name.to_string(),
                BinderService {
                    name: name.to_string(),
                    presence,
                    handle,
                },
            );
            handle += 1;
        }

        Self {
            services: RwLock::new(services),
            next_handle: RwLock::new(handle),
            declared_perms: RwLock::new(Vec::new()),
            allowlist,
            hw,
        }
    }

    /// Called when the guest app is loaded – we parse its manifest permissions
    pub fn set_declared_permissions(&self, perms: Vec<String>) {
        info!("Binder: app declared permissions: {:?}", perms);
        *self.declared_perms.write() = perms;
    }

    /// Core entry point: guest calls getSystemService(name)
    pub fn get_system_service(&self, name: &str) -> Result<Option<u32>> {
        debug!("Binder: getSystemService(\"{name}\")");

        let services = self.services.read();
        let svc = match services.get(name) {
            Some(s) => s,
            None => {
                warn!("Binder: unknown service '{name}' → null (deny-by-default)");
                return Ok(None);
            }
        };

        match svc.presence {
            ServicePresence::Absent => {
                warn!("Binder: service '{name}' marked ABSENT in sailfish profile → null");
                // also make sure hasSystemFeature returns false for related features
                Ok(None)
            }
            ServicePresence::Present => {
                // permission check (simplified – real Binder does much more)
                if !self.is_permission_granted(name) {
                    return Err(Pixel1Error::ServiceDenied(format!(
                        "SecurityException: {name} not granted"
                    )));
                }
                info!("Binder: returning handle {} for {name}", svc.handle);
                Ok(Some(svc.handle))
            }
        }
    }

    fn is_permission_granted(&self, service: &str) -> bool {
        // Extremely simplified mapping. Real code would look at the exact
        // permission required by each service.
        let required = match service {
            "LOCATION_SERVICE" => "android.permission.ACCESS_FINE_LOCATION",
            "CAMERA_SERVICE" => "android.permission.CAMERA",
            _ => return true, // most framework services are free once present
        };

        let declared = self.declared_perms.read();
        declared.iter().any(|p| p == required) && self.allowlist.iter().any(|p| p == required)
    }

    /// hasSystemFeature(feature) – used by apps to probe hardware
    pub fn has_system_feature(&self, feature: &str) -> bool {
        // Route to the hardware stack which knows the profile
        self.hw.has_feature(feature)
    }

    /// Transaction dispatch (very simplified Binder driver)
    pub fn transact(&self, handle: u32, code: u32, data: &[u8]) -> Result<Vec<u8>> {
        debug!("Binder: transact(handle={handle}, code={code}, {} bytes)", data.len());

        // In a real implementation we would look up the service by handle
        // and call into the corresponding mock (WindowManager, AudioManager…)
        // For now we just acknowledge and return empty parcel.
        Ok(Vec::new())
    }
}
