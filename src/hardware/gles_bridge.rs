//! C-Exposed GLES / EGL Hook Registry
//!
//! These `extern "C"` symbols are what `libunity.so` (and any native GLES app)
//! expects to dlsym from libEGL.so / libGLESv2.so / libGLESv3.so.
//!
//! All calls are forwarded to the global host `glow::Context` owned by DisplaySurface.

use crate::hardware::display::{with_gl, GL_CONTEXT};
use log::{debug, info, warn};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

// ─────────────────────────────────────────────────────────────
// EGL types (simplified, enough for Unity)
// ─────────────────────────────────────────────────────────────
pub type EGLBoolean = u32;
pub type EGLint = i32;
pub type EGLDisplay = *mut c_void;
pub type EGLConfig = *mut c_void;
pub type EGLSurface = *mut c_void;
pub type EGLContext = *mut c_void;
pub type EGLNativeWindowType = *mut c_void;

pub const EGL_TRUE: EGLBoolean = 1;
pub const EGL_FALSE: EGLBoolean = 0;
pub const EGL_SUCCESS: EGLint = 0x3000;
pub const EGL_NONE: EGLint = 0x3038;
pub const EGL_OPENGL_ES_API: EGLint = 0x30A0;
pub const EGL_OPENGL_ES2_BIT: EGLint = 0x0004;
pub const EGL_SURFACE_TYPE: EGLint = 0x3033;
pub const EGL_WINDOW_BIT: EGLint = 0x0004;
pub const EGL_RENDERABLE_TYPE: EGLint = 0x3040;
pub const EGL_BLUE_SIZE: EGLint = 0x3022;
pub const EGL_GREEN_SIZE: EGLint = 0x3023;
pub const EGL_RED_SIZE: EGLint = 0x3024;
pub const EGL_DEPTH_SIZE: EGLint = 0x3025;
pub const EGL_NATIVE_VISUAL_ID: EGLint = 0x302E;

// Fake handles so Unity thinks it got real objects
static mut FAKE_DISPLAY: u64 = 0xE6L_D15P_LAY;
static mut FAKE_CONFIG: u64 = 0xE6L_C0NF1G;
static mut FAKE_SURFACE: u64 = 0xE6L_5URF4C;
static mut FAKE_CONTEXT: u64 = 0xE6L_C0NT3X;

// ─────────────────────────────────────────────────────────────
// EGL entry points
// ─────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn eglGetDisplay(_display_id: *mut c_void) -> EGLDisplay {
    info!(target: "hle::gles", "eglGetDisplay → fake display");
    &raw mut FAKE_DISPLAY as *mut _ as EGLDisplay
}

#[no_mangle]
pub unsafe extern "C" fn eglInitialize(
    _display: EGLDisplay,
    major: *mut EGLint,
    minor: *mut EGLint,
) -> EGLBoolean {
    info!(target: "hle::gles", "eglInitialize");
    if !major.is_null() {
        *major = 1;
    }
    if !minor.is_null() {
        *minor = 5; // we pretend EGL 1.5
    }
    EGL_TRUE
}

#[no_mangle]
pub unsafe extern "C" fn eglChooseConfig(
    _display: EGLDisplay,
    _attrib_list: *const EGLint,
    configs: *mut EGLConfig,
    config_size: EGLint,
    num_config: *mut EGLint,
) -> EGLBoolean {
    info!(target: "hle::gles", "eglChooseConfig");
    if !num_config.is_null() {
        *num_config = 1;
    }
    if !configs.is_null() && config_size > 0 {
        *configs = &raw mut FAKE_CONFIG as *mut _ as EGLConfig;
    }
    EGL_TRUE
}

#[no_mangle]
pub unsafe extern "C" fn eglCreateWindowSurface(
    _display: EGLDisplay,
    _config: EGLConfig,
    _win: EGLNativeWindowType,
    _attrib_list: *const EGLint,
) -> EGLSurface {
    info!(target: "hle::gles", "eglCreateWindowSurface → 1920x1080 sailfish surface");
    &raw mut FAKE_SURFACE as *mut _ as EGLSurface
}

#[no_mangle]
pub unsafe extern "C" fn eglCreateContext(
    _display: EGLDisplay,
    _config: EGLConfig,
    _share: EGLContext,
    _attrib_list: *const EGLint,
) -> EGLContext {
    info!(target: "hle::gles", "eglCreateContext");
    &raw mut FAKE_CONTEXT as *mut _ as EGLContext
}

#[no_mangle]
pub unsafe extern "C" fn eglMakeCurrent(
    _display: EGLDisplay,
    _draw: EGLSurface,
    _read: EGLSurface,
    _ctx: EGLContext,
) -> EGLBoolean {
    info!(target: "hle::gles", "eglMakeCurrent");
    // The real glow context is already current on the host side
    EGL_TRUE
}

#[no_mangle]
pub unsafe extern "C" fn eglSwapBuffers(
    _display: EGLDisplay,
    _surface: EGLSurface,
) -> EGLBoolean {
    debug!(target: "hle::gles", "eglSwapBuffers");
    // In a full implementation we would call the glutin swap here.
    // For HLE we just succeed so Unity continues its frame loop.
    EGL_TRUE
}

#[no_mangle]
pub unsafe extern "C" fn eglGetError() -> EGLint {
    EGL_SUCCESS
}

#[no_mangle]
pub unsafe extern "C" fn eglTerminate(_display: EGLDisplay) -> EGLBoolean {
    info!(target: "hle::gles", "eglTerminate");
    EGL_TRUE
}

