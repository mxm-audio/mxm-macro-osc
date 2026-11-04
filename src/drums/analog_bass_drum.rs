//! Ported from plaits/dsp/drums/analog_bass_drum.h: 808 bass drum model, revisited.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `Diode` reads no member, so it is an associated function while the interpolator holds
//! `sustain_gain_`.

use crate::cpp::min;
use crate::dsp::K_SAMPLE_RATE;
use crate::oscillator::sine_oscillator::SineOscillator;
use crate::stmlib::dsp::dsp::{cast_i32, one_pole};
use crate::stmlib::dsp::filter::{
    FILTER_MODE_BAND_PASS, FILTER_MODE_LOW_PASS, FREQUENCY_DIRTY, Svf,
};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::stmlib::constrain;

#[derive(Default)]
pub struct AnalogBassDrum {
    pulse_remaining_samples_: i32,
    fm_pulse_remaining_samples_: i32,
    pulse_: f32,
    pulse_height_: f32,
    pulse_lp_: f32,
    fm_pulse_lp_: f32,
    retrig_pulse_: f32,
    lp_out_: f32,
    tone_lp_: f32,
    sustain_gain_: f32,

    resonator_: Svf,

    // Replace the resonator in "free running" (sustain) mode.
    oscillator_: SineOscillator,
}

impl AnalogBassDrum {
    pub fn init(&mut self) {
        self.pulse_remaining_samples_ = 0;
        self.fm_pulse_remaining_samples_ = 0;
        self.pulse_ = 0.0;
        self.pulse_height_ = 0.0;
        self.pulse_lp_ = 0.0;
        self.fm_pulse_lp_ = 0.0;
        self.retrig_pulse_ = 0.0;
        self.lp_out_ = 0.0;
        self.tone_lp_ = 0.0;
        self.sustain_gain_ = 0.0;

        self.resonator_.init();
        self.oscillator_.init();
    }

    #[inline]
    fn diode(mut x: f32) -> f32 {
        if x >= 0.0 {
            x
        } else {
            x *= 2.0;
            0.7 * x / (1.0 + x.abs())
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        sustain: bool,
        trigger: bool,
        accent: f32,
        f0: f32,
        tone: f32,
        decay: f32,
        attack_fm_amount: f32,
        self_fm_amount: f32,
        out: &mut [f32],
        size: usize,
    ) {
        let k_trigger_pulse_duration = cast_i32(1.0e-3 * K_SAMPLE_RATE);
        let k_fm_pulse_duration = cast_i32(6.0e-3 * K_SAMPLE_RATE);
        let k_pulse_decay_time = 0.2e-3 * K_SAMPLE_RATE;
        let k_pulse_filter_time = 0.1e-3 * K_SAMPLE_RATE;
        let k_retrig_pulse_duration = 0.05 * K_SAMPLE_RATE;

        let scale = 0.001 / f0;
        let q = 1500.0 * semitones_to_ratio(decay * 80.0);
        let tone_f = min(4.0 * f0 * semitones_to_ratio(tone * 108.0), 1.0);
        let exciter_leak = 0.08 * (tone + 0.25);

        if trigger {
            self.pulse_remaining_samples_ = k_trigger_pulse_duration;
            self.fm_pulse_remaining_samples_ = k_fm_pulse_duration;
            self.pulse_height_ = 3.0 + 7.0 * accent;
            self.lp_out_ = 0.0;
        }

        let mut sustain_gain =
            ParameterInterpolator::new(&mut self.sustain_gain_, accent * decay, size);

        for sample in &mut out[..size] {
            // Q39 / Q40
            let mut pulse;
            if self.pulse_remaining_samples_ != 0 {
                self.pulse_remaining_samples_ -= 1;
                pulse = if self.pulse_remaining_samples_ != 0 {
                    self.pulse_height_
                } else {
                    self.pulse_height_ - 1.0
                };
                self.pulse_ = pulse;
            } else {
                self.pulse_ *= 1.0 - 1.0 / k_pulse_decay_time;
                pulse = self.pulse_;
            }
            if sustain {
                pulse = 0.0;
            }

            // C40 / R163 / R162 / D83
            one_pole(&mut self.pulse_lp_, pulse, 1.0 / k_pulse_filter_time);
            pulse = Self::diode((pulse - self.pulse_lp_) + pulse * 0.044);

            // Q41 / Q42
            let mut fm_pulse = 0.0;
            if self.fm_pulse_remaining_samples_ != 0 {
                self.fm_pulse_remaining_samples_ -= 1;
                fm_pulse = 1.0;
                // C39 / C52
                self.retrig_pulse_ = if self.fm_pulse_remaining_samples_ != 0 {
                    0.0
                } else {
                    -0.8
                };
            } else {
                // C39 / R161
                self.retrig_pulse_ *= 1.0 - 1.0 / k_retrig_pulse_duration;
            }
            if sustain {
                fm_pulse = 0.0;
            }
            one_pole(&mut self.fm_pulse_lp_, fm_pulse, 1.0 / k_pulse_filter_time);

            // Q43 and R170 leakage
            let punch = 0.7 + Self::diode(10.0 * self.lp_out_ - 1.0);

            // Q43 / R165
            let attack_fm = self.fm_pulse_lp_ * 1.7 * attack_fm_amount;
            let self_fm = punch * 0.08 * self_fm_amount;
            let mut f = f0 * (1.0 + attack_fm + self_fm);
            constrain(&mut f, 0.0, 0.4);

            let mut resonator_out = 0.0;
            if sustain {
                self.oscillator_.next_quadrature(
                    f,
                    sustain_gain.next(),
                    &mut resonator_out,
                    &mut self.lp_out_,
                );
            } else {
                self.resonator_.set_f_q::<FREQUENCY_DIRTY>(f, 1.0 + q * f);
                self.resonator_
                    .process2::<FILTER_MODE_BAND_PASS, FILTER_MODE_LOW_PASS>(
                        (pulse - self.retrig_pulse_ * 0.2) * scale,
                        &mut resonator_out,
                        &mut self.lp_out_,
                    );
            }

            one_pole(
                &mut self.tone_lp_,
                pulse * exciter_leak + resonator_out,
                tone_f,
            );

            *sample = self.tone_lp_;
        }
    }
}
