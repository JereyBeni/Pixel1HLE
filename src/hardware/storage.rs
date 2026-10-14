//! Sparse disk image router mimicking the internal 32 GB partitions
//! /data/data/, /sdcard/, /system, etc.

use crate::error::{Pixel1Error, Result};
use log::info;
use std::collections::HashMap;
use parking_lot::RwLock;
use std::path::{Path, PathBuf};

pub struct SparseStorage {
    /// guest path → host path (or in-memory blob)
    mounts: RwLock<HashMap<String, PathBuf>>,
    capacity_bytes: u64,
}

impl SparseStorage {
    pub fn new(internal_gb: u32) -> Result<Self> {
        info!("Storage: sparse  {internal_gb} GB image");

        let mut mounts = HashMap::new();
        // classic Android layout
        mounts.insert("/data".into(), PathBuf::from("./guest_data"));
        mounts.insert("/data/data".into(), PathBuf::from("./guest_data/data"));
        mounts.insert("/sdcard".into(), PathBuf::from("./guest_sdcard"));
        mounts.insert("/storage/emulated/0".into(), PathBuf::from("./guest_sdcard"));
        mounts.insert("/system".into(), PathBuf::from("./guest_system")); // read-only

        // ensure host dirs exist
        for p in mounts.values() {
            std::fs::create_dir_all(p).ok();
        }

        Ok(Self {
            mounts: RwLock::new(mounts),
            capacity_bytes: (internal_gb as u64) * 1024 * 1024 * 1024,
        })
    }

    pub fn resolve(&self, guest_path: &str) -> Option<PathBuf> {
        let mounts = self.mounts.read();
        // longest prefix match
        let mut best: Option<(&str, &PathBuf)> = None;
        for (prefix, host) in mounts.iter() {
            if guest_path.starts_with(prefix) {
                if best.map(|(p, _)| prefix.len() > p.len()).unwrap_or(true) {
                    best = Some((prefix, host));
                }
            }
        }
        best.map(|(prefix, host)| {
            let rel = &guest_path[prefix.len()..];
            host.join(rel.trim_start_matches('/'))
        })
    }
}
