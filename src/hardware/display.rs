//! Display surface + Glow / Glutin + egui state machine
//! Compatible with egui_glow 0.29 / winit 0.30 ApplicationHandler.

use crate::error::{Pixel1Error, Result};
use crate::ui::{AppPicker, BootAnimation, EmulatorState, LoadingScreen, SharedUi};
use glow::HasContext;
use log::{info, warn};
use once_cell::sync::OnceCell;
use std::sync::Arc;
use std::time::Instant;

/// Global glow context for the GLES bridge (set once the window is created).
pub static GL_CONTEXT: OnceCell<Arc<glow::Context>> = OnceCell::new();

pub fn with_gl<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&glow::Context) -> R,
{
    GL_CONTEXT.get().map(|gl| f(gl))
}

pub struct DisplaySurface {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    pub ui_shared: SharedUi,
}

impl DisplaySurface {
    pub fn new(width: u32, height: u32, refresh_hz: u32, ui_shared: SharedUi) -> Result<Self> {
        info!(target: "hle::display", "Display: {width}x{height} @{refresh_hz}Hz (GL created on resume)");
        Ok(Self {
            width,
            height,
            refresh_hz,
            ui_shared,
        })
    }

    pub fn update_loading_progress(shared: &SharedUi, progress: f32, status: &str) {
        let mut g = shared.lock();
        g.loading_progress = progress.clamp(0.0, 1.0);
        g.loading_status = status.to_string();
        log::debug!(target: "hle::display", "Loading {:.0}% – {}", progress * 100.0, status);
    }

    pub fn set_state(shared: &SharedUi, state: EmulatorState) {
        let mut g = shared.lock();
        log::info!(target: "hle::display", "State → {:?}", state);
        g.state = state;
    }

    pub fn inject_touch(&self, slot: u32, x_norm: f32, y_norm: f32, down: bool) {
        let x = (x_norm * self.width as f32) as u32;
        let y = (y_norm * self.height as f32) as u32;
        log::trace!(target: "hle::display", "Touch slot={slot} ({x},{y}) down={down}");
    }
}

struct AppState {
    ui_shared: SharedUi,
    width: u32,
    height: u32,
    gl_window: Option<GlutinWindowContext>,
    gl: Option<Arc<glow::Context>>,
    egui_glow: Option<egui_glow::EguiGlow>,
    start: Instant,
    boot_timer: Option<Instant>,
}

struct GlutinWindowContext {
    window: winit::window::Window,
    gl_context: glutin::context::PossiblyCurrentContext,
    gl_surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
}

impl AppState {
    fn new(ui_shared: SharedUi, width: u32, height: u32) -> Self {
        Self {
            ui_shared,
            width,
            height,
            gl_window: None,
            gl: None,
            egui_glow: None,
            start: Instant::now(),
            boot_timer: None,
        }
    }
}

impl winit::application::ApplicationHandler for AppState {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.gl_window.is_some() {
            return;
        }

        info!(target: "hle::display", "resumed – creating GL window");

