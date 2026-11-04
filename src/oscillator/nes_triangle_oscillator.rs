//! Ported from plaits/dsp/oscillator/nes_triangle_oscillator.h: triangle waveform approximated
//! by 16 discrete steps.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The class template's `num_bits` is a const generic, defaulting to 5 as in the C++.

use crate::cpp::min;
use crate::stmlib::dsp::dsp::cast_i32;
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::polyblep::{
    next_blep_sample, next_integrated_blep_sample, this_blep_sample, this_integrated_blep_sample,
};
use crate::stmlib::stmlib::constrain;

#[derive(Default)]
pub struct NESTriangleOscillator<const NUM_BITS: i32 = 5> {
    // Oscillator state.
    phase_: f32,
    next_sample_: f32,
    step_: i32,
    ascending_: bool,

    // For interpolation of parameters.
    frequency_: f32,
}

impl<const NUM_BITS: i32> NESTriangleOscillator<NUM_BITS> {
    #[inline]
    pub fn init(&mut self) {
        self.phase_ = 0.0;
        self.step_ = 0;
        self.ascending_ = true;
        self.next_sample_ = 0.0;
        self.frequency_ = 0.001;
    }

    #[inline]
    pub fn render(&mut self, mut frequency: f32, out: &mut [f32], size: usize) {
        // Compute all constants needed to scale the waveform and its
        // discontinuities.
        let num_steps: i32 = 1 << NUM_BITS;
        let half = num_steps / 2;
        let top = if num_steps != 2 { num_steps - 1 } else { 2 };
        let num_steps_f = num_steps as f32;
        let scale = if num_steps != 2 {
            4.0 / (top - 1) as f32
        } else {
            2.0
        };

        frequency = min(frequency, 0.25);

        let mut fm = ParameterInterpolator::new(&mut self.frequency_, frequency, size);

        let mut next_sample = self.next_sample_;
        for out_sample in &mut out[..size] {
            let frequency = fm.next();
            self.phase_ += frequency;

            // Compute the point at which we transition between the "full resolution"
            // NES triangle, and a naive band-limited triangle.
            let mut fade_to_tri = (frequency - 0.5 / num_steps_f) * 2.0 * num_steps_f;
            constrain(&mut fade_to_tri, 0.0, 1.0);

            let nes_gain = 1.0 - fade_to_tri;
            let tri_gain = fade_to_tri * 2.0 / scale;

            let mut this_sample = next_sample;
            next_sample = 0.0;

            // Handle the discontinuity at the top of the naive triangle.
            if self.ascending_ && self.phase_ >= 0.5 {
                let discontinuity = 4.0 * frequency * tri_gain;
                // `if (discontinuity)`: a float is true unless it is zero.
                if discontinuity != 0.0 {
                    let t = (self.phase_ - 0.5) / frequency;
                    this_sample -= this_integrated_blep_sample(t) * discontinuity;
                    next_sample -= next_integrated_blep_sample(t) * discontinuity;
                }
                self.ascending_ = false;
            }

            let mut next_step = cast_i32(self.phase_ * num_steps_f);
            if next_step != self.step_ {
                let mut wrap = false;
                if next_step >= num_steps {
                    self.phase_ -= 1.0;
                    next_step -= num_steps;
                    wrap = true;
                }

                let mut discontinuity = if next_step < half { 1.0 } else { -1.0 };
                if num_steps == 2 {
                    discontinuity = -discontinuity;
                } else if next_step == 0 || next_step == half {
                    discontinuity = 0.0;
                }

                // Handle the discontinuity at each step of the NES triangle.
                discontinuity *= nes_gain;
                if discontinuity != 0.0 {
                    let frac = self.phase_ * num_steps_f - next_step as f32;
                    let t = frac / (frequency * num_steps_f);
                    this_sample += this_blep_sample(t) * discontinuity;
                    next_sample += next_blep_sample(t) * discontinuity;
                }

                // Handle the discontinuity at the bottom of the naive triangle.
                if wrap {
                    let discontinuity = 4.0 * frequency * tri_gain;
                    if discontinuity != 0.0 {
                        let t = self.phase_ / frequency;
                        this_sample += this_integrated_blep_sample(t) * discontinuity;
                        next_sample += next_integrated_blep_sample(t) * discontinuity;
                    }
                    self.ascending_ = true;
                }
            }
            self.step_ = next_step;

            // Contribution from NES triangle.
            next_sample += nes_gain
                * (if self.step_ < half {
                    self.step_
                } else {
                    top - self.step_
                }) as f32;

            // Contribution from naive triangle.
            next_sample += tri_gain
                * (if self.phase_ < 0.5 {
                    2.0 * self.phase_
                } else {
                    2.0 - 2.0 * self.phase_
                });

            *out_sample = this_sample * scale - 1.0;
        }
        self.next_sample_ = next_sample;
    }
}
