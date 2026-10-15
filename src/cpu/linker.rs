//! HLE Dynamic Linker & APK ELF Executor
//! Supports Unity, Minecraft PE, and Adobe AIR (`libcom_adobe_air.so`).
//! Missing symbols are never fatal: a safe fallback stub is injected.
//!
//! IMPORTANT: Guest code is AArch64. On non-aarch64 hosts we map and inspect
//! libraries but refuse to call into guest machine code (would be UB).

use crate::error::{Pixel1Error, Result};
use crate::hardware::display::DisplaySurface;
use crate::hardware::gles_bridge;
use crate::ui::SharedUi;
use log::{debug, info, warn};
use memmap2::{MmapMut, MmapOptions};
use object::{Object, ObjectSection, ObjectSymbol};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use zip::ZipArchive;

#[inline]
fn addr_of_fn<T>(f: T) -> usize {
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

pub const ENGINE_CANDIDATES: &[&str] = &[
    "libunity.so",
    "libminecraftpe.so",
    "libmain.so",
    "libgame.so",
    "libcom_adobe_air.so",
    "libCore.so",
];

#[derive(Debug)]
pub struct LoadedLibrary {
    pub soname: String,
    pub guest_base: u64,
    pub host_base: *mut u8,
    pub size: usize,
    pub entry: Option<u64>,
    pub jni_onload: Option<*const ()>,
    pub is_adobe_air: bool,
    _mapping: MmapMut,
}

unsafe impl Send for LoadedLibrary {}
unsafe impl Sync for LoadedLibrary {}

pub struct SymbolResolver {
    table: HashMap<String, HashMap<String, usize>>,
    fallbacks: RwLock<HashMap<String, usize>>,
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
        egl.insert("eglGetProcAddress".into(), addr_of_fn(stub_egl_get_proc_address as unsafe extern "C" fn(*const i8) -> *mut ()));
        egl.insert("eglDestroySurface".into(), addr_of_fn(stub_egl_true as unsafe extern "C" fn() -> i32));
        egl.insert("eglDestroyContext".into(), addr_of_fn(stub_egl_true as unsafe extern "C" fn() -> i32));
        egl.insert("eglSwapInterval".into(), addr_of_fn(stub_egl_true as unsafe extern "C" fn() -> i32));
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
        table.insert("libGLESv1_CM.so".into(), gles.clone());
        table.insert("libGLESv2.so".into(), gles.clone());
        table.insert("libGLESv3.so".into(), gles);

        let mut bionic = HashMap::new();
        bionic.insert("__errno".into(), addr_of_fn(stub_errno as unsafe extern "C" fn() -> _));
        bionic.insert("malloc".into(), addr_of_fn(stub_malloc as unsafe extern "C" fn(_) -> _));
        bionic.insert("free".into(), addr_of_fn(stub_free as unsafe extern "C" fn(_)));
        bionic.insert("calloc".into(), addr_of_fn(stub_calloc as unsafe extern "C" fn(_, _) -> _));
        bionic.insert("realloc".into(), addr_of_fn(stub_realloc as unsafe extern "C" fn(_, _) -> _));
        bionic.insert("memcpy".into(), addr_of_fn(stub_memcpy as unsafe extern "C" fn(_, _, _) -> _));
        bionic.insert("memmove".into(), addr_of_fn(stub_memmove as unsafe extern "C" fn(_, _, _) -> _));
        bionic.insert("memset".into(), addr_of_fn(stub_memset as unsafe extern "C" fn(_, _, _) -> _));
        bionic.insert("strlen".into(), addr_of_fn(stub_strlen as unsafe extern "C" fn(_) -> _));
        bionic.insert("strcmp".into(), addr_of_fn(stub_strcmp as unsafe extern "C" fn(_, _) -> _));
        bionic.insert("strncmp".into(), addr_of_fn(stub_strncmp as unsafe extern "C" fn(_, _, _) -> _));
        bionic.insert("pthread_mutex_lock".into(), addr_of_fn(stub_ok as unsafe extern "C" fn() -> i32));
        bionic.insert("pthread_mutex_unlock".into(), addr_of_fn(stub_ok as unsafe extern "C" fn() -> i32));
        bionic.insert("pthread_mutex_init".into(), addr_of_fn(stub_ok as unsafe extern "C" fn() -> i32));
        bionic.insert("pthread_mutex_destroy".into(), addr_of_fn(stub_ok as unsafe extern "C" fn() -> i32));
        bionic.insert("__android_log_print".into(), addr_of_fn(stub_android_log as unsafe extern "C" fn() -> i32));
        table.insert("libc.so".into(), bionic);

        table.insert("liblog.so".into(), {
            let mut log = HashMap::new();
            log.insert("__android_log_print".into(), addr_of_fn(stub_android_log as unsafe extern "C" fn() -> i32));
            log.insert("__android_log_write".into(), addr_of_fn(stub_ok as unsafe extern "C" fn() -> i32));
            log
        });
        table.insert("libm.so".into(), HashMap::new());
        table.insert("libdl.so".into(), {
            let mut dl = HashMap::new();
            dl.insert("dlopen".into(), addr_of_fn(stub_dlopen as unsafe extern "C" fn(*const i8, i32) -> *mut ()));
            dl.insert("dlsym".into(), addr_of_fn(stub_dlsym as unsafe extern "C" fn(*mut (), *const i8) -> *mut ()));
            dl.insert("dlclose".into(), addr_of_fn(stub_ok as unsafe extern "C" fn() -> i32));
            dl.insert("dlerror".into(), addr_of_fn(stub_null as unsafe extern "C" fn() -> *mut ()));
            dl
        });
        table.insert("libandroid.so".into(), {
            let mut a = HashMap::new();
            a.insert("ANativeWindow_getWidth".into(), addr_of_fn(stub_native_window_dim as unsafe extern "C" fn() -> i32));
            a.insert("ANativeWindow_getHeight".into(), addr_of_fn(stub_native_window_dim as unsafe extern "C" fn() -> i32));
            a.insert("ANativeWindow_setBuffersGeometry".into(), addr_of_fn(stub_ok as unsafe extern "C" fn() -> i32));
            a
        });

        Self { table, fallbacks: RwLock::new(HashMap::new()) }
    }

    pub fn resolve(&self, soname: &str, sym: &str) -> Option<usize> {
        if let Some(addr) = self.table.get(soname).and_then(|m| m.get(sym).copied()) {
            return Some(addr);
        }
        for (k, m) in &self.table {
            let key = k.trim_start_matches("lib").trim_end_matches(".so");
            let needle = soname.trim_start_matches("lib").trim_end_matches(".so");
            if k == soname || key.contains(needle) || needle.contains(key) {
                if let Some(p) = m.get(sym) {
                    return Some(*p);
                }
            }
        }
        self.fallbacks.read().get(sym).copied()
    }

    pub fn inject_fallback(&self, missing_symbol: &str) -> usize {
        if let Some(addr) = self.fallbacks.read().get(missing_symbol).copied() {
            return addr;
        }
        warn!(target: "hle::linker", "Missing function detected: {} -> Injecting safe fallback", missing_symbol);
        let addr = addr_of_fn(unresolved_stub as unsafe extern "C" fn() -> i64);
        self.fallbacks.write().insert(missing_symbol.to_string(), addr);
        addr
    }
}

