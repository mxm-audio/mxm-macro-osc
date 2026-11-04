//! Ported from plaits/dsp/oscillator/harmonic_oscillator.h: harmonic oscillator based on
//! Chebyshev polynomials. Works well for a small number of harmonics. For the higher order
//! harmonics, we need to reinitialize the recurrence by computing two high harmonics.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! Both template parameters are const generics: `HarmonicOscillator<NUM_HARMONICS>` and
//! `render::<FIRST_HARMONIC_INDEX>`. The C++ default-constructs its array of interpolators and
//! `Init`s each; here each is made with its state, which is the same.

use crate::oscillator::sine_oscillator::{sine, sine_no_wrap};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;

pub struct HarmonicOscillator<const NUM_HARMONICS: usize> {
    // Oscillator state.
    phase_: f32,

    // For interpolation of parameters.
    frequency_: f32,
    amplitude_: [f32; NUM_HARMONICS],
}

impl<const NUM_HARMONICS: usize> Default for HarmonicOscillator<NUM_HARMONICS> {
    fn default() -> Self {
        Self {
            phase_: 0.0,
            frequency_: 0.0,
            amplitude_: [0.0; NUM_HARMONICS],
        }
    }
}

impl<const NUM_HARMONICS: usize> HarmonicOscillator<NUM_HARMONICS> {
    pub fn init(&mut self) {
        self.phase_ = 0.0;
        self.frequency_ = 0.0;
        for amplitude in self.amplitude_.iter_mut() {
            *amplitude = 0.0;
        }
    }

    /// `amplitudes` holds `NUM_HARMONICS` values.
    pub fn render<const FIRST_HARMONIC_INDEX: i32>(
        &mut self,
        mut frequency: f32,
        amplitudes: &[f32],
        out: &mut [f32],
        size: usize,
    ) {
        if frequency >= 0.5 {
            frequency = 0.5;
        }

        let mut fm = ParameterInterpolator::new(&mut self.frequency_, frequency, size);

        let mut i = 0;
        let mut am: [ParameterInterpolator<'_>; NUM_HARMONICS] =
            self.amplitude_.each_mut().map(|state| {
                let mut f = frequency * FIRST_HARMONIC_INDEX.wrapping_add(i as i32) as f32;
                if f >= 0.5 {
                    f = 0.5;
                }
                let interpolator =
                    ParameterInterpolator::new(state, amplitudes[i] * (1.0 - f * 2.0), size);
                i += 1;
                interpolator
            });

        for sample in &mut out[..size] {
            self.phase_ += fm.next();
            if self.phase_ >= 1.0 {
                self.phase_ -= 1.0;
            }
            let two_x = 2.0 * sine_no_wrap(self.phase_);
            let mut previous;
            let mut current;
            if FIRST_HARMONIC_INDEX == 1 {
                previous = 1.0;
                current = two_x * 0.5;
            } else {
                let k = FIRST_HARMONIC_INDEX as f32;
                previous = sine(self.phase_ * (k - 1.0) + 0.25);
                current = sine(self.phase_ * k);
            }

            let mut sum = 0.0;
            for a in am.iter_mut() {
                sum += a.next() * current;
                let temp = current;
                current = two_x * current - previous;
                previous = temp;
            }
            if FIRST_HARMONIC_INDEX == 1 {
                *sample = sum;
            } else {
                *sample += sum;
            }
        }
    }
}