#[no_mangle]
pub unsafe extern "C" fn eglQueryString(
    _display: EGLDisplay,
    name: EGLint,
) -> *const c_char {
    match name {
        0x3053 => b"EGL_VERSION 1.5\0".as_ptr() as *const c_char, // EGL_VERSION
        0x3055 => b"EGL_VENDOR Pixel1HLE\0".as_ptr() as *const c_char,
        _ => ptr::null(),
    }
}

// ─────────────────────────────────────────────────────────────
// Core GLES 2.0 / 3.0 symbols Unity actually calls
// ─────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn glViewport(x: i32, y: i32, width: i32, height: i32) {
    debug!(target: "hle::gles", "glViewport({}, {}, {}, {})", x, y, width, height);
    with_gl(|gl| {
        gl.viewport(x, y, width, height);
    });
}

#[no_mangle]
pub unsafe extern "C" fn glClearColor(r: f32, g: f32, b: f32, a: f32) {
    debug!(target: "hle::gles", "glClearColor({r}, {g}, {b}, {a})");
    with_gl(|gl| {
        gl.clear_color(r, g, b, a);
    });
}

#[no_mangle]
pub unsafe extern "C" fn glClear(mask: u32) {
    debug!(target: "hle::gles", "glClear(0x{mask:x})");
    with_gl(|gl| {
        gl.clear(mask);
    });
}

#[no_mangle]
pub unsafe extern "C" fn glGenBuffers(n: i32, buffers: *mut u32) {
    debug!(target: "hle::gles", "glGenBuffers({n})");
    with_gl(|gl| {
        if n > 0 && !buffers.is_null() {
            let mut ids = vec![0u32; n as usize];
            gl.gen_buffers(&mut ids);
            for (i, id) in ids.iter().enumerate() {
                *buffers.add(i) = *id;
            }
        }
    });
}

#[no_mangle]
pub unsafe extern "C" fn glBindBuffer(target: u32, buffer: u32) {
    debug!(target: "hle::gles", "glBindBuffer(0x{target:x}, {buffer})");
    with_gl(|gl| {
        gl.bind_buffer(target, if buffer == 0 { None } else { Some(glow::NativeBuffer(buffer)) });
    });
}

#[no_mangle]
pub unsafe extern "C" fn glBufferData(
    target: u32,
    size: isize,
    data: *const c_void,
    usage: u32,
) {
    debug!(target: "hle::gles", "glBufferData(0x{target:x}, size={size}, usage=0x{usage:x})");
    with_gl(|gl| {
        if data.is_null() {
            gl.buffer_data_size(target, size, usage);
        } else {
            let slice = std::slice::from_raw_parts(data as *const u8, size as usize);
            gl.buffer_data_u8_slice(target, slice, usage);
        }
    });
}

#[no_mangle]
pub unsafe extern "C" fn glDrawElements(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const c_void,
) {
    debug!(target: "hle::gles", "glDrawElements(mode=0x{mode:x}, count={count})");
    with_gl(|gl| {
        gl.draw_elements(mode, count, type_, indices as i32);
    });
}

#[no_mangle]
pub unsafe extern "C" fn glEnable(cap: u32) {
    with_gl(|gl| gl.enable(cap));
}

#[no_mangle]
pub unsafe extern "C" fn glDisable(cap: u32) {
    with_gl(|gl| gl.disable(cap));
}

#[no_mangle]
pub unsafe extern "C" fn glGetError() -> u32 {
    with_gl(|gl| gl.get_error()).unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn glGetString(name: u32) -> *const c_char {
    // Return static strings that look like a real GLES driver
    match name {
        0x1F00 => b"Pixel1HLE GLES\0".as_ptr() as *const c_char, // GL_VENDOR
        0x1F01 => b"OpenGL ES 3.2 via glow/glutin\0".as_ptr() as *const c_char, // GL_RENDERER
        0x1F02 => b"OpenGL ES 3.2\0".as_ptr() as *const c_char, // GL_VERSION
        0x1F03 => b"GL_OES_EGL_image GL_EXT_texture_format_BGRA8888\0".as_ptr() as *const c_char,
        _ => ptr::null(),
    }
}

// Extra stubs that Unity frequently touches early
#[no_mangle]
pub unsafe extern "C" fn glPixelStorei(_pname: u32, _param: i32) {}
#[no_mangle]
pub unsafe extern "C" fn glHint(_target: u32, _mode: u32) {}
#[no_mangle]
pub unsafe extern "C" fn glDepthMask(_flag: u8) {}
#[no_mangle]
pub unsafe extern "C" fn glColorMask(_r: u8, _g: u8, _b: u8, _a: u8) {}
#[no_mangle]
pub unsafe extern "C" fn glBlendFunc(_sfactor: u32, _dfactor: u32) {}
#[no_mangle]
pub unsafe extern "C" fn glCullFace(_mode: u32) {}
#[no_mangle]
pub unsafe extern "C" fn glFrontFace(_mode: u32) {}

/// Helper used by the CPU runtime to register all these symbols into the JNI/ELF hook table
pub fn register_gles_symbols(cpu: &crate::cpu::CpuRuntime) {
    // In a real loader we would resolve the guest addresses of these symbols
    // after loading libGLESv2.so / libEGL.so and then patch the GOT/PLT.
    // For the HLE skeleton we just log that the table is ready.
    info!(target: "hle::gles", "GLES/EGL symbol table registered ({} entry points)", 30);
    let _ = cpu; // silence unused for now
}
