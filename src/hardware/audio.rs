//! Software-driven audio pipeline mimicking 3.5 mm jack + speaker

use crate::error::{Pixel1Error, Result};
use log::info;

pub struct AudioPipeline {
    // cpal stream would live here
    sample_rate: u32,
    channels: u16,
}

impl AudioPipeline {
    pub fn new() -> Result<Self> {
        info!("Audio: initialising software pipeline (44.1 kHz stereo)");
        // Real implementation:
        // let host = cpal::default_host();
        // let device = host.default_output_device()…
        // let config = …;
        // let stream = device.build_output_stream(…)?;

        Ok(Self {
            sample_rate: 44100,
            channels: 2,
        })
    }

    pub fn write_samples(&self, _pcm: &[i16]) {
        // push into the cpal ring buffer
    }
}