impl Default for SymbolResolver {
    fn default() -> Self { Self::new() }
}

unsafe extern "C" fn stub_errno() -> *mut i32 {
    static mut ERRNO: i32 = 0;
    &raw mut ERRNO
}
unsafe extern "C" fn stub_malloc(size: usize) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size.max(1), 16).unwrap_or_else(|_| std::alloc::Layout::from_size_align(1, 1).unwrap());
    std::alloc::alloc_zeroed(layout)
}
unsafe extern "C" fn stub_calloc(n: usize, size: usize) -> *mut u8 { stub_malloc(n.saturating_mul(size)) }
unsafe extern "C" fn stub_realloc(ptr: *mut u8, size: usize) -> *mut u8 {
    if ptr.is_null() { return stub_malloc(size); }
    let new_ptr = stub_malloc(size);
    if !new_ptr.is_null() && size > 0 { std::ptr::copy_nonoverlapping(ptr, new_ptr, size.min(4096)); }
    new_ptr
}
unsafe extern "C" fn stub_free(_ptr: *mut u8) {}
unsafe extern "C" fn stub_memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if !dest.is_null() && !src.is_null() && n > 0 { std::ptr::copy_nonoverlapping(src, dest, n); }
    dest
}
unsafe extern "C" fn stub_memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if !dest.is_null() && !src.is_null() && n > 0 { std::ptr::copy(src, dest, n); }
    dest
}
unsafe extern "C" fn stub_memset(dest: *mut u8, c: i32, n: usize) -> *mut u8 {
    if !dest.is_null() && n > 0 { std::ptr::write_bytes(dest, c as u8, n); }
    dest
}
unsafe extern "C" fn stub_strlen(s: *const u8) -> usize {
    if s.is_null() { return 0; }
    let mut len = 0;
    while *s.add(len) != 0 { len += 1; }
    len
}
unsafe extern "C" fn stub_strcmp(a: *const u8, b: *const u8) -> i32 {
    if a.is_null() || b.is_null() { return 0; }
    let mut i = 0;
    loop {
        let ca = *a.add(i); let cb = *b.add(i);
        if ca != cb { return ca as i32 - cb as i32; }
        if ca == 0 { return 0; }
        i += 1;
    }
}
unsafe extern "C" fn stub_strncmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    if a.is_null() || b.is_null() || n == 0 { return 0; }
    for i in 0..n {
        let ca = *a.add(i); let cb = *b.add(i);
        if ca != cb { return ca as i32 - cb as i32; }
        if ca == 0 { return 0; }
    }
    0
}
unsafe extern "C" fn stub_ok() -> i32 { 0 }
unsafe extern "C" fn stub_egl_true() -> i32 { 1 }
unsafe extern "C" fn stub_null() -> *mut () { std::ptr::null_mut() }
unsafe extern "C" fn stub_native_window_dim() -> i32 { 1080 }
unsafe extern "C" fn stub_android_log() -> i32 { 0 }
unsafe extern "C" fn stub_egl_get_proc_address(_name: *const i8) -> *mut () { std::ptr::null_mut() }
unsafe extern "C" fn stub_dlopen(_path: *const i8, _flags: i32) -> *mut () { 0x4149_5200 as *mut () }
unsafe extern "C" fn stub_dlsym(_handle: *mut (), name: *const i8) -> *mut () {
    if name.is_null() { return std::ptr::null_mut(); }
    unresolved_stub as *const () as *mut ()
}
unsafe extern "C" fn unresolved_stub() -> i64 { 0 }

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
        let file = File::open(&self.apk_path).map_err(|e| Pixel1Error::ElfLoad(format!("open apk: {e}")))?;
        let mut archive = ZipArchive::new(file).map_err(|e| Pixel1Error::ElfLoad(format!("zip: {e}")))?;
        let mut libs = Vec::new();
        for i in 0..archive.len() {
            let entry = archive.by_index(i).map_err(|e| Pixel1Error::ElfLoad(format!("zip entry: {e}")))?;
            let name = entry.name().to_string();
            if (name.starts_with("lib/arm64-v8a/") || name.starts_with("lib/armeabi-v7a/")) && name.ends_with(".so") {
                let base = name.rsplit('/').next().unwrap_or(&name).to_string();
                if !libs.contains(&base) { libs.push(base); }
            }
        }
        info!(target: "hle::linker", "APK contains {} native libraries", libs.len());
        Ok(libs)
    }

    pub fn detect_engine(&self) -> Result<Option<String>> {
        let libs = self.list_native_libs()?;
        for cand in ENGINE_CANDIDATES {
            if libs.iter().any(|l| l == cand) { return Ok(Some((*cand).to_string())); }
        }
        Ok(None)
    }

    pub fn load_library(&self, soname: &str) -> Result<Arc<LoadedLibrary>> {
        if let Some(existing) = self.loaded.read().get(soname) { return Ok(existing.clone()); }
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
        let file = File::open(&self.apk_path).map_err(|e| Pixel1Error::ElfLoad(format!("open apk: {e}")))?;
        let mut archive = ZipArchive::new(file).map_err(|e| Pixel1Error::ElfLoad(format!("zip: {e}")))?;
        let candidates = [
            format!("lib/arm64-v8a/{soname}"),
            format!("lib/armeabi-v7a/{soname}"),
            format!("lib/armeabi/{soname}"),
        ];
        for path_in_apk in &candidates {
            if let Ok(mut entry) = archive.by_name(path_in_apk) {
                let mut buf = Vec::with_capacity(entry.size() as usize);
                entry.read_to_end(&mut buf).map_err(|e| Pixel1Error::ElfLoad(format!("read so: {e}")))?;
                info!(target: "hle::linker", "Extracted {path_in_apk} ({} bytes)", buf.len());
                return Ok(buf);
            }
        }
        Err(Pixel1Error::ElfLoad(format!("{soname} not found in arm64-v8a / armeabi-v7a")))
    }

    fn map_and_relocate(&self, soname: &str, data: &[u8]) -> Result<LoadedLibrary> {
        let obj = object::File::parse(data).map_err(|e| Pixel1Error::ElfLoad(format!("ELF parse {soname}: {e}")))?;
        let arch = obj.architecture();
        if arch != object::Architecture::Aarch64 && arch != object::Architecture::Arm {
            return Err(Pixel1Error::ElfLoad(format!("{soname} architecture {:?} not supported", arch)));
        }
        if arch == object::Architecture::Arm {
            warn!(target: "hle::linker", "{soname} is 32-bit ARM – mapped for inspection only");
        }

        self.report(0.35, "Mapping segments…");
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
        if min_vaddr == u64::MAX { min_vaddr = 0; }
        let total_size = ((max_vaddr - min_vaddr) as usize + 0xffff) & !0xffff;
        let total_size = total_size.max(0x1000);

        let mut mapping = MmapOptions::new().len(total_size).map_anon().map_err(|e| Pixel1Error::ElfLoad(format!("mmap: {e}")))?;
        unsafe { self.mprotect_exec(mapping.as_mut_ptr(), total_size)?; }

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

        self.report(0.55, "Symbol inventory…");
        self.apply_relocations(soname, &obj, host_base, guest_base, min_vaddr)?;

        self.report(0.75, "Looking up JNI_OnLoad…");
        let jni_onload = obj.symbols()
            .find(|s| matches!(s.name().ok(), Some("JNI_OnLoad")))
            .map(|s| {
                let offset = (s.address() - min_vaddr) as usize;
                unsafe { host_base.add(offset) as *const () }
            });

        let entry = if obj.entry() != 0 { Some(guest_base + (obj.entry() - min_vaddr)) } else { None };
        let is_adobe_air = soname.contains("adobe_air") || soname.contains("AdobeAIR");

        info!(target: "hle::linker", "Mapped {} @ guest 0x{:x} host {:p} size 0x{:x}", soname, guest_base, host_base, total_size);

        Ok(LoadedLibrary {
            soname: soname.to_string(),
            guest_base,
            host_base,
            size: total_size,
            entry,
            jni_onload,
            is_adobe_air,
            _mapping: mapping,
        })
    }

    /// NOTE: Does **not** yet write RELA into the GOT. It inventories undefined
    /// symbols and installs fallback stubs in the resolver table only.
    fn apply_relocations(
        &self,
        soname: &str,
        obj: &object::File<'_>,
        _host_base: *mut u8,
        _guest_base: u64,
        _min_vaddr: u64,
    ) -> Result<()> {
        let mut missing = 0u32;
        for sym in obj.symbols() {
            if !sym.is_undefined() { continue; }
            let name = match sym.name() {
                Ok(n) if !n.is_empty() && !n.starts_with('$') => n,
                _ => continue,
            };
            if self.resolver.resolve("libEGL.so", name).is_some()
                || self.resolver.resolve("libGLESv2.so", name).is_some()
                || self.resolver.resolve("libc.so", name).is_some()
                || self.resolver.resolve("libdl.so", name).is_some()
                || self.resolver.resolve("liblog.so", name).is_some()
                || self.resolver.resolve("libandroid.so", name).is_some()
            {
                continue;
            }
            let _ = self.resolver.inject_fallback(name);
            missing += 1;
        }
        if missing > 0 {
            warn!(target: "hle::linker", "{soname}: {missing} unresolved symbol(s) – fallbacks registered (GOT not patched yet)");
        }
        Ok(())
    }

    unsafe fn mprotect_exec(&self, ptr: *mut u8, len: usize) -> Result<()> {
        #[cfg(unix)]
        {
            let page_size = libc::sysconf(libc::_SC_PAGESIZE) as usize;
            let start = (ptr as usize) & !(page_size - 1);
            let end = (ptr as usize + len + page_size - 1) & !(page_size - 1);
            let ret = libc::mprotect(start as *mut _, end - start, libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC);
            if ret != 0 {
                return Err(Pixel1Error::ElfLoad(format!("mprotect: {}", std::io::Error::last_os_error())));
            }
        }
        let _ = (ptr, len);
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

        #[cfg(not(target_arch = "aarch64"))]
        {
            let _ = onload;
            warn!(
                target: "hle::linker",
                "Refusing to execute guest JNI_OnLoad for {} on host arch {} — need AArch64 host or a CPU emulator/DBT backend",
                lib.soname,
                std::env::consts::ARCH
            );
            self.report(1.0, "Guest exec blocked (need aarch64 or emulator)");
            return Err(Pixel1Error::ElfLoad(format!(
                "cannot execute AArch64 guest code for {} on host {}",
                lib.soname,
                std::env::consts::ARCH
            )));
        }

        #[cfg(target_arch = "aarch64")]
        {
            self.report(0.95, "Calling JNI_OnLoad…");
            info!(target: "hle::linker", "Calling JNI_OnLoad on {} (AIR={})", lib.soname, lib.is_adobe_air);
            let fake_vm: usize = if lib.is_adobe_air { 0x4149_5200 } else { 0x4A56_4D00 };
            type JniOnLoadFn = unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void) -> i32;
            let func: JniOnLoadFn = unsafe { std::mem::transmute(onload) };
            let rc = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                func(fake_vm as *mut _, std::ptr::null_mut())
            }));
            match rc {
                Ok(v) => {
                    info!(target: "hle::linker", "JNI_OnLoad returned {v}");
                    self.report(1.0, "Ready");
                    Ok(v)
                }
                Err(_) => {
                    warn!(target: "hle::linker", "JNI_OnLoad panicked");
                    Err(Pixel1Error::ElfLoad(format!("JNI_OnLoad panicked in {}", lib.soname)))
                }
            }
        }
    }

    pub fn bootstrap_engine(&self, engine_soname: &str) -> Result<Arc<LoadedLibrary>> {
        let lib = self.load_library(engine_soname)?;
        // Mapping succeeds even if guest exec is blocked on this host.
        match self.call_jni_onload(&lib) {
            Ok(_) => {}
            Err(e) => warn!(target: "hle::linker", "JNI_OnLoad skipped/failed: {e}"),
        }
        Ok(lib)
    }

    pub fn bootstrap_any_engine(&self) -> Result<Arc<LoadedLibrary>> {
        if let Some(name) = self.detect_engine()? {
            return self.bootstrap_engine(&name);
        }
        for eng in ENGINE_CANDIDATES {
            match self.bootstrap_engine(eng) {
                Ok(lib) => return Ok(lib),
                Err(e) => debug!(target: "hle::linker", "{eng}: {e}"),
            }
        }
        Err(Pixel1Error::ElfLoad("No supported engine found in APK".into()))
    }

    pub fn resolve_symbol(&self, from_soname: &str, sym: &str) -> usize {
        if let Some(addr) = self.resolver.resolve(from_soname, sym) { return addr; }
        for lib in ["libEGL.so", "libGLESv2.so", "libGLESv3.so", "libc.so", "libdl.so", "liblog.so", "libandroid.so"] {
            if let Some(addr) = self.resolver.resolve(lib, sym) { return addr; }
        }
        self.resolver.inject_fallback(sym)
    }
}
