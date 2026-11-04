//! Ported from plaits/dsp/fx/low_pass_gate.h: approximative low pass gate.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The two C++ overloads of `Process` are `process` (in place, floats) and `process_to_shorts`,
//! which writes the 16-bit output itself, every `stride`-th short.

use crate::stmlib::dsp::dsp::{cast_i32, clip16};
use crate::stmlib::dsp::filter::{FILTER_MODE_LOW_PASS, FREQUENCY_DIRTY, Svf};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;

#[derive(Default)]
pub struct LowPassGate {
    previous_gain_: f32,
    filter_: Svf,
}

impl LowPassGate {
    pub fn init(&mut self) {
        self.previous_gain_ = 0.0;
        self.filter_.init();
    }

    pub fn process(
        &mut self,
        gain: f32,
        frequency: f32,
        hf_bleed: f32,
        in_out: &mut [f32],
        size: usize,
    ) {
        let mut gain_modulation = ParameterInterpolator::new(&mut self.previous_gain_, gain, size);
        self.filter_.set_f_q::<FREQUENCY_DIRTY>(frequency, 0.4);
        for sample in &mut in_out[..size] {
            let s = *sample * gain_modulation.next();
            let lp = self.filter_.process::<FILTER_MODE_LOW_PASS>(s);
            *sample = lp + (s - lp) * hf_bleed;
        }
    }

    /// `Process(gain, frequency, hf_bleed, const float* in, short* out, size_t size, size_t
    /// stride)`: `out[0]`, `out[stride]`, … take the 16-bit samples.
    #[allow(clippy::too_many_arguments)]
    pub fn process_to_shorts(
        &mut self,
        gain: f32,
        frequency: f32,
        hf_bleed: f32,
        in_: &[f32],
        out: &mut [i16],
        size: usize,
        stride: usize,
    ) {
        let mut gain_modulation = ParameterInterpolator::new(&mut self.previous_gain_, gain, size);
        self.filter_.set_f_q::<FREQUENCY_DIRTY>(frequency, 0.4);
        for (i, x) in in_[..size].iter().enumerate() {
            let s = *x * gain_modulation.next();
            let lp = self.filter_.process::<FILTER_MODE_LOW_PASS>(s);
            out[i * stride] = clip16(1i32.wrapping_add(cast_i32(lp + (s - lp) * hf_bleed))) as i16;
        }
    }
}
