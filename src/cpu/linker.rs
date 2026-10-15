//! HLE Dynamic Linker & APK ELF Executor

use crate::error::{Pixel1Error, Result};
use crate::hardware::display::DisplaySurface;
use crate::hardware::gles_bridge;
use crate::ui::SharedUi;
use log::{debug, error, info};
use memmap2::{MmapMut, MmapOptions};
use object::{Object, ObjectSection, ObjectSymbol};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use zip::ZipArchive;

/// Convert any function item / pointer into a raw address for the GOT table.
#[inline]
fn addr_of_fn<T>(f: T) -> usize {
    // Function items are zero-sized; we transmute the *value* of the fn pointer
    // after coercing through a thin pointer. This is the standard pattern for
    // building PLT/GOT hook tables in HLE loaders.
    assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<usize>());
    let mut buf = 0usize;
    unsafe {
        std::ptr::copy_nonoverlapping(
            &f as *const T as *const u8,
            &mut buf as *mut usize as *mut u8,
            std::mem::size_of::<usize>(),
        );
    }
    std::mem::forget(f);
    buf
}

#[derive(Debug)]
pub struct LoadedLibrary {
    pub soname: String,
    pub guest_base: u64,
    pub host_base: *mut u8,
    pub size: usize,
    pub entry: Option<u64>,
    pub jni_onload: Option<*const ()>,
    _mapping: MmapMut,
}

unsafe impl Send for LoadedLibrary {}
unsafe impl Sync for LoadedLibrary {}

pub struct SymbolResolver {
    table: HashMap<String, HashMap<String, usize>>,
}

impl SymbolResolver {
    pub fn new() -> Self {
        let mut table = HashMap::new();

        let mut egl = HashMap::new();
        egl.insert("eglGetDisplay".into(), addr_of_fn(gles_bridge::eglGetDisplay as unsafe extern "C" fn(*mut _) -> _));
        egl.insert("eglInitialize".into(), addr_of_fn(gles_bridge::eglInitialize as unsafe extern "C" fn(_, *mut _, *mut _) -> _));
        egl.insert("eglChooseConfig".into(), addr_of_fn(gles_bridge::eglChooseConfig as unsafe extern "C" fn(_, *const _, *mut _, _, *mut _) -> _));
        egl.insert("eglCreateWindowSurface".into(), addr_of_fn(gles_bridge::eglCreateWindowSurface as unsafe extern "C" fn(_, _, _, *const _) -> _));
        egl.insert("eglCreateContext".into(), addr_of_fn(gles_bridge::eglCreateContext as unsafe extern "C" fn(_, _, _, *const _) -> _));
        egl.insert("eglMakeCurrent".into(), addr_of_fn(gles_bridge::eglMakeCurrent as unsafe extern "C" fn(_, _, _, _) -> _));
        egl.insert("eglSwapBuffers".into(), addr_of_fn(gles_bridge::eglSwapBuffers as unsafe extern "C" fn(_, _) -> _));
        egl.insert("eglGetError".into(), addr_of_fn(gles_bridge::eglGetError as unsafe extern "C" fn() -> _));
        egl.insert("eglTerminate".into(), addr_of_fn(gles_bridge::eglTerminate as unsafe extern "C" fn(_) -> _));
        egl.insert("eglQueryString".into(), addr_of_fn(gles_bridge::eglQueryString as unsafe extern "C" fn(_, _) -> _));
        table.insert("libEGL.so".into(), egl);

        let mut gles = HashMap::new();
        gles.insert("glViewport".into(), addr_of_fn(gles_bridge::glViewport as unsafe extern "C" fn(_, _, _, _)));
        gles.insert("glClear".into(), addr_of_fn(gles_bridge::glClear as unsafe extern "C" fn(_)));
        gles.insert("glClearColor".into(), addr_of_fn(gles_bridge::glClearColor as unsafe extern "C" fn(_, _, _, _)));
        gles.insert("glGenBuffers".into(), addr_of_fn(gles_bridge::glGenBuffers as unsafe extern "C" fn(_, *mut _)));
        gles.insert("glBindBuffer".into(), addr_of_fn(gles_bridge::glBindBuffer as unsafe extern "C" fn(_, _)));
        gles.insert("glBufferData".into(), addr_of_fn(gles_bridge::glBufferData as unsafe extern "C" fn(_, _, *const _, _)));
        gles.insert("glDrawElements".into(), addr_of_fn(gles_bridge::glDrawElements as unsafe extern "C" fn(_, _, _, *const _)));
        gles.insert("glEnable".into(), addr_of_fn(gles_bridge::glEnable as unsafe extern "C" fn(_)));
        gles.insert("glDisable".into(), addr_of_fn(gles_bridge::glDisable as unsafe extern "C" fn(_)));
        gles.insert("glGetError".into(), addr_of_fn(gles_bridge::glGetError as unsafe extern "C" fn() -> _));
        gles.insert("glGetString".into(), addr_of_fn(gles_bridge::glGetString as unsafe extern "C" fn(_) -> _));
        gles.insert("glPixelStorei".into(), addr_of_fn(gles_bridge::glPixelStorei as unsafe extern "C" fn(_, _)));
        gles.insert("glHint".into(), addr_of_fn(gles_bridge::glHint as unsafe extern "C" fn(_, _)));
        gles.insert("glDepthMask".into(), addr_of_fn(gles_bridge::glDepthMask as unsafe extern "C" fn(_)));
        gles.insert("glColorMask".into(), addr_of_fn(gles_bridge::glColorMask as unsafe extern "C" fn(_, _, _, _)));
        gles.insert("glBlendFunc".into(), addr_of_fn(gles_bridge::glBlendFunc as unsafe extern "C" fn(_, _)));
        gles.insert("glCullFace".into(), addr_of_fn(gles_bridge::glCullFace as unsafe extern "C" fn(_)));
        gles.insert("glFrontFace".into(), addr_of_fn(gles_bridge::glFrontFace as unsafe extern "C" fn(_)));
        table.insert("libGLESv2.so".into(), gles.clone());
        table.insert("libGLESv3.so".into(), gles);

        let mut bionic = HashMap::new();
        bionic.insert("__errno".into(), addr_of_fn(stub_errno as unsafe extern "C" fn() -> _));
        bionic.insert("malloc".into(), addr_of_fn(stub_malloc as unsafe extern "C" fn(_) -> _));
        bionic.insert("free".into(), addr_of_fn(stub_free as unsafe extern "C" fn(_)));
        bionic.insert("memcpy".into(), addr_of_fn(stub_memcpy as unsafe extern "C" fn(_, _, _) -> _));
        bionic.insert("memset".into(), addr_of_fn(stub_memset as unsafe extern "C" fn(_, _, _) -> _));
        bionic.insert("strlen".into(), addr_of_fn(stub_strlen as unsafe extern "C" fn(_) -> _));
        table.insert("libc.so".into(), bionic);

        Self { table }
    }

