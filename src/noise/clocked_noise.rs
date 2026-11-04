//! Ported from plaits/dsp/noise/clocked_noise.h: noise processed by a sample and hold running
//! at a target frequency.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! It draws from the voice's `Random` on every sample, whether or not the clock ticks.

use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::polyblep::{next_blep_sample, this_blep_sample};
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct ClockedNoise {
    // Oscillator state.
    phase_: f32,
    sample_: f32,
    next_sample_: f32,

    // For interpolation of parameters.
    frequency_: f32,
}

impl ClockedNoise {
    pub fn init(&mut self) {
        self.phase_ = 0.0;
        self.sample_ = 0.0;
        self.next_sample_ = 0.0;
        self.frequency_ = 0.001;
    }

    pub fn render(
        &mut self,
        sync: bool,
        mut frequency: f32,
        out: &mut [f32],
        size: usize,
        random: &mut Random,
    ) {
        constrain(&mut frequency, 0.0, 1.0);

        let mut fm = ParameterInterpolator::new(&mut self.frequency_, frequency, size);

        let mut next_sample = self.next_sample_;
        let mut sample = self.sample_;

        if sync {
            self.phase_ = 1.0;
        }

        for out in &mut out[..size] {
            let mut this_sample = next_sample;
            next_sample = 0.0;

            let frequency = fm.next();
            let raw_sample = random.get_float() * 2.0 - 1.0;
            let mut raw_amount = 4.0 * (frequency - 0.25);
            constrain(&mut raw_amount, 0.0, 1.0);

            self.phase_ += frequency;

            if self.phase_ >= 1.0 {
                self.phase_ -= 1.0;
                let t = self.phase_ / frequency;
                let new_sample = raw_sample;
                let discontinuity = new_sample - sample;
                this_sample += discontinuity * this_blep_sample(t);
                next_sample += discontinuity * next_blep_sample(t);
                sample = new_sample;
            }
            next_sample += sample;
            *out = this_sample + raw_amount * (raw_sample - this_sample);
        }
        self.next_sample_ = next_sample;
        self.sample_ = sample;
    }
}
