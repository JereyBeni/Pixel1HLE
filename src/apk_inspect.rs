//! Real APK / ZIP inspection (Milestone B).
//!
//! This module does **not** execute code. It validates package structure,
//! lists native libraries by ABI, and reports whether a binary manifest exists.

use crate::error::{Pixel1Error, Result};
use log::info;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use zip::ZipArchive;

/// Supported Android ABIs we care about for sailfish-era packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Abi {
    Arm64V8a,
    ArmeabiV7a,
    Armeabi,
    X86,
    X86_64,
    Unknown,
}

impl Abi {
    pub fn as_str(self) -> &'static str {
        match self {
            Abi::Arm64V8a => "arm64-v8a",
            Abi::ArmeabiV7a => "armeabi-v7a",
            Abi::Armeabi => "armeabi",
            Abi::X86 => "x86",
            Abi::X86_64 => "x86_64",
            Abi::Unknown => "unknown",
        }
    }

    fn from_lib_path(path: &str) -> Option<Self> {
        // lib/<abi>/<name>.so
        let mut parts = path.split('/');
        if parts.next()? != "lib" {
            return None;
        }
        let abi = parts.next()?;
        let name = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        if !name.ends_with(".so") {
            return None;
        }
        Some(match abi {
            "arm64-v8a" => Abi::Arm64V8a,
            "armeabi-v7a" => Abi::ArmeabiV7a,
            "armeabi" => Abi::Armeabi,
            "x86" => Abi::X86,
            "x86_64" => Abi::X86_64,
            _ => Abi::Unknown,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeLib {
    pub abi: Abi,
    pub soname: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApkInfo {
    pub path: String,
    pub has_manifest: bool,
    pub manifest_size: u64,
    pub native_libs: Vec<NativeLib>,
    pub entry_count: usize,
    pub total_uncompressed: u64,
}

impl ApkInfo {
    pub fn abis(&self) -> Vec<Abi> {
        let mut v = Vec::new();
        for lib in &self.native_libs {
            if !v.contains(&lib.abi) {
                v.push(lib.abi);
            }
        }
        v
    }

    pub fn libs_for_abi(&self, abi: Abi) -> Vec<&NativeLib> {
        self.native_libs.iter().filter(|l| l.abi == abi).collect()
    }

    pub fn has_engine_candidate(&self) -> bool {
        const ENGINES: &[&str] = &[
            "libunity.so",
            "libminecraftpe.so",
            "libmain.so",
            "libgame.so",
            "libcom_adobe_air.so",
            "libCore.so",
        ];
        self.native_libs
            .iter()
            .any(|l| ENGINES.iter().any(|e| l.soname == *e))
    }
}

/// Inspect an on-disk APK. Returns structured metadata or a typed error.
pub fn inspect_apk(path: &Path) -> Result<ApkInfo> {
    let file = File::open(path).map_err(|e| {
        Pixel1Error::Import(format!("cannot open {}: {e}", path.display()))
    })?;
    let mut archive = ZipArchive::new(file).map_err(|e| {
        Pixel1Error::Import(format!("not a valid ZIP/APK ({}): {e}", path.display()))
    })?;

    let mut has_manifest = false;
    let mut manifest_size = 0u64;
    let mut native_libs = Vec::new();
    let mut total_uncompressed = 0u64;
    let entry_count = archive.len();

    for i in 0..entry_count {
        let entry = archive.by_index(i).map_err(|e| {
            Pixel1Error::Import(format!("zip entry {i}: {e}"))
        })?;
        let name = entry.name().to_string();
        let size = entry.size();
        total_uncompressed = total_uncompressed.saturating_add(size);

        if name == "AndroidManifest.xml" {
            has_manifest = true;
            manifest_size = size;
        }

        if let Some(abi) = Abi::from_lib_path(&name) {
            let soname = name
                .rsplit('/')
                .next()
                .unwrap_or(&name)
                .to_string();
            native_libs.push(NativeLib { abi, soname, size });
        }
    }

    native_libs.sort_by(|a, b| {
        a.abi
            .as_str()
            .cmp(b.abi.as_str())
            .then_with(|| a.soname.cmp(&b.soname))
    });

    let info = ApkInfo {
        path: path.display().to_string(),
        has_manifest,
        manifest_size,
        native_libs,
        entry_count,
        total_uncompressed,
    };

    info!(
        target: "hle::apk",
        "Inspected {}: {} entries, manifest={}, abis={:?}, libs={}",
        info.path,
        info.entry_count,
        info.has_manifest,
        info.abis(),
        info.native_libs.len()
    );

    Ok(info)
}

/// Write a minimal synthetic APK (ZIP) for tests. Not a valid Android package
/// for installation — only exercises our inspector.
pub fn write_fixture_apk(path: &Path, libs: &[(&str, &[u8])]) -> Result<()> {
    let file = File::create(path).map_err(|e| Pixel1Error::Import(e.to_string()))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);

    zip.start_file("AndroidManifest.xml", opts)
        .map_err(|e| Pixel1Error::Import(e.to_string()))?;
    // Minimal binary-looking blob (not real AXML) — presence is what we test.
    zip.write_all(b"\x03\x00\x08\x00fixture-manifest")
        .map_err(|e| Pixel1Error::Import(e.to_string()))?;

    for (lib_path, bytes) in libs {
        zip.start_file(*lib_path, opts)
            .map_err(|e| Pixel1Error::Import(e.to_string()))?;
        zip.write_all(bytes)
            .map_err(|e| Pixel1Error::Import(e.to_string()))?;
    }

    zip.finish()
        .map_err(|e| Pixel1Error::Import(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_apk(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("pixel1hle_test_{name}_{}.apk", std::process::id()));
        p
    }

    #[test]
    fn inspect_valid_fixture_lists_arm64_lib() {
        let path = temp_apk("ok");
        write_fixture_apk(
            &path,
            &[("lib/arm64-v8a/libunity.so", b"\x7fELF\x02\x01fake")],
        )
        .expect("write fixture");

        let info = inspect_apk(&path).expect("inspect");
        assert!(info.has_manifest);
        assert_eq!(info.manifest_size, 22);
        assert_eq!(info.native_libs.len(), 1);
        assert_eq!(info.native_libs[0].abi, Abi::Arm64V8a);
        assert_eq!(info.native_libs[0].soname, "libunity.so");
        assert!(info.has_engine_candidate());
        assert_eq!(info.abis(), vec![Abi::Arm64V8a]);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn inspect_lists_multiple_abis() {
        let path = temp_apk("multi");
        write_fixture_apk(
            &path,
            &[
                ("lib/arm64-v8a/libmain.so", b"elf64"),
                ("lib/armeabi-v7a/libmain.so", b"elf32"),
            ],
        )
        .expect("write");

        let info = inspect_apk(&path).expect("inspect");
        assert_eq!(info.native_libs.len(), 2);
        assert!(info.abis().contains(&Abi::Arm64V8a));
        assert!(info.abis().contains(&Abi::ArmeabiV7a));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn inspect_rejects_non_zip() {
        let path = temp_apk("bad");
        std::fs::write(&path, b"not a zip").unwrap();
        let err = inspect_apk(&path).expect_err("must fail");
        let msg = err.to_string();
        assert!(
            msg.contains("ZIP") || msg.contains("zip") || msg.contains("APK"),
            "unexpected error: {msg}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn inspect_missing_file() {
        let path = PathBuf::from("/tmp/pixel1hle_definitely_missing_12345.apk");
        let err = inspect_apk(&path).expect_err("must fail");
        assert!(err.to_string().contains("cannot open"));
    }

    #[test]
    fn abi_from_lib_path_rejects_traversal() {
        assert!(Abi::from_lib_path("lib/arm64-v8a/../x.so").is_none());
        assert!(Abi::from_lib_path("assets/foo.so").is_none());
        assert_eq!(
            Abi::from_lib_path("lib/arm64-v8a/libx.so"),
            Some(Abi::Arm64V8a)
        );
    }
}