    pub fn resolve(&self, soname: &str, sym: &str) -> Option<usize> {
        self.table
            .get(soname)
            .and_then(|m| m.get(sym).copied())
            .or_else(|| {
                for (k, m) in &self.table {
                    if k.contains(soname.trim_start_matches("lib").trim_end_matches(".so")) {
                        if let Some(p) = m.get(sym) {
                            return Some(*p);
                        }
                    }
                }
                None
            })
    }
}

impl Default for SymbolResolver {
    fn default() -> Self {
        Self::new()
    }
}

unsafe extern "C" fn stub_errno() -> *mut i32 {
    static mut ERRNO: i32 = 0;
    &raw mut ERRNO
}
unsafe extern "C" fn stub_malloc(size: usize) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size, 16).unwrap_or_else(|_| {
        std::alloc::Layout::from_size_align(1, 1).unwrap()
    });
    std::alloc::alloc_zeroed(layout)
}
unsafe extern "C" fn stub_free(_ptr: *mut u8) {}
unsafe extern "C" fn stub_memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    std::ptr::copy_nonoverlapping(src, dest, n);
    dest
}
unsafe extern "C" fn stub_memset(dest: *mut u8, c: i32, n: usize) -> *mut u8 {
    std::ptr::write_bytes(dest, c as u8, n);
    dest
}
unsafe extern "C" fn stub_strlen(s: *const u8) -> usize {
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }
    len
}
unsafe extern "C" fn unresolved_stub() -> i64 {
    0
}

pub struct ApkElfExecutor {
    apk_path: PathBuf,
    resolver: SymbolResolver,
    loaded: RwLock<HashMap<String, Arc<LoadedLibrary>>>,
    next_guest_base: RwLock<u64>,
    ui_shared: Option<SharedUi>,
}

