//! CPU & Binary Loader + HLE Dynamic Linker
//!
//! - `elf_loader`: real PT_LOAD + AArch64 RELA (tested)
//! - `linker`: APK `.so` extract + map orchestration
//! - Execution of guest AArch64 on x86_64 requires an `AArch64Backend`
//!   (Dynarmic/Unicorn-style). Default is `NullAArch64Backend`.

pub mod elf_loader;
pub mod linker;

pub use elf_loader::{load_elf64_aarch64, AArch64Backend, LoadedImage, NullAArch64Backend};
pub use linker::{ApkElfExecutor, LoadedLibrary, SymbolResolver, ENGINE_CANDIDATES};

use crate::error::{Pixel1Error, Result};
use log::{debug, info};
use memmap2::MmapOptions;
use object::{Object, ObjectSection};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

#[derive(Debug)]
pub struct GuestMapping {
    pub guest_base: u64,
    pub host_ptr: *mut u8,
    pub size: usize,
    pub prot: u32,
}

unsafe impl Send for GuestMapping {}
unsafe impl Sync for GuestMapping {}

#[derive(Debug, Clone)]
pub struct JniHook {
    pub class: String,
    pub method: String,
    pub signature: String,
    pub host_fn: usize,
}

pub struct CpuRuntime {
    mappings: RwLock<Vec<GuestMapping>>,
    jni_hooks: RwLock<HashMap<u64, JniHook>>,
    loaded_libs: RwLock<HashMap<String, u64>>,
    next_guest_base: RwLock<u64>,
    pub linker: RwLock<Option<ApkElfExecutor>>,
}

impl CpuRuntime {
    pub fn new() -> Self {
        Self {
            mappings: RwLock::new(Vec::new()),
            jni_hooks: RwLock::new(HashMap::new()),
            loaded_libs: RwLock::new(HashMap::new()),
            next_guest_base: RwLock::new(0x7f00_0000_0000),
            linker: RwLock::new(None),
        }
    }

    /// Load a raw ELF file using the PT_LOAD-aware loader.
    pub fn load_elf_image(&self, path: &Path) -> Result<LoadedImage> {
        let data = std::fs::read(path).map_err(|e| Pixel1Error::ElfLoad(e.to_string()))?;
        let lookup = |_name: &str| -> Option<u64> { None };
        load_elf64_aarch64(&data, &lookup)
    }

    pub fn load_elf(&self, path: &Path, soname: &str) -> Result<u64> {
        info!("CPU: loading ELF {} as {}", path.display(), soname);

        // Prefer the new PT_LOAD path; fall back to section copy on failure
        match self.load_elf_image(path) {
            Ok(img) => {
                let guest_base = img.min_vaddr;
                // Leak buffer so host_ptr stays valid for the process lifetime
                // of this experimental path (same pattern as before).
                let size = img.image.len();
                let mut buf = img.image;
                let host_ptr = buf.as_mut_ptr();
                std::mem::forget(buf);
                self.mappings.write().push(GuestMapping {
                    guest_base,
                    host_ptr,
                    size,
                    prot: 0x7,
                });
                self.loaded_libs
                    .write()
                    .insert(soname.to_string(), guest_base);
                Ok(guest_base)
            }
            Err(e) => {
                debug!("PT_LOAD path failed ({e}); legacy section map");
                self.load_elf_legacy(path, soname)
            }
        }
    }

    fn load_elf_legacy(&self, path: &Path, soname: &str) -> Result<u64> {
        let file = File::open(path).map_err(|e| Pixel1Error::ElfLoad(e.to_string()))?;
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        let obj = object::File::parse(&*mmap)
            .map_err(|e| Pixel1Error::ElfLoad(format!("parse: {e}")))?;

        if obj.architecture() != object::Architecture::Aarch64
            && obj.architecture() != object::Architecture::Arm
        {
            return Err(Pixel1Error::ElfLoad("not ARM/ARM64".into()));
        }

        let mut guest_base = *self.next_guest_base.read();
        guest_base = (guest_base + 0xffff) & !0xffff;

        let mut total_size = 0usize;
        for section in obj.sections() {
            if let Ok(data) = section.data() {
                total_size = total_size.max(section.address() as usize + data.len());
            }
        }

        let mut host_buf = vec![0u8; total_size];
        for section in obj.sections() {
            if let Ok(data) = section.data() {
                let offset = section.address() as usize;
                if offset + data.len() <= host_buf.len() {
                    host_buf[offset..offset + data.len()].copy_from_slice(data);
                }
            }
        }

        let host_ptr = host_buf.as_mut_ptr();
        std::mem::forget(host_buf);

        self.mappings.write().push(GuestMapping {
            guest_base,
            host_ptr,
            size: total_size,
            prot: 0x7,
        });
        self.loaded_libs
            .write()
            .insert(soname.to_string(), guest_base);
        *self.next_guest_base.write() = guest_base + total_size as u64 + 0x10000;
        Ok(guest_base)
    }

    pub fn register_jni_hook(
        &self,
        guest_va: u64,
        class: &str,
        method: &str,
        signature: &str,
        host_fn: usize,
    ) {
        let hook = JniHook {
            class: class.to_string(),
            method: method.to_string(),
            signature: signature.to_string(),
            host_fn,
        };
        info!("CPU: JNI hook {}.{}{} → 0x{:x}", class, method, signature, host_fn);
        self.jni_hooks.write().insert(guest_va, hook);
    }

    pub fn resolve_guest_va(&self, guest_va: u64) -> Option<*mut u8> {
        let mappings = self.mappings.read();
        for m in mappings.iter() {
            if guest_va >= m.guest_base && guest_va < m.guest_base + m.size as u64 {
                let offset = (guest_va - m.guest_base) as usize;
                return Some(unsafe { m.host_ptr.add(offset) });
            }
        }
        None
    }

    pub fn lookup_jni_hook(&self, guest_va: u64) -> Option<JniHook> {
        self.jni_hooks.read().get(&guest_va).cloned()
    }

    pub fn get_lib_base(&self, soname: &str) -> Option<u64> {
        self.loaded_libs.read().get(soname).copied()
    }

    pub fn attach_linker(&self, executor: ApkElfExecutor) {
        *self.linker.write() = Some(executor);
    }
}

impl Default for CpuRuntime {
    fn default() -> Self {
        Self::new()
    }
}
