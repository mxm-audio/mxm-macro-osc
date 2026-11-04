//! Ported from plaits/dsp/oscillator/z_oscillator.h: sinewave multiplied by and sync'ed to a
//! carrier.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `Z` reads no member, so it is an associated function, `z`: the parameter interpolators
//! borrow the oscillator's fields while it runs.

use crate::oscillator::oscillator::K_MAX_FREQUENCY;
use crate::oscillator::sine_oscillator::sine;
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::polyblep::{next_blep_sample, this_blep_sample};

#[derive(Default)]
pub struct ZOscillator {
    // Oscillator state.
    carrier_phase_: f32,
    discontinuity_phase_: f32,
    formant_phase_: f32,
    next_sample_: f32,

    // For interpolation of parameters.
    carrier_frequency_: f32,
    formant_frequency_: f32,
    carrier_shape_: f32,
    mode_: f32,
}

impl ZOscillator {
    pub fn init(&mut self) {
        self.carrier_phase_ = 0.0;
        self.discontinuity_phase_ = 0.0;
        self.formant_phase_ = 0.0;
        self.next_sample_ = 0.0;

        self.carrier_frequency_ = 0.0;
        self.formant_frequency_ = 0.0;
        self.carrier_shape_ = 0.0;
        self.mode_ = 0.0;
    }

    pub fn render(
        &mut self,
        mut carrier_frequency: f32,
        mut formant_frequency: f32,
        carrier_shape: f32,
        mode: f32,
        out: &mut [f32],
        size: usize,
    ) {
        if carrier_frequency >= K_MAX_FREQUENCY * 0.5 {
            carrier_frequency = K_MAX_FREQUENCY * 0.5;
        }
        if formant_frequency >= K_MAX_FREQUENCY {
            formant_frequency = K_MAX_FREQUENCY;
        }

        let mut carrier_frequency_modulation =
            ParameterInterpolator::new(&mut self.carrier_frequency_, carrier_frequency, size);
        let mut formant_frequency_modulation =
            ParameterInterpolator::new(&mut self.formant_frequency_, formant_frequency, size);
        let mut carrier_shape_modulation =
            ParameterInterpolator::new(&mut self.carrier_shape_, carrier_shape, size);
        let mut mode_modulation = ParameterInterpolator::new(&mut self.mode_, mode, size);

        let mut next_sample = self.next_sample_;

        for sample in &mut out[..size] {
            let mut this_sample = next_sample;
            next_sample = 0.0;

            let f0 = carrier_frequency_modulation.next();
            let f1 = formant_frequency_modulation.next();

            self.discontinuity_phase_ += 2.0 * f0;
            self.carrier_phase_ += f0;
            let reset = self.discontinuity_phase_ >= 1.0;

            if reset {
                self.discontinuity_phase_ -= 1.0;
                let reset_time = self.discontinuity_phase_ / (2.0 * f0);

                let carrier_phase_before = if self.carrier_phase_ >= 1.0 { 1.0 } else { 0.5 };
                let carrier_phase_after = if self.carrier_phase_ >= 1.0 { 0.0 } else { 0.5 };
                let before = Self::z(
                    carrier_phase_before,
                    1.0,
                    self.formant_phase_ + (1.0 - reset_time) * f1,
                    carrier_shape_modulation.subsample(1.0 - reset_time),
                    mode_modulation.subsample(1.0 - reset_time),
                );

                let after = Self::z(
                    carrier_phase_after,
                    0.0,
                    0.0,
                    carrier_shape_modulation.subsample(1.0),
                    mode_modulation.subsample(1.0),
                );

                let discontinuity = after - before;
                this_sample += discontinuity * this_blep_sample(reset_time);
                next_sample += discontinuity * next_blep_sample(reset_time);
                self.formant_phase_ = reset_time * f1;

                if self.carrier_phase_ > 1.0 {
                    self.carrier_phase_ = self.discontinuity_phase_ * 0.5;
                }
            } else {
                self.formant_phase_ += f1;
                if self.formant_phase_ >= 1.0 {
                    self.formant_phase_ -= 1.0;
                }
            }

            if self.carrier_phase_ >= 1.0 {
                self.carrier_phase_ -= 1.0;
            }

            next_sample += Self::z(
                self.carrier_phase_,
                self.discontinuity_phase_,
                self.formant_phase_,
                carrier_shape_modulation.next(),
                mode_modulation.next(),
            );
            *sample = this_sample;
        }

        self.next_sample_ = next_sample;
    }

    #[inline]
    fn z(c: f32, d: f32, f: f32, mut shape: f32, mode: f32) -> f32 {
        let mut ramp_down = 0.5 * (1.0 + sine(0.5 * d + 0.25));

        let offset;
        let phase_shift;
        if mode < 0.333 {
            offset = 1.0;
            phase_shift = 0.25 + mode * 1.50;
        } else if mode < 0.666 {
            phase_shift = 0.7495 - (mode - 0.33) * 0.75;
            offset = -sine(phase_shift);
        } else {
            phase_shift = 0.7495 - (mode - 0.33) * 0.75;
            offset = 0.001;
        }

        let discontinuity = sine(f + phase_shift);
        let contour = if shape < 0.5 {
            shape *= 2.0;
            if c >= 0.5 {
                ramp_down *= shape;
            }
            1.0 + (sine(c + 0.25) - 1.0) * shape
        } else {
            sine(c + shape * 0.5)
        };
        (ramp_down * (offset + discontinuity) - offset) * contour
    }
}
