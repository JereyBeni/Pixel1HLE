//! C-Exposed GLES / EGL Hook Registry

use crate::hardware::display::with_gl;
use glow::HasContext;
use log::{debug, info};
use std::num::NonZeroU32;
use std::os::raw::{c_char, c_void};
use std::ptr;

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

static mut FAKE_DISPLAY: u64 = 0xE6_D15P_LAY;
static mut FAKE_CONFIG: u64 = 0xE6_C0NF1G;
static mut FAKE_SURFACE: u64 = 0xE6_5URF4C;
static mut FAKE_CONTEXT: u64 = 0xE6_C0NT3X;

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
    if !major.is_null() { *major = 1; }
    if !minor.is_null() { *minor = 5; }
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
    if !num_config.is_null() { *num_config = 1; }
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
    EGL_TRUE
}

#[no_mangle]
pub unsafe extern "C" fn eglSwapBuffers(
    _display: EGLDisplay,
    _surface: EGLSurface,
) -> EGLBoolean {
    debug!(target: "hle::gles", "eglSwapBuffers");
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
        0x3053 => b"EGL_VERSION 1.5\0".as_ptr() as *const c_char,
        0x3055 => b"EGL_VENDOR Pixel1HLE\0".as_ptr() as *const c_char,
        _ => ptr::null(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn glViewport(x: i32, y: i32, width: i32, height: i32) {
    debug!(target: "hle::gles", "glViewport({}, {}, {}, {})", x, y, width, height);
    with_gl(|gl| unsafe { gl.viewport(x, y, width, height); });
}

#[no_mangle]
pub unsafe extern "C" fn glClearColor(r: f32, g: f32, b: f32, a: f32) {
    debug!(target: "hle::gles", "glClearColor({r}, {g}, {b}, {a})");
    with_gl(|gl| unsafe { gl.clear_color(r, g, b, a); });
}

#[no_mangle]
pub unsafe extern "C" fn glClear(mask: u32) {
    debug!(target: "hle::gles", "glClear(0x{mask:x})");
    with_gl(|gl| unsafe { gl.clear(mask); });
}

#[no_mangle]
pub unsafe extern "C" fn glGenBuffers(n: i32, buffers: *mut u32) {
    debug!(target: "hle::gles", "glGenBuffers({n})");
    with_gl(|gl| unsafe {
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
    with_gl(|gl| unsafe {
        let buf = NonZeroU32::new(buffer).map(glow::NativeBuffer);
        gl.bind_buffer(target, buf);
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
    with_gl(|gl| unsafe {
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
    with_gl(|gl| unsafe {
        gl.draw_elements(mode, count, type_, indices as i32);
    });
}

#[no_mangle]
pub unsafe extern "C" fn glEnable(cap: u32) {
    with_gl(|gl| unsafe { gl.enable(cap); });
}

#[no_mangle]
pub unsafe extern "C" fn glDisable(cap: u32) {
    with_gl(|gl| unsafe { gl.disable(cap); });
}

#[no_mangle]
pub unsafe extern "C" fn glGetError() -> u32 {
    with_gl(|gl| unsafe { gl.get_error() }).unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn glGetString(name: u32) -> *const c_char {
    match name {
        0x1F00 => b"Pixel1HLE GLES\0".as_ptr() as *const c_char,
        0x1F01 => b"OpenGL ES 3.2 via glow/glutin\0".as_ptr() as *const c_char,
        0x1F02 => b"OpenGL ES 3.2\0".as_ptr() as *const c_char,
        0x1F03 => b"GL_OES_EGL_image GL_EXT_texture_format_BGRA8888\0".as_ptr() as *const c_char,
        _ => ptr::null(),
    }
}

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

pub fn register_gles_symbols(_cpu: &crate::cpu::CpuRuntime) {
    info!(target: "hle::gles", "GLES/EGL symbol table registered");
}
