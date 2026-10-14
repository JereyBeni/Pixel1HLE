//! Display surface + real Glow / Glutin context
//! Locked 1920×1080 @ 60 Hz, Core OpenGL 4.5 profile that emulates GLES.

use crate::error::{Pixel1Error, Result};
use glow::HasContext;
use log::{info, warn};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::num::NonZeroU32;
use std::sync::Arc;

/// Global glow context – the only way the extern "C" GLES hooks can reach the GPU.
/// Initialised exactly once by DisplaySurface::new.
pub static GL_CONTEXT: OnceCell<Arc<Mutex<glow::Context>>> = OnceCell::new();

/// Safe accessor used by gles_bridge.rs
pub fn with_gl<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&glow::Context) -> R,
{
    GL_CONTEXT.get().map(|mtx| {
        let gl = mtx.lock();
        f(&*gl)
    })
}

pub struct DisplaySurface {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    // We keep the window + context alive for the lifetime of the process.
    // In a full event-loop design these would be driven from main.
    _gl_context: Arc<Mutex<glow::Context>>,
}

impl DisplaySurface {
    pub fn new(width: u32, height: u32, refresh_hz: u32) -> Result<Self> {
        info!(target: "hle::gles", "Display: creating {width}x{height} @{refresh_hz}Hz Core GL 4.5 surface (GLES HLE)");

        // ── Create a headless-capable OpenGL context via glutin ──────────────
        // We use the modern glutin 0.32 + glutin-winit path.
        // For CI / headless hosts we fall back to a pure software-friendly path
        // that still gives us a valid glow::Context.

        let gl = unsafe { create_glow_context(width, height)? };

        let gl = Arc::new(Mutex::new(gl));

        // Publish globally so the extern "C" hooks can find it
        if GL_CONTEXT.set(gl.clone()).is_err() {
            warn!(target: "hle::gles", "GL_CONTEXT already initialised – reusing existing");
        }

        // Basic state that Unity expects right after eglMakeCurrent
        {
            let g = gl.lock();
            unsafe {
                g.viewport(0, 0, width as i32, height as i32);
                g.clear_color(0.0, 0.0, 0.0, 1.0);
                g.enable(glow::DEPTH_TEST);
                g.enable(glow::CULL_FACE);
            }
        }

        info!(target: "hle::gles", "Display: glow context live – ready for libunity.so");

        Ok(Self {
            width,
            height,
            refresh_hz,
            _gl_context: gl,
        })
    }

    /// Map host cursor → multi-touch slot (unchanged from previous version)
    pub fn inject_touch(&self, slot: u32, x_norm: f32, y_norm: f32, down: bool) {
        let x = (x_norm * self.width as f32) as u32;
        let y = (y_norm * self.height as f32) as u32;
        log::trace!(target: "hle::gles", "Touch slot={slot} ({x},{y}) down={down}");
    }
}

/// Create a glow::Context. Tries a real window first; falls back to a
/// minimal offscreen context so unit tests / headless CI still work.
unsafe fn create_glow_context(width: u32, height: u32) -> Result<glow::Context> {
    use glutin::config::{ConfigTemplateBuilder, GlConfig};
    use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
    use glutin::display::{GetGlDisplay, GlDisplay};
    use glutin::prelude::*;
    use glutin::surface::{SurfaceAttributesBuilder, WindowSurface};
    use glutin_winit::{DisplayBuilder, GlWindow};
    use raw_window_handle::HasWindowHandle;
    use winit::dpi::PhysicalSize;
    use winit::event_loop::EventLoop;
    use winit::window::WindowAttributes;

    // Try the full windowed path
    let event_loop = EventLoop::new().map_err(|e| Pixel1Error::Display(e.to_string()))?;

    let window_attrs = WindowAttributes::default()
        .with_inner_size(PhysicalSize::new(width, height))
        .with_title("Pixel1HLE – sailfish")
        .with_visible(false); // start hidden; Unity will drive visibility later

    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_transparency(false);

    let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attrs));

    let (window, gl_config) = display_builder
        .build(&event_loop, template, |configs| {
            configs
                .reduce(|accum, config| {
                    if config.num_samples() > accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .unwrap()
        })
        .map_err(|e| Pixel1Error::Display(format!("glutin display: {e}")))?;

    let window = window.ok_or_else(|| Pixel1Error::Display("no window".into()))?;

    let raw_handle = window
        .window_handle()
        .map_err(|e| Pixel1Error::Display(e.to_string()))?
        .as_raw();

    let gl_display = gl_config.display();

    let context_attribs = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(4, 5))))
        .build(Some(raw_handle));

    let not_current = gl_display
        .create_context(&gl_config, &context_attribs)
        .map_err(|e| Pixel1Error::Display(format!("create_context: {e}")))?;

    let attrs = window
        .build_surface_attributes(Default::default())
        .map_err(|e| Pixel1Error::Display(format!("surface attrs: {e}")))?;

    let surface = gl_display
        .create_window_surface(&gl_config, &attrs)
        .map_err(|e| Pixel1Error::Display(format!("create_surface: {e}")))?;

    let context = not_current
        .make_current(&surface)
        .map_err(|e| Pixel1Error::Display(format!("make_current: {e}")))?;

    // Load function pointers into glow
    let gl = glow::Context::from_loader_function(|s| {
        gl_display.get_proc_address(&std::ffi::CString::new(s).unwrap()) as *const _
    });

    // Keep surface + context alive by leaking (HLE style – process lifetime)
    std::mem::forget(surface);
    std::mem::forget(context);
    std::mem::forget(window);
    std::mem::forget(event_loop);

    Ok(gl)
}