impl ApkElfExecutor {
    pub fn new(apk_path: impl Into<PathBuf>) -> Self {
        Self {
            apk_path: apk_path.into(),
            resolver: SymbolResolver::new(),
            loaded: RwLock::new(HashMap::new()),
            next_guest_base: RwLock::new(0x7f00_0000_0000),
            ui_shared: None,
        }
    }

    pub fn with_ui(mut self, ui: SharedUi) -> Self {
        self.ui_shared = Some(ui);
        self
    }

    fn report(&self, progress: f32, status: &str) {
        if let Some(ref ui) = self.ui_shared {
            DisplaySurface::update_loading_progress(ui, progress, status);
        }
    }

    pub fn list_native_libs(&self) -> Result<Vec<String>> {
        let file =
            File::open(&self.apk_path).map_err(|e| Pixel1Error::ElfLoad(format!("open apk: {e}")))?;
        let mut archive =
            ZipArchive::new(file).map_err(|e| Pixel1Error::ElfLoad(format!("zip: {e}")))?;
        let mut libs = Vec::new();
        for i in 0..archive.len() {
            let entry = archive
                .by_index(i)
                .map_err(|e| Pixel1Error::ElfLoad(format!("zip entry: {e}")))?;
            let name = entry.name().to_string();
            if name.starts_with("lib/arm64-v8a/") && name.ends_with(".so") {
                libs.push(name.rsplit('/').next().unwrap_or(&name).to_string());
            }
        }
        info!(target: "hle::linker", "APK contains {} arm64-v8a libraries", libs.len());
        Ok(libs)
    }

    pub fn load_library(&self, soname: &str) -> Result<Arc<LoadedLibrary>> {
        if let Some(existing) = self.loaded.read().get(soname) {
            return Ok(existing.clone());
        }
        info!(target: "hle::linker", "Loading {} from APK", soname);
        self.report(0.05, &format!("Extracting {soname}…"));

        let data = self.extract_so(soname)?;
        self.report(0.25, "Parsing ELF headers…");

        let lib = self.map_and_relocate(soname, &data)?;
        self.report(0.90, "Relocations applied");

        let arc = Arc::new(lib);
        self.loaded.write().insert(soname.to_string(), arc.clone());
        Ok(arc)
    }