        match create_display(event_loop, self.width, self.height) {
            Ok((gl_window, gl)) => {
                let gl = Arc::new(gl);

                if GL_CONTEXT.set(Arc::clone(&gl)).is_err() {
                    warn!(target: "hle::display", "GL_CONTEXT already set");
                }

                let egui_glow = egui_glow::EguiGlow::new(
                    event_loop,
                    Arc::clone(&gl),
                    None,
                    None,
                    false,
                );

                {
                    let mut g = self.ui_shared.lock();
                    g.available_apks = scan_apks();
                    info!(target: "hle::display", "Found {} APK(s)", g.available_apks.len());
                }

                self.gl = Some(gl);
                self.egui_glow = Some(egui_glow);
                self.gl_window = Some(gl_window);
            }
            Err(e) => {
                log::error!(target: "hle::display", "Failed to create display: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(gl_window) = self.gl_window.as_mut() else {
            return;
        };
        let Some(egui_glow) = self.egui_glow.as_mut() else {
            return;
        };
        let Some(gl) = self.gl.as_ref() else {
            return;
        };

        let is_ui = {
            let g = self.ui_shared.lock();
            !matches!(g.state, EmulatorState::GameRunning { .. })
        };

        if is_ui {
            let repaint = egui_glow.on_window_event(&gl_window.window, &event);
            if repaint.repaint {
                gl_window.window.request_redraw();
            }
        }

        match event {
            winit::event::WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            winit::event::WindowEvent::RedrawRequested => {
                let time = self.start.elapsed().as_secs_f64();
                let state = self.ui_shared.lock().state.clone();

                if let EmulatorState::BootAnimation { apk_name } = &state {
                    if self.boot_timer.is_none() {
                        self.boot_timer = Some(Instant::now());
                    }
                    if self.boot_timer.unwrap().elapsed().as_secs_f32() > 2.2 {
                        DisplaySurface::set_state(
                            &self.ui_shared,
                            EmulatorState::ApkLoading {
                                apk_name: apk_name.clone(),
                            },
                        );
                        self.boot_timer = None;
                    }
                }

                unsafe {
                    gl.clear_color(0.0, 0.0, 0.0, 1.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                }

                match state {
                    EmulatorState::GameRunning { .. } => {}
                    _ => {
                        let ui_shared = self.ui_shared.clone();
                        egui_glow.run(&gl_window.window, |ctx| {
                            match &ui_shared.lock().state {
                                EmulatorState::AppPicker => AppPicker::ui(ctx, &ui_shared),
                                EmulatorState::BootAnimation { .. } => {
                                    BootAnimation::ui(ctx, &ui_shared, time)
                                }
                                EmulatorState::ApkLoading { .. } => {
                                    LoadingScreen::ui(ctx, &ui_shared)
                                }
                                EmulatorState::GameRunning { .. } => {}
                            }
                        });
                        egui_glow.paint(&gl_window.window);
                    }
                }

                use glutin::prelude::GlSurface;
                let _ = gl_window.gl_surface.swap_buffers(&gl_window.gl_context);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Some(ref gw) = self.gl_window {
            gw.window.request_redraw();
        }
    }
}

fn create_display(
    event_loop: &winit::event_loop::ActiveEventLoop,
    width: u32,
    height: u32,
) -> Result<(GlutinWindowContext, glow::Context)> {
    use glutin::config::ConfigTemplateBuilder;
    use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
    use glutin::display::{GetGlDisplay, GlDisplay};
    use glutin::prelude::*;
    use glutin::surface::SwapInterval;
    use glutin_winit::{DisplayBuilder, GlWindow};
    use raw_window_handle::HasWindowHandle;
    use winit::dpi::PhysicalSize;
    use winit::window::WindowAttributes;

    let window_attrs = WindowAttributes::default()
        .with_inner_size(PhysicalSize::new(width, height))
        .with_title("Pixel1HLE – sailfish")
        .with_resizable(false);

    let template = ConfigTemplateBuilder::new().with_alpha_size(8);
    let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attrs));

    let (window, gl_config) = display_builder
        .build(event_loop, template, |configs| {
            configs
                .reduce(|a, b| {
                    if b.num_samples() > a.num_samples() {
                        b
                    } else {
                        a
                    }
                })
                .unwrap()
        })
        .map_err(|e| Pixel1Error::Display(format!("{e}")))?;

    let window = window.ok_or_else(|| Pixel1Error::Display("no window".into()))?;
    let raw_handle = window
        .window_handle()
        .map_err(|e| Pixel1Error::Display(e.to_string()))?
        .as_raw();
    let gl_display = gl_config.display();

    let context_attribs = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(4, 5))))
        .build(Some(raw_handle));

    // glutin marks these as unsafe – isolate the FFI boundary here
    let not_current = unsafe {
        gl_display
            .create_context(&gl_config, &context_attribs)
            .map_err(|e| Pixel1Error::Display(format!("create_context: {e}")))?
    };

    let attrs = GlWindow::build_surface_attributes(&window, Default::default())
        .map_err(|e| Pixel1Error::Display(format!("surface attrs: {e}")))?;

    let gl_surface = unsafe {
        gl_display
            .create_window_surface(&gl_config, &attrs)
            .map_err(|e| Pixel1Error::Display(format!("create_window_surface: {e}")))?
    };

    let gl_context = not_current
        .make_current(&gl_surface)
        .map_err(|e| Pixel1Error::Display(format!("make_current: {e}")))?;

    let _ = gl_surface.set_swap_interval(
        &gl_context,
        SwapInterval::Wait(std::num::NonZeroU32::new(1).unwrap()),
    );

    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            gl_display.get_proc_address(&std::ffi::CString::new(s).unwrap()) as *const _
        })
    };

    unsafe {
        gl.viewport(0, 0, width as i32, height as i32);
        gl.clear_color(0.0, 0.0, 0.0, 1.0);
    }

    Ok((
        GlutinWindowContext {
            window,
            gl_context,
            gl_surface,
        },
        gl,
    ))
}

pub fn run_ui_loop(ui_shared: SharedUi, width: u32, height: u32) -> Result<()> {
    info!(target: "hle::display", "Starting UI event loop (sailfish {width}x{height})");

    let event_loop = winit::event_loop::EventLoop::new()
        .map_err(|e| Pixel1Error::Display(e.to_string()))?;

    let mut app = AppState::new(ui_shared, width, height);

    event_loop
        .run_app(&mut app)
        .map_err(|e| Pixel1Error::Display(format!("event loop: {e}")))?;

    Ok(())
}

fn scan_apks() -> Vec<crate::ui::ApkEntry> {
    let mut list = Vec::new();
    for dir in [".", "apps", "apks"] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) == Some("apk") {
                    let size_mb = e
                        .metadata()
                        .map(|m| m.len() as f32 / 1_048_576.0)
                        .unwrap_or(0.0);
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    list.push(crate::ui::ApkEntry {
                        path: path.to_string_lossy().into(),
                        name,
                        size_mb,
                    });
                }
            }
        }
    }
    list
}
