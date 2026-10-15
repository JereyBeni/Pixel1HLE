//! Software-driven audio pipeline mimicking 3.5 mm jack + speaker

use crate::error::Result;
use log::info;

pub struct AudioPipeline {
    sample_rate: u32,
    channels: u16,
}

impl AudioPipeline {
    pub fn new() -> Result<Self> {
        info!("Audio: initialising software pipeline (44.1 kHz stereo)");
        Ok(Self {
            sample_rate: 44100,
            channels: 2,
        })
    }

    pub fn write_samples(&self, _pcm: &[i16]) {
        let _ = (self.sample_rate, self.channels);
    }
}