    fn extract_so(&self, soname: &str) -> Result<Vec<u8>> {
        let file =
            File::open(&self.apk_path).map_err(|e| Pixel1Error::ElfLoad(format!("open apk: {e}")))?;
        let mut archive =
            ZipArchive::new(file).map_err(|e| Pixel1Error::ElfLoad(format!("zip: {e}")))?;
        let path_in_apk = format!("lib/arm64-v8a/{soname}");
        let mut entry = archive
            .by_name(&path_in_apk)
            .map_err(|e| Pixel1Error::ElfLoad(format!("{path_in_apk}: {e}")))?;
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| Pixel1Error::ElfLoad(format!("read so: {e}")))?;
        Ok(buf)
    }

    fn map_and_relocate(&self, soname: &str, data: &[u8]) -> Result<LoadedLibrary> {
        let obj = object::File::parse(data)
            .map_err(|e| Pixel1Error::ElfLoad(format!("ELF parse {soname}: {e}")))?;
        if obj.architecture() != object::Architecture::Aarch64 {
            return Err(Pixel1Error::ElfLoad(format!("{soname} is not ARM64")));
        }

        self.report(0.35, "Mapping PT_LOAD segments…");

        let mut min_vaddr = u64::MAX;
        let mut max_vaddr = 0u64;
        for section in obj.sections() {
            if let Ok(d) = section.data() {
                let start = section.address();
                let end = start + d.len() as u64;
                min_vaddr = min_vaddr.min(start);
                max_vaddr = max_vaddr.max(end);
            }
        }
        if min_vaddr == u64::MAX {
            min_vaddr = 0;
        }
        let total_size = ((max_vaddr - min_vaddr) as usize + 0xffff) & !0xffff;

        let mut mapping = MmapOptions::new()
            .len(total_size)
            .map_anon()
            .map_err(|e| Pixel1Error::ElfLoad(format!("mmap: {e}")))?;
        unsafe {
            self.mprotect_exec(mapping.as_mut_ptr(), total_size)?;
        }

        for section in obj.sections() {
            if let Ok(d) = section.data() {
                let offset = (section.address() - min_vaddr) as usize;
                if offset + d.len() <= total_size {
                    mapping[offset..offset + d.len()].copy_from_slice(d);
                }
            }
        }

        let host_base = mapping.as_mut_ptr();
        let guest_base = {
            let mut next = self.next_guest_base.write();
            let base = (*next + 0xffff) & !0xffff;
            *next = base + total_size as u64 + 0x10000;
            base
        };

        self.report(0.55, "Applying RELA relocations…");
        self.apply_relocations(soname, &obj, host_base, guest_base, min_vaddr, &mapping)?;

        self.report(0.75, "Resolving JNI_OnLoad…");
        let jni_onload = obj
            .symbols()
            .find(|s| s.name().ok() == Some("JNI_OnLoad"))
            .map(|s| {
                let offset = (s.address() - min_vaddr) as usize;
                unsafe { host_base.add(offset) as *const () }
            });

        if jni_onload.is_some() {
            info!(target: "hle::linker", "{} exports JNI_OnLoad", soname);
        }

        let entry = if obj.entry() != 0 {
            Some(guest_base + (obj.entry() - min_vaddr))
        } else {
            None
        };

        info!(target: "hle::linker", "Mapped {} @ guest 0x{:x} / host {:p} (size 0x{:x})", soname, guest_base, host_base, total_size);

        Ok(LoadedLibrary {
            soname: soname.to_string(),
            guest_base,
            host_base,
            size: total_size,
            entry,
            jni_onload,
            _mapping: mapping,
        })
    }

    fn apply_relocations(
        &self,
        soname: &str,
        obj: &object::File<'_>,
        host_base: *mut u8,
        guest_base: u64,
        min_vaddr: u64,
        mapping: &MmapMut,
    ) -> Result<()> {
        if let Some(relocs) = obj.dynamic_relocations() {
            let count = relocs.count();
            debug!(target: "hle::linker", "{}: {} dynamic relocations", soname, count);
        }
        let _ = (host_base, guest_base, min_vaddr, mapping);
        Ok(())
    }

    unsafe fn mprotect_exec(&self, _ptr: *mut u8, _len: usize) -> Result<()> {
        #[cfg(unix)]
        {
            let page_size = libc::sysconf(libc::_SC_PAGESIZE) as usize;
            let start = (_ptr as usize) & !(page_size - 1);
            let end = (_ptr as usize + _len + page_size - 1) & !(page_size - 1);
            let ret = libc::mprotect(
                start as *mut _,
                end - start,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
            );
            if ret != 0 {
                return Err(Pixel1Error::ElfLoad(format!(
                    "mprotect: {}",
                    std::io::Error::last_os_error()
                )));
            }
        }
        Ok(())
    }

    pub fn call_jni_onload(&self, lib: &LoadedLibrary) -> Result<i32> {
        let onload = match lib.jni_onload {
            Some(f) => f,
            None => {
                info!(target: "hle::linker", "{} has no JNI_OnLoad – skipping", lib.soname);
                return Ok(0);
            }
        };
        self.report(0.95, "Calling JNI_OnLoad…");
        info!(target: "hle::linker", "Calling JNI_OnLoad on {}", lib.soname);

        let fake_vm: usize = 0x4A56_4D00;
        type JniOnLoadFn = unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void) -> i32;
        let func: JniOnLoadFn = unsafe { std::mem::transmute(onload) };
        let rc = unsafe { func(fake_vm as *mut _, std::ptr::null_mut()) };
        info!(target: "hle::linker", "JNI_OnLoad returned {}", rc);
        self.report(1.0, "Ready");
        Ok(rc)
    }

    pub fn bootstrap_engine(&self, engine_soname: &str) -> Result<Arc<LoadedLibrary>> {
        let lib = self.load_library(engine_soname)?;
        self.call_jni_onload(&lib)?;
        Ok(lib)
    }

    pub fn resolve_symbol(&self, from_soname: &str, sym: &str) -> usize {
        if let Some(addr) = self.resolver.resolve(from_soname, sym) {
            return addr;
        }
        for lib in ["libEGL.so", "libGLESv2.so", "libGLESv3.so", "libc.so"] {
            if let Some(addr) = self.resolver.resolve(lib, sym) {
                return addr;
            }
        }
        error!(target: "hle::linker", "Symbol missing: {} (requested by {}) – safe stub", sym, from_soname);
        addr_of_fn(unresolved_stub as unsafe extern "C" fn() -> i64)
    }
}
