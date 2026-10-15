//! Display surface + Glow / Glutin + egui (desktop + Android)

use crate::error::{Pixel1Error, Result};
use crate::logging;
use crate::ui::{AppPicker, BootAnimation, EmulatorState, LoadingScreen, SharedUi, TerminalPanel};
use glow::HasContext;
use log::{info, warn};
use once_cell::sync::OnceCell;
use std::sync::Arc;
use std::time::Instant;

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
        info!(target: "hle::display", "Display: {width}x{height} @{refresh_hz}Hz");
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
    }

    pub fn set_state(shared: &SharedUi, state: EmulatorState) {
        let mut g = shared.lock();
        info!(target: "hle::display", "State → {:?}", state);
        g.state = state;
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
                let _ = GL_CONTEXT.set(Arc::clone(&gl));
                let egui_glow =
                    egui_glow::EguiGlow::new(event_loop, Arc::clone(&gl), None, None, false);
                {
                    let mut g = self.ui_shared.lock();
                    g.available_apks = scan_apks();
                }
                self.gl = Some(gl);
                self.egui_glow = Some(egui_glow);
                self.gl_window = Some(gl_window);
            }
            Err(e) => {
                log::error!(target: "hle::display", "display create failed: {e}");
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

        if let winit::event::WindowEvent::KeyboardInput {
            event: key_event, ..
        } = &event
        {
            if key_event.state == winit::event::ElementState::Pressed {
                if let winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Backquote) =
                    key_event.physical_key
                {
                    let mut g = self.ui_shared.lock();
                    g.terminal_open = !g.terminal_open;
                }
            }
        }

        let is_ui = {
            let g = self.ui_shared.lock();
            !matches!(g.state, EmulatorState::GameRunning { .. }) || g.terminal_open
        };
        if is_ui {
            let repaint = egui_glow.on_window_event(&gl_window.window, &event);
            if repaint.repaint {
                gl_window.window.request_redraw();
            }
        }

        match event {
            winit::event::WindowEvent::CloseRequested => event_loop.exit(),
            winit::event::WindowEvent::RedrawRequested => {
                let time = self.start.elapsed().as_secs_f64();
                let (state, terminal_open) = {
                    let g = self.ui_shared.lock();
                    (g.state.clone(), g.terminal_open)
                };

                if let EmulatorState::BootAnimation { apk_name } = &state {
                    if self.boot_timer.is_none() {
                        self.boot_timer = Some(Instant::now());
                    }
                    if self
                        .boot_timer
                        .map(|t| t.elapsed().as_secs_f32() > 2.2)
                        .unwrap_or(false)
                    {
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
                    gl.clear_color(0.05, 0.05, 0.08, 1.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                }

                let paint_ui =
                    !matches!(state, EmulatorState::GameRunning { .. }) || terminal_open;

                if paint_ui {
                    let ui_shared = self.ui_shared.clone();
                    egui_glow.run(&gl_window.window, |ctx| {
                        if let Some(log_state) = logging::global_log_state() {
                            let mut open = ui_shared.lock().terminal_open;
                            TerminalPanel::ui(ctx, log_state, &mut open);
                            ui_shared.lock().terminal_open = open;
                        }
                        let st = ui_shared.lock().state.clone();
                        match st {
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

/// Scale portrait phone resolution so it fits a typical desktop monitor.
fn fit_portrait_window(width: u32, height: u32) -> (u32, u32) {
    let mut w = width.max(320);
    let mut h = height.max(480);
    // Prefer portrait: if landscape profile slipped in, swap
    if w > h {
        std::mem::swap(&mut w, &mut h);
    }
    // Cap height for desktop windows (~900px)
    const MAX_H: u32 = 900;
    if h > MAX_H {
        let scale = MAX_H as f32 / h as f32;
        h = MAX_H;
        w = ((w as f32) * scale).round() as u32;
    }
    (w.max(320), h.max(480))
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

    let (width, height) = fit_portrait_window(width, height);
    info!(target: "hle::display", "Window size (portrait): {width}x{height}");

    let window_attrs = WindowAttributes::default()
        .with_inner_size(PhysicalSize::new(width, height))
        .with_title("Pixel1HLE – sailfish (portrait)")
        .with_resizable(true);

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
                .expect("no GL configs")
        })
        .map_err(|e| Pixel1Error::Display(format!("{e}")))?;

    let window = window.ok_or_else(|| Pixel1Error::Display("no window".into()))?;
    let raw_handle = window
        .window_handle()
        .map_err(|e| Pixel1Error::Display(e.to_string()))?
        .as_raw();
    let gl_display = gl_config.display();

    let attempts = [
        ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(4, 5)))),
        ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3)))),
        ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(Some(Version::new(3, 0)))),
        ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(Some(Version::new(2, 0)))),
        ContextAttributesBuilder::new(),
    ];

    let mut last_err = String::from("no context");
    let mut not_current = None;
    for builder in attempts {
        let attribs = builder.build(Some(raw_handle));
        match unsafe { gl_display.create_context(&gl_config, &attribs) } {
            Ok(ctx) => {
                not_current = Some(ctx);
                break;
            }
            Err(e) => {
                last_err = format!("{e}");
                warn!(target: "hle::display", "GL attempt failed: {e}");
            }
        }
    }
    let not_current = not_current
        .ok_or_else(|| Pixel1Error::Display(format!("create_context: {last_err}")))?;

    let attrs = GlWindow::build_surface_attributes(&window, Default::default())
        .map_err(|e| Pixel1Error::Display(format!("surface: {e}")))?;
    let gl_surface = unsafe {
        gl_display
            .create_window_surface(&gl_config, &attrs)
            .map_err(|e| Pixel1Error::Display(format!("window_surface: {e}")))?
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
            let c = std::ffi::CString::new(s).unwrap_or_default();
            gl_display.get_proc_address(&c) as *const _
        })
    };
    unsafe {
        gl.viewport(0, 0, width as i32, height as i32);
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
    info!(target: "hle::display", "UI loop desktop {width}x{height}");
    let event_loop = winit::event_loop::EventLoop::new()
        .map_err(|e| Pixel1Error::Display(e.to_string()))?;
    let mut app = AppState::new(ui_shared, width, height);
    event_loop
        .run_app(&mut app)
        .map_err(|e| Pixel1Error::Display(format!("{e}")))?;
    Ok(())
}

