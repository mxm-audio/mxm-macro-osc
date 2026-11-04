//! Ported from plaits/dsp/drums/synthetic_snare_drum.h: naive snare drum model (two modulated
//! oscillators + filtered noise).
//!
//! Uses a few magic numbers taken from the 909 schematics:
//! - Ratio between the two modes of the drum set to 1.47.
//! - Funky coupling between the two modes.
//! - Noise coloration filters and envelope shapes for the snare.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `DistortedSine` reads no member, so it is an associated function while the interpolator
//! holds `sustain_gain_`. The C++ loop is `while (size--)` and its drum envelope tests
//! `size & 1` after the decrement: the port counts `size` down the same way.

use crate::cpp::min;
use crate::dsp::K_SAMPLE_RATE;
use crate::stmlib::dsp::dsp::{cast_i32, sqrt};
use crate::stmlib::dsp::filter::{
    FILTER_MODE_HIGH_PASS, FILTER_MODE_LOW_PASS, FREQUENCY_FAST, OnePole, Svf,
};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct SyntheticSnareDrum {
    phase_: [f32; 2],
    drum_amplitude_: f32,
    snare_amplitude_: f32,
    fm_: f32,
    sustain_gain_: f32,
    hold_counter_: i32,

    drum_lp_: OnePole,
    snare_hp_: OnePole,
    snare_lp_: Svf,
}

impl SyntheticSnareDrum {
    pub fn init(&mut self) {
        self.phase_[0] = 0.0;
        self.phase_[1] = 0.0;
        self.drum_amplitude_ = 0.0;
        self.snare_amplitude_ = 0.0;
        self.fm_ = 0.0;
        self.hold_counter_ = 0;
        self.sustain_gain_ = 0.0;

        self.drum_lp_.init();
        self.snare_hp_.init();
        self.snare_lp_.init();
    }

    #[inline]
    fn distorted_sine(phase: f32) -> f32 {
        let triangle = (if phase < 0.5 { phase } else { 1.0 - phase }) * 4.0 - 1.3;
        2.0 * triangle / (1.0 + triangle.abs())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        sustain: bool,
        trigger: bool,
        accent: f32,
        f0: f32,
        mut fm_amount: f32,
        decay: f32,
        mut snappy: f32,
        out: &mut [f32],
        size: usize,
        random: &mut Random,
    ) {
        let decay_xt = decay * (1.0 + decay * (decay - 1.0));
        fm_amount *= fm_amount;
        let drum_decay = 1.0
            - 1.0 / (0.015 * K_SAMPLE_RATE)
                * semitones_to_ratio(-decay_xt * 72.0 - fm_amount * 12.0 + snappy * 7.0);
        let snare_decay =
            1.0 - 1.0 / (0.01 * K_SAMPLE_RATE) * semitones_to_ratio(-decay * 60.0 - snappy * 7.0);
        let fm_decay = 1.0 - 1.0 / (0.007 * K_SAMPLE_RATE);

        snappy = snappy * 1.1 - 0.05;
        constrain(&mut snappy, 0.0, 1.0);

        let drum_level = sqrt(1.0 - snappy);
        let snare_level = sqrt(snappy);

        let snare_f_min = min(10.0 * f0, 0.5);
        let snare_f_max = min(35.0 * f0, 0.5);

        self.snare_hp_.set_f::<FREQUENCY_FAST>(snare_f_min);
        self.snare_lp_
            .set_f_q::<FREQUENCY_FAST>(snare_f_max, 0.5 + 2.0 * snappy);
        self.drum_lp_.set_f::<FREQUENCY_FAST>(3.0 * f0);

        if trigger {
            self.drum_amplitude_ = 0.3 + 0.7 * accent;
            self.snare_amplitude_ = self.drum_amplitude_;
            self.fm_ = 1.0;
            self.phase_[1] = 0.0;
            self.phase_[0] = self.phase_[1];
            self.hold_counter_ = cast_i32((0.04 + decay * 0.03) * K_SAMPLE_RATE);
        }

        let mut sustain_gain =
            ParameterInterpolator::new(&mut self.sustain_gain_, accent * decay, size);
        let mut size = size;
        let mut out_index = 0;
        while size != 0 {
            size -= 1;
            if sustain {
                self.snare_amplitude_ = sustain_gain.next();
                self.drum_amplitude_ = self.snare_amplitude_;
                self.fm_ = 0.0;
            } else {
                // Compute all D envelopes.
                // The envelope for the drum has a very long tail.
                // The envelope for the snare has a "hold" stage which lasts between
                // 40 and 70 ms
                self.drum_amplitude_ *= if self.drum_amplitude_ > 0.03 || (size & 1) == 0 {
                    drum_decay
                } else {
                    1.0
                };
                if self.hold_counter_ != 0 {
                    self.hold_counter_ -= 1;
                } else {
                    self.snare_amplitude_ *= snare_decay;
                }
                self.fm_ *= fm_decay;
            }

            // The 909 circuit has a funny kind of oscillator coupling - the signal
            // leaving Q40's collector and resetting all oscillators allow some
            // intermodulation.
            let mut reset_noise = 0.0;
            let mut reset_noise_amount = (0.125 - f0) * 8.0;
            constrain(&mut reset_noise_amount, 0.0, 1.0);
            reset_noise_amount *= reset_noise_amount;
            reset_noise_amount *= fm_amount;
            reset_noise += if self.phase_[0] > 0.5 { -1.0 } else { 1.0 };
            reset_noise += if self.phase_[1] > 0.5 { -1.0 } else { 1.0 };
            reset_noise *= reset_noise_amount * 0.025;

            let f = f0 * (1.0 + fm_amount * (4.0 * self.fm_));
            self.phase_[0] += f;
            self.phase_[1] += f * 1.47;
            if reset_noise_amount > 0.1 {
                if self.phase_[0] >= 1.0 + reset_noise {
                    self.phase_[0] = 1.0 - self.phase_[0];
                }
                if self.phase_[1] >= 1.0 + reset_noise {
                    self.phase_[1] = 1.0 - self.phase_[1];
                }
            } else {
                if self.phase_[0] >= 1.0 {
                    self.phase_[0] -= 1.0;
                }
                if self.phase_[1] >= 1.0 {
                    self.phase_[1] -= 1.0;
                }
            }

            let mut drum = -0.1;
            drum += Self::distorted_sine(self.phase_[0]) * 0.60;
            drum += Self::distorted_sine(self.phase_[1]) * 0.25;
            drum *= self.drum_amplitude_ * drum_level;
            drum = self.drum_lp_.process::<FILTER_MODE_LOW_PASS>(drum);

            let noise = random.get_float();
            let mut snare = self.snare_lp_.process::<FILTER_MODE_LOW_PASS>(noise);
            snare = self.snare_hp_.process::<FILTER_MODE_HIGH_PASS>(snare);
            snare = (snare + 0.1) * (self.snare_amplitude_ + self.fm_) * snare_level;

            out[out_index] = snare + drum; // It's a snare, it's a drum, it's a snare drum.
            out_index += 1;
        }
    }
}
