//! CPU & Binary Loader + HLE Dynamic Linker

mod linker;

pub use linker::{ApkElfExecutor, LoadedLibrary, SymbolResolver};

use crate::error::{Pixel1Error, Result};
use log::{debug, info, warn};
use memmap2::MmapOptions;
use object::{Object, ObjectSection};
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
    pub prot: u32,
}

unsafe impl Send for GuestMapping {}
unsafe impl Sync for GuestMapping {}

/// JNI native method hook table entry
#[derive(Debug, Clone)]
pub struct JniHook {
    pub class: String,
    pub method: String,
    pub signature: String,
    pub host_fn: usize,
}

/// Core CPU / loader state
pub struct CpuRuntime {
    mappings: RwLock<Vec<GuestMapping>>,
    jni_hooks: RwLock<HashMap<u64, JniHook>>,
    loaded_libs: RwLock<HashMap<String, u64>>,
    next_guest_base: RwLock<u64>,
    /// The HLE dynamic linker instance
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

    /// Legacy simple ELF loader (kept for non-APK paths)
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

        let mapping = GuestMapping {
            guest_base,
            host_ptr,
            size: total_size,
            prot: 0x7,
        };

        self.mappings.write().push(mapping);
        self.loaded_libs.write().insert(soname.to_string(), guest_base);
        *self.next_guest_base.write() = guest_base + total_size as u64 + 0x10000;

        debug!("CPU: mapped {} @ guest 0x{:x} (size 0x{:x})", soname, guest_base, total_size);
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

    /// Attach a fully constructed ApkElfExecutor (called from Pixel1Translator)
    pub fn attach_linker(&self, executor: ApkElfExecutor) {
        *self.linker.write() = Some(executor);
    }

    /// Demo: simulate the exact early graphics calls that libunity.so issues
    pub fn demo_unity_graphics_bootstrap(&self) {
        info!(target: "hle::gles", "CPU: simulating Unity graphics bootstrap cascade");

        unsafe {
            let display = crate::hardware::gles_bridge::eglGetDisplay(std::ptr::null_mut());
            let mut major = 0;
            let mut minor = 0;
            crate::hardware::gles_bridge::eglInitialize(display, &mut major, &mut minor);

            let mut config: crate::hardware::gles_bridge::EGLConfig = std::ptr::null_mut();
            let mut num = 0;
            crate::hardware::gles_bridge::eglChooseConfig(
                display,
                std::ptr::null(),
                &mut config,
                1,
                &mut num,
            );

            let surface = crate::hardware::gles_bridge::eglCreateWindowSurface(
                display,
                config,
                std::ptr::null_mut(),
                std::ptr::null(),
            );

            let ctx = crate::hardware::gles_bridge::eglCreateContext(
                display,
                config,
                std::ptr::null_mut(),
                std::ptr::null(),
            );

            crate::hardware::gles_bridge::eglMakeCurrent(display, surface, surface, ctx);
        }

        unsafe {
            crate::hardware::gles_bridge::glViewport(0, 0, 1920, 1080);
            crate::hardware::gles_bridge::glClearColor(0.1, 0.2, 0.3, 1.0);
            crate::hardware::gles_bridge::glClear(0x00004000 | 0x00000100);
        }

        unsafe {
            let mut buf = 0u32;
            crate::hardware::gles_bridge::glGenBuffers(1, &mut buf);
            crate::hardware::gles_bridge::glBindBuffer(0x8892, buf);
            let dummy: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
            crate::hardware::gles_bridge::glBufferData(
                0x8892,
                16,
                dummy.as_ptr() as *const _,
                0x88E4,
            );
        }

        info!(target: "hle::gles", "CPU: Unity bootstrap cascade completed – no faults");
    }
}

impl Default for CpuRuntime {
    fn default() -> Self {
        Self::new()
    }
}
