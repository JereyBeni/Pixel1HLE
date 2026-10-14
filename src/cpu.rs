//! CPU & Binary Loader
//! ELF parser (object + memmap2) that loads ARM64 native .so from APK,
//! maps guest code into host memory and maintains JNI function pointer hook tables.

use crate::error::{Pixel1Error, Result};
use log::{debug, info, warn};
use memmap2::{Mmap, MmapOptions};
use object::{Object, ObjectSection, ObjectSymbol};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

/// Guest virtual address space region
#[derive(Debug)]
pub struct GuestMapping {
    pub guest_base: u64,
    pub host_ptr: *mut u8,
    pub size: usize,
    pub prot: u32, // PROT_READ | PROT_WRITE | PROT_EXEC
}

// Safety: we own the mapping lifetime via the Mmap
unsafe impl Send for GuestMapping {}
unsafe impl Sync for GuestMapping {}

/// JNI native method hook table entry
#[derive(Debug, Clone)]
pub struct JniHook {
    pub class: String,
    pub method: String,
    pub signature: String,
    pub host_fn: usize, // function pointer as usize
}

/// Core CPU / loader state
pub struct CpuRuntime {
    mappings: RwLock<Vec<GuestMapping>>,
    /// guest_va → host_fn
    jni_hooks: RwLock<HashMap<u64, JniHook>>,
    /// loaded libraries (soname → base)
    loaded_libs: RwLock<HashMap<String, u64>>,
    next_guest_base: RwLock<u64>,
}

impl CpuRuntime {
    pub fn new() -> Self {
        Self {
            mappings: RwLock::new(Vec::new()),
            jni_hooks: RwLock::new(HashMap::new()),
            loaded_libs: RwLock::new(HashMap::new()),
            // start guest mappings at a high canonical address to avoid host collisions
            next_guest_base: RwLock::new(0x7f00_0000_0000),
        }
    }

    /// Load an ARM64 ELF .so from the APK filesystem into the guest address space
    pub fn load_elf(&self, path: &Path, soname: &str) -> Result<u64> {
        info!("CPU: loading ELF {} as {}", path.display(), soname);

        let file = File::open(path).map_err(|e| Pixel1Error::ElfLoad(e.to_string()))?;
        let mmap = unsafe { MmapOptions::new().map(&file)? };

        let obj = object::File::parse(&*mmap)
            .map_err(|e| Pixel1Error::ElfLoad(format!("parse: {e}")))?;

        if obj.architecture() != object::Architecture::Aarch64 {
            return Err(Pixel1Error::ElfLoad("not ARM64".into()));
        }

        let mut guest_base = *self.next_guest_base.read();
        // align to 64 KiB
        guest_base = (guest_base + 0xffff) & !0xffff;

        let mut total_size = 0usize;
        for section in obj.sections() {
            if let Ok(data) = section.data() {
                total_size = total_size.max(section.address() as usize + data.len());
            }
        }

        // allocate host memory that will back the guest mapping
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
        // leak the vec so the mapping lives for the process lifetime (HLE style)
        std::mem::forget(host_buf);

        let mapping = GuestMapping {
            guest_base,
            host_ptr,
            size: total_size,
            prot: 0x7, // RWX for simplicity in HLE (real would use mprotect)
        };

        self.mappings.write().push(mapping);
        self.loaded_libs.write().insert(soname.to_string(), guest_base);
        *self.next_guest_base.write() = guest_base + total_size as u64 + 0x10000;

        debug!("CPU: mapped {} @ guest 0x{:x} (size 0x{:x})", soname, guest_base, total_size);
        Ok(guest_base)
    }

    /// Register a JNI native method so the guest can call into host code
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

    /// Resolve a guest virtual address to a host pointer (for call interception)
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

    /// Lookup a registered JNI hook by guest VA
    pub fn lookup_jni_hook(&self, guest_va: u64) -> Option<JniHook> {
        self.jni_hooks.read().get(&guest_va).cloned()
    }

    pub fn get_lib_base(&self, soname: &str) -> Option<u64> {
        self.loaded_libs.read().get(soname).copied()
    }
}

impl Default for CpuRuntime {
    fn default() -> Self {
        Self::new()
    }
}
