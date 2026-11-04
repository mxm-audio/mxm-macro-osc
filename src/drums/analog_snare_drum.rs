//! Ported from plaits/dsp/drums/analog_snare_drum.h: 808 snare drum model, revisited.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).

use crate::cpp::min;
use crate::dsp::K_SAMPLE_RATE;
use crate::oscillator::sine_oscillator::SineOscillator;
use crate::stmlib::dsp::dsp::{cast_i32, one_pole, soft_clip};
use crate::stmlib::dsp::filter::{FILTER_MODE_BAND_PASS, FREQUENCY_FAST, Svf};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::random::Random;

pub const K_NUM_MODES: usize = 5;

const K_MODE_FREQUENCIES: [f32; K_NUM_MODES] = [1.00, 2.00, 3.18, 4.16, 5.62];

#[derive(Default)]
pub struct AnalogSnareDrum {
    pulse_remaining_samples_: i32,
    pulse_: f32,
    pulse_height_: f32,
    pulse_lp_: f32,
    noise_envelope_: f32,
    sustain_gain_: f32,

    resonator_: [Svf; K_NUM_MODES],
    noise_filter_: Svf,

    // Replace the resonators in "free running" (sustain) mode.
    oscillator_: [SineOscillator; K_NUM_MODES],
}

impl AnalogSnareDrum {
    pub fn init(&mut self) {
        self.pulse_remaining_samples_ = 0;
        self.pulse_ = 0.0;
        self.pulse_height_ = 0.0;
        self.pulse_lp_ = 0.0;
        self.noise_envelope_ = 0.0;
        self.sustain_gain_ = 0.0;

        for i in 0..K_NUM_MODES {
            self.resonator_[i].init();
            self.oscillator_[i].init();
        }
        self.noise_filter_.init();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        sustain: bool,
        trigger: bool,
        accent: f32,
        f0: f32,
        mut tone: f32,
        decay: f32,
        mut snappy: f32,
        out: &mut [f32],
        size: usize,
        random: &mut Random,
    ) {
        let decay_xt = decay * (1.0 + decay * (decay - 1.0));
        let k_trigger_pulse_duration = cast_i32(1.0e-3 * K_SAMPLE_RATE);
        let k_pulse_decay_time = 0.1e-3 * K_SAMPLE_RATE;
        let q = 2000.0 * semitones_to_ratio(decay_xt * 84.0);
        let noise_envelope_decay =
            1.0 - 0.0017 * semitones_to_ratio(-decay * (50.0 + snappy * 10.0));
        let exciter_leak = snappy * (2.0 - snappy) * 0.1;

        snappy = snappy * 1.1 - 0.05;
        constrain(&mut snappy, 0.0, 1.0);

        if trigger {
            self.pulse_remaining_samples_ = k_trigger_pulse_duration;
            self.pulse_height_ = 3.0 + 7.0 * accent;
            self.noise_envelope_ = 2.0;
        }

        let mut f = [0.0f32; K_NUM_MODES];
        let mut gain = [0.0f32; K_NUM_MODES];

        for i in 0..K_NUM_MODES {
            f[i] = min(f0 * K_MODE_FREQUENCIES[i], 0.499);
            self.resonator_[i]
                .set_f_q::<FREQUENCY_FAST>(f[i], 1.0 + f[i] * (if i == 0 { q } else { q * 0.25 }));
        }

        if tone < 0.666667 {
            // 808-style (2 modes)
            tone *= 1.5;
            gain[0] = 1.5 + (1.0 - tone) * (1.0 - tone) * 4.5;
            gain[1] = 2.0 * tone + 0.15;
            gain[2..K_NUM_MODES].fill(0.0);
        } else {
            // What the 808 could have been if there were extra modes!
            tone = (tone - 0.666667) * 3.0;
            gain[0] = 1.5 - tone * 0.5;
            gain[1] = 2.15 - tone * 0.7;
            for g in &mut gain[2..K_NUM_MODES] {
                *g = tone;
                tone *= tone;
            }
        }

        let mut f_noise = f0 * 16.0;
        constrain(&mut f_noise, 0.0, 0.499);
        self.noise_filter_
            .set_f_q::<FREQUENCY_FAST>(f_noise, 1.0 + f_noise * 1.5);

        let mut sustain_gain =
            ParameterInterpolator::new(&mut self.sustain_gain_, accent * decay, size);

        for sample in &mut out[..size] {
            // Q45 / Q46
            let pulse;
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

            let sustain_gain_value = sustain_gain.next();

            // R189 / C57 / R190 + C58 / C59 / R197 / R196 / IC14
            one_pole(&mut self.pulse_lp_, pulse, 0.75);

            let mut shell = 0.0;
            for i in 0..K_NUM_MODES {
                let excitation = if i == 0 {
                    (pulse - self.pulse_lp_) + 0.006 * pulse
                } else {
                    0.026 * pulse
                };
                shell += gain[i]
                    * (if sustain {
                        self.oscillator_[i].next(f[i]) * sustain_gain_value * 0.25
                    } else {
                        self.resonator_[i].process::<FILTER_MODE_BAND_PASS>(excitation)
                            + excitation * exciter_leak
                    });
            }
            shell = soft_clip(shell);

            // C56 / R194 / Q48 / C54 / R188 / D54
            let mut noise = 2.0 * random.get_float() - 1.0;
            if noise < 0.0 {
                noise = 0.0;
            }
            self.noise_envelope_ *= noise_envelope_decay;
            noise *= (if sustain {
                sustain_gain_value
            } else {
                self.noise_envelope_
            }) * snappy
                * 2.0;

            // C66 / R201 / C67 / R202 / R203 / Q49
            noise = self.noise_filter_.process::<FILTER_MODE_BAND_PASS>(noise);

            // IC13
            *sample = noise + shell * (1.0 - snappy);
        }
    }
}
