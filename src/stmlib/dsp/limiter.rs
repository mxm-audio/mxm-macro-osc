//! Ported from stmlib/dsp/limiter.h: limiter.
//!
//! Copyright 2015 Emilie Gillet. MIT licence (`NOTICE.md`).

use super::dsp::slope;

#[derive(Default)]
pub struct Limiter {
    peak_: f32,
}

impl Limiter {
    pub fn init(&mut self) {
        self.peak_ = 0.5;
    }

    /// `Process(float pre_gain, float* in_out, size_t size)`: the first `size` samples.
    pub fn process(&mut self, pre_gain: f32, in_out: &mut [f32], size: usize) {
        for sample in &mut in_out[..size] {
            let s = *sample * pre_gain;
            slope(&mut self.peak_, s.abs(), 0.05, 0.00002);
            let gain = if self.peak_ <= 1.0 {
                1.0
            } else {
                1.0 / self.peak_
            };
            *sample = s * gain * 0.8;
        }
    }
}
