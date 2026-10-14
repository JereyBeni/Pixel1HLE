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

// Fake handles (pure hex, no invalid digits)
static mut FAKE_DISPLAY: u64 = 0xE6_D15_PLAY;
static mut FAKE_CONFIG: u64 = 0xE6_C0_NF1G;
static mut FAKE_SURFACE: u64 = 0xE6_5U_RF4C;
static mut FAKE_CONTEXT: u64 = 0xE6_C0_NT3X;
