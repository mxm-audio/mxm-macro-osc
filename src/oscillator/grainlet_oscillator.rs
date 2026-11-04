//! Ported from plaits/dsp/oscillator/grainlet_oscillator.h: a phase-distortd single cycle sine
//! * another continuously running sine, the whole thing synced to a main oscillator.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `Carrier` and `Grainlet` read no member, so they are associated functions: the parameter
//! interpolators borrow the oscillator's fields while it runs.

use crate::oscillator::oscillator::K_MAX_FREQUENCY;
use crate::oscillator::sine_oscillator::sine;
use crate::stmlib::dsp::dsp::make_integral_fractional;
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::polyblep::{next_blep_sample, this_blep_sample};

#[derive(Default)]
pub struct GrainletOscillator {
    // Oscillator state.
    carrier_phase_: f32,
    formant_phase_: f32,
    next_sample_: f32,

    // For interpolation of parameters.
    carrier_frequency_: f32,
    formant_frequency_: f32,
    carrier_shape_: f32,
    carrier_bleed_: f32,
}

impl GrainletOscillator {
    pub fn init(&mut self) {
        self.carrier_phase_ = 0.0;
        self.formant_phase_ = 0.0;
        self.next_sample_ = 0.0;

        self.carrier_frequency_ = 0.0;
        self.formant_frequency_ = 0.0;
        self.carrier_shape_ = 0.0;
        self.carrier_bleed_ = 0.0;
    }

    pub fn render(
        &mut self,
        mut carrier_frequency: f32,
        mut formant_frequency: f32,
        carrier_shape: f32,
        carrier_bleed: f32,
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
        let mut carrier_bleed_modulation =
            ParameterInterpolator::new(&mut self.carrier_bleed_, carrier_bleed, size);

        let mut next_sample = self.next_sample_;

        for sample in &mut out[..size] {
            let mut this_sample = next_sample;
            next_sample = 0.0;

            let f0 = carrier_frequency_modulation.next();
            let f1 = formant_frequency_modulation.next();

            self.carrier_phase_ += f0;
            let reset = self.carrier_phase_ >= 1.0;

            if reset {
                self.carrier_phase_ -= 1.0;
                let reset_time = self.carrier_phase_ / f0;
                let before = Self::grainlet(
                    1.0,
                    self.formant_phase_ + (1.0 - reset_time) * f1,
                    carrier_shape_modulation.subsample(1.0 - reset_time),
                    carrier_bleed_modulation.subsample(1.0 - reset_time),
                );

                let after = Self::grainlet(
                    0.0,
                    0.0,
                    carrier_shape_modulation.subsample(1.0),
                    carrier_bleed_modulation.subsample(1.0),
                );

                let discontinuity = after - before;
                this_sample += discontinuity * this_blep_sample(reset_time);
                next_sample += discontinuity * next_blep_sample(reset_time);
                self.formant_phase_ = reset_time * f1;
            } else {
                self.formant_phase_ += f1;
                if self.formant_phase_ >= 1.0 {
                    self.formant_phase_ -= 1.0;
                }
            }

            next_sample += Self::grainlet(
                self.carrier_phase_,
                self.formant_phase_,
                carrier_shape_modulation.next(),
                carrier_bleed_modulation.next(),
            );
            *sample = this_sample;
        }

        self.next_sample_ = next_sample;
    }

    #[inline]
    fn carrier(mut phase: f32, mut shape: f32) -> f32 {
        shape *= 3.0;
        let (shape_integral, shape_fractional) = make_integral_fractional(shape);
        let mut t = 1.0 - shape_fractional;

        if shape_integral == 0 {
            phase *= 1.0 + t * t * t * 15.0;
            if phase >= 1.0 {
                phase = 1.0;
            }
            phase += 0.75;
        } else if shape_integral == 1 {
            let breakpoint = 0.001 + 0.499 * t * t * t;
            if phase < breakpoint {
                phase *= 0.5 / breakpoint;
            } else {
                phase = 0.5 + (phase - breakpoint) * 0.5 / (1.0 - breakpoint);
            }
            phase += 0.75;
        } else {
            t = 1.0 - t;
            phase = 0.25 + phase * (0.5 + t * t * t * 14.5);
            if phase >= 0.75 {
                phase = 0.75;
            }
        }
        (sine(phase) + 1.0) * 0.25
    }

    #[inline]
    fn grainlet(carrier_phase: f32, formant_phase: f32, shape: f32, bleed: f32) -> f32 {
        let carrier = Self::carrier(carrier_phase, shape);
        let formant = sine(formant_phase);
        carrier * (formant + bleed) / (1.0 + bleed)
    }
}