#[cfg(target_os = "android")]
pub fn run_ui_loop_android(
    ui_shared: SharedUi,
    width: u32,
    height: u32,
    android_app: Option<android_activity::AndroidApp>,
) -> Result<()> {
    use winit::platform::android::EventLoopBuilderExtAndroid;

    info!(target: "hle::display", "UI loop Android NativeActivity {width}x{height}");
    let app = android_app.ok_or_else(|| {
        Pixel1Error::Display("AndroidApp handle missing".into())
    })?;

    let event_loop = winit::event_loop::EventLoop::builder()
        .with_android_app(app)
        .build()
        .map_err(|e| Pixel1Error::Display(e.to_string()))?;

    let mut state = AppState::new(ui_shared, width, height);
    event_loop
        .run_app(&mut state)
        .map_err(|e| Pixel1Error::Display(format!("{e}")))?;
    Ok(())
}

fn scan_apks() -> Vec<crate::ui::ApkEntry> {
    let mut list = Vec::new();
    let mut roots = vec![
        std::path::PathBuf::from("."),
        std::path::PathBuf::from("apps"),
    ];
    #[cfg(target_os = "android")]
    {
        roots.push(std::path::PathBuf::from("/sdcard/Download"));
        roots.push(std::path::PathBuf::from("/storage/emulated/0/Download"));
        roots.push(std::path::PathBuf::from("/data/data/com.pixel.hle/files/apps"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
            roots.push(dir.join("apps"));
        }
    }
    for dir in roots {
        if let Ok(entries) = std::fs::read_dir(&dir) {
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
