//! Display surface + Glow / Glutin + egui state machine

use crate::error::{Pixel1Error, Result};
use crate::ui::{AppPicker, BootAnimation, EmulatorState, LoadingScreen, SharedUi};
use glow::HasContext;
use log::{info, warn};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Instant;

pub static GL_CONTEXT: OnceCell<Arc<Mutex<glow::Context>>> = OnceCell::new();

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
    _gl_context: Arc<Mutex<glow::Context>>,
    pub ui_shared: SharedUi,
}

impl DisplaySurface {
    pub fn new(width: u32, height: u32, refresh_hz: u32, ui_shared: SharedUi) -> Result<Self> {
        info!(target: "hle::display", "Display: creating {width}x{height} @{refresh_hz}Hz + egui state machine");

        let gl = unsafe { create_glow_context(width, height)? };
        let gl = Arc::new(Mutex::new(gl));

        if GL_CONTEXT.set(gl.clone()).is_err() {
            warn!(target: "hle::display", "GL_CONTEXT already initialised");
        }

        {
            let g = gl.lock();
            unsafe {
                g.viewport(0, 0, width as i32, height as i32);
                g.clear_color(0.0, 0.0, 0.0, 1.0);
            }
        }

        info!(target: "hle::display", "Display: glow + egui ready");

        Ok(Self {
            width,
            height,
            refresh_hz,
            _gl_context: gl,
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

unsafe fn create_glow_context(width: u32, height: u32) -> Result<glow::Context> {
    use glutin::config::ConfigTemplateBuilder;
    use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
    use glutin::display::{GetGlDisplay, GlDisplay};
    use glutin::prelude::*;
    use glutin_winit::{DisplayBuilder, GlWindow};
    use raw_window_handle::HasWindowHandle;
    use winit::dpi::PhysicalSize;
    use winit::event_loop::EventLoop;
    use winit::window::WindowAttributes;

    let event_loop = EventLoop::new().map_err(|e| Pixel1Error::Display(e.to_string()))?;

    let window_attrs = WindowAttributes::default()
        .with_inner_size(PhysicalSize::new(width, height))
        .with_title("Pixel1HLE – sailfish")
        .with_visible(true);

    let template = ConfigTemplateBuilder::new().with_alpha_size(8);
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
    let raw_handle = window.window_handle().map_err(|e| Pixel1Error::Display(e.to_string()))?.as_raw();
    let gl_display = gl_config.display();

    let context_attribs = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(4, 5))))
        .build(Some(raw_handle));

    let not_current = gl_display
        .create_context(&gl_config, &context_attribs)
        .map_err(|e| Pixel1Error::Display(format!("create_context: {e}")))?;

    let attrs = GlWindow::build_surface_attributes(&window, Default::default())
        .map_err(|e| Pixel1Error::Display(format!("surface attrs: {e}")))?;
    let surface = gl_display.create_window_surface(&gl_config, &attrs)
        .map_err(|e| Pixel1Error::Display(format!("create_surface: {e}")))?;
    let context = not_current.make_current(&surface)
        .map_err(|e| Pixel1Error::Display(format!("make_current: {e}")))?;

    let gl = glow::Context::from_loader_function(|s| {
        gl_display.get_proc_address(&std::ffi::CString::new(s).unwrap()) as *const _
    });

    std::mem::forget(surface);
    std::mem::forget(context);
    std::mem::forget(window);
    std::mem::forget(event_loop);

    Ok(gl)
}

pub fn run_ui_loop(ui_shared: SharedUi, width: u32, height: u32) -> Result<()> {
    use glutin::config::ConfigTemplateBuilder;
    use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
    use glutin::display::{GetGlDisplay, GlDisplay};
    use glutin::prelude::*;
    use glutin::surface::SwapInterval;
    use glutin_winit::{DisplayBuilder, GlWindow};
    use raw_window_handle::HasWindowHandle;
    use winit::dpi::PhysicalSize;
    use winit::event::{Event, WindowEvent};
    use winit::event_loop::{ControlFlow, EventLoop};
    use winit::window::WindowAttributes;

    info!(target: "hle::display", "Starting UI event loop (sailfish {width}x{height})");

    let event_loop = EventLoop::new().map_err(|e| Pixel1Error::Display(e.to_string()))?;

    let window_attrs = WindowAttributes::default()
        .with_inner_size(PhysicalSize::new(width, height))
        .with_title("Pixel1HLE – sailfish")
        .with_resizable(false);

    let template = ConfigTemplateBuilder::new().with_alpha_size(8);
    let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attrs));

    let (window, gl_config) = display_builder
        .build(&event_loop, template, |configs| {
            configs
                .reduce(|a, b| if b.num_samples() > a.num_samples() { b } else { a })
                .unwrap()
        })
        .map_err(|e| Pixel1Error::Display(format!("{e}")))?;

    let window = window.ok_or_else(|| Pixel1Error::Display("no window".into()))?;
    let raw_handle = window.window_handle().map_err(|e| Pixel1Error::Display(e.to_string()))?.as_raw();
    let gl_display = gl_config.display();

    let context_attribs = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(4, 5))))
        .build(Some(raw_handle));

    let not_current = gl_display.create_context(&gl_config, &context_attribs)
        .map_err(|e| Pixel1Error::Display(format!("{e}")))?;
    let attrs = GlWindow::build_surface_attributes(&window, Default::default())
        .map_err(|e| Pixel1Error::Display(format!("{e}")))?;
    let surface = gl_display.create_window_surface(&gl_config, &attrs)
        .map_err(|e| Pixel1Error::Display(format!("{e}")))?;
    let context = not_current.make_current(&surface)
        .map_err(|e| Pixel1Error::Display(format!("{e}")))?;

    let _ = surface.set_swap_interval(&context, SwapInterval::Wait(std::num::NonZeroU32::new(1).unwrap()));

    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            gl_display.get_proc_address(&std::ffi::CString::new(s).unwrap()) as *const _
        })
    };
    let gl = Arc::new(Mutex::new(gl));
    let _ = GL_CONTEXT.set(gl.clone());

    // egui_glow 0.29 API: EguiGlow::new takes (&EventLoop, glow::Context, ...)
    let gl_for_egui = {
        let guard = gl.lock();
        // We need a clone of the Context. glow::Context is not Clone by default
        // in all versions; create a second loader-based context sharing the same display.
        unsafe {
            glow::Context::from_loader_function(|s| {
                gl_display.get_proc_address(&std::ffi::CString::new(s).unwrap()) as *const _
            })
        }
    };

    let mut egui_glow = egui_glow::EguiGlow::new(&event_loop, gl_for_egui, None, None, false);
    let mut egui_winit = egui_winit::State::new(
        egui_glow.egui_ctx.clone(),
        egui::ViewportId::ROOT,
        &window,
        Some(window.scale_factor() as f32),
        None,
        None,
    );

    let start = Instant::now();
    let mut boot_timer: Option<Instant> = None;

    {
        let mut g = ui_shared.lock();
        g.available_apks = scan_apks();
        info!(target: "hle::display", "Found {} APK(s)", g.available_apks.len());
    }

    #[allow(deprecated)]
    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { event, .. } => {
                let is_ui = {
                    let g = ui_shared.lock();
                    !matches!(g.state, EmulatorState::GameRunning { .. })
                };

                if is_ui {
                    let response = egui_winit.on_window_event(&window, &event);
                    if response.consumed {
                        return;
                    }
                }

                match event {
                    WindowEvent::CloseRequested => elwt.exit(),
                    WindowEvent::RedrawRequested => {
                        let time = start.elapsed().as_secs_f64();
                        let state = ui_shared.lock().state.clone();

                        if let EmulatorState::BootAnimation { apk_name } = &state {
                            if boot_timer.is_none() {
                                boot_timer = Some(Instant::now());
                            }
                            if boot_timer.unwrap().elapsed().as_secs_f32() > 2.2 {
                                DisplaySurface::set_state(
                                    &ui_shared,
                                    EmulatorState::ApkLoading { apk_name: apk_name.clone() },
                                );
                                boot_timer = None;
                            }
                        }

                        {
                            let gl = gl.lock();
                            unsafe {
                                gl.clear_color(0.0, 0.0, 0.0, 1.0);
                                gl.clear(glow::COLOR_BUFFER_BIT);
                            }
                        }

                        match state {
                            EmulatorState::GameRunning { .. } => {}
                            _ => {
                                let _raw_input = egui_winit.take_egui_input(&window);
                                let full_output = egui_glow.run(elwt, |ctx| {
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
                                egui_winit.handle_platform_output(&window, full_output.platform_output);
                                egui_glow.paint(&window);
                            }
                        }

                        surface.swap_buffers(&context).ok();
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => {
                window.request_redraw();
            }
            _ => {}
        }
    }).map_err(|e| Pixel1Error::Display(format!("event loop: {e}")))?;

    Ok(())
}

fn scan_apks() -> Vec<crate::ui::ApkEntry> {
    let mut list = Vec::new();
    for dir in [".", "apps", "apks"] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) == Some("apk") {
                    let size_mb = e.metadata().map(|m| m.len() as f32 / 1_048_576.0).unwrap_or(0.0);
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
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
