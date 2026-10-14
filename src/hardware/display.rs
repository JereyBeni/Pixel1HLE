//! Locked 1920×1080 @ 441 ppi @ 60 Hz canvas
//! Host mouse/trackpad → multi-touch coordinates

use crate::error::{Pixel1Error, Result};
use log::info;
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

pub struct DisplaySurface {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    // In a full implementation we would hold the winit Window + glutin context here.
    // For the architectural skeleton we keep the essential state.
    _window_placeholder: (),
}

impl DisplaySurface {
    pub fn new(width: u32, height: u32, refresh_hz: u32) -> Result<Self> {
        info!("Display: creating {width}x{height} @{refresh_hz}Hz surface (sailfish)");

        // Real code would do:
        // let event_loop = EventLoop::new().unwrap();
        // let window = WindowBuilder::new()
        //     .with_inner_size(PhysicalSize::new(width, height))
        //     .with_title("Pixel 1 HLE")
        //     .with_resizable(false)
        //     .build(&event_loop)
        //     .map_err(|e| Pixel1Error::Display(e.to_string()))?;
        // + glutin context creation…

        Ok(Self {
            width,
            height,
            refresh_hz,
            _window_placeholder: (),
        })
    }

    /// Map host cursor position (0..1 normalised) to multi-touch slot
    pub fn inject_touch(&self, slot: u32, x_norm: f32, y_norm: f32, down: bool) {
        let x = (x_norm * self.width as f32) as u32;
        let y = (y_norm * self.height as f32) as u32;
        // In full HLE this would write into the InputReader / EventHub
        // that the Android InputDispatcher consumes.
        log::trace!("Touch slot={slot} ({x},{y}) down={down}");
    }
}
