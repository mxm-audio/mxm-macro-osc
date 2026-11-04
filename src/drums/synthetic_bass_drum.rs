//! Ported from plaits/dsp/drums/synthetic_bass_drum.h: naive bass drum model (modulated
//! oscillator with FM + envelope). Inadvertently 909-ish.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `DistortedSine` and `TransistorVCA` read no member, so they are associated functions while
//! the interpolators hold `f0_` and `sustain_gain_`. `transient_env_` and `transient_env_lp_`
//! are not set by `Init`, as in the C++: they start at zero with the voice.

use crate::cpp::{max, min};
use crate::dsp::K_SAMPLE_RATE;
use crate::oscillator::sine_oscillator::sine;
use crate::stmlib::dsp::dsp::{cast_i32, make_integral_fractional, one_pole, slope};
use crate::stmlib::dsp::filter::{FILTER_MODE_LOW_PASS, FREQUENCY_FAST, Svf};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct SyntheticBassDrumClick {
    lp_: f32,
    hp_: f32,
    filter_: Svf,
}

impl SyntheticBassDrumClick {
    pub fn init(&mut self) {
        self.lp_ = 0.0;
        self.hp_ = 0.0;
        self.filter_.init();
        self.filter_
            .set_f_q::<FREQUENCY_FAST>(5000.0 / K_SAMPLE_RATE, 2.0);
    }

    #[inline]
    pub fn process(&mut self, in_: f32) -> f32 {
        slope(&mut self.lp_, in_, 0.5, 0.1);
        one_pole(&mut self.hp_, self.lp_, 0.04);
        self.filter_
            .process::<FILTER_MODE_LOW_PASS>(self.lp_ - self.hp_)
    }
}

#[derive(Default)]
pub struct SyntheticBassDrumAttackNoise {
    lp_: f32,
    hp_: f32,
}

impl SyntheticBassDrumAttackNoise {
    pub fn init(&mut self) {
        self.lp_ = 0.0;
        self.hp_ = 0.0;
    }

    #[inline]
    pub fn render(&mut self, random: &mut Random) -> f32 {
        let sample = random.get_float();
        one_pole(&mut self.lp_, sample, 0.05);
        one_pole(&mut self.hp_, self.lp_, 0.005);
        self.lp_ - self.hp_
    }
}

#[derive(Default)]
pub struct SyntheticBassDrum {
    f0_: f32,
    phase_: f32,
    phase_noise_: f32,

    fm_: f32,
    fm_lp_: f32,
    body_env_: f32,
    body_env_lp_: f32,
    transient_env_: f32,
    transient_env_lp_: f32,

    sustain_gain_: f32,

    tone_lp_: f32,

    click_: SyntheticBassDrumClick,
    noise_: SyntheticBassDrumAttackNoise,

    body_env_pulse_width_: i32,
    fm_pulse_width_: i32,
}

impl SyntheticBassDrum {
    pub fn init(&mut self) {
        self.phase_ = 0.0;
        self.phase_noise_ = 0.0;
        self.f0_ = 0.0;
        self.fm_ = 0.0;
        self.fm_lp_ = 0.0;
        self.body_env_lp_ = 0.0;
        self.body_env_ = 0.0;
        self.body_env_pulse_width_ = 0;
        self.fm_pulse_width_ = 0;
        self.tone_lp_ = 0.0;
        self.sustain_gain_ = 0.0;

        self.click_.init();
        self.noise_.init();
    }

    #[inline]
    fn distorted_sine(mut phase: f32, phase_noise: f32, dirtiness: f32) -> f32 {
        phase += phase_noise * dirtiness;
        let (_, phase_fractional) = make_integral_fractional(phase);
        phase = phase_fractional;
        let triangle = (if phase < 0.5 { phase } else { 1.0 - phase }) * 4.0 - 1.0;
        let sine_ = 2.0 * triangle / (1.0 + triangle.abs());
        // `phase` is above −0.5 (the phase is in [0, 1) and the noise within ±0.5), so the
        // wrapped read never starts before the table.
        let clean_sine = sine(phase + 0.75);
        sine_ + (1.0 - dirtiness) * (clean_sine - sine_)
    }

    #[inline]
    fn transistor_vca(mut s: f32, gain: f32) -> f32 {
        s = (s - 0.6) * gain;
        3.0 * s / (2.0 + s.abs()) + gain * 0.3
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        sustain: bool,
        trigger: bool,
        accent: f32,
        f0: f32,
        tone: f32,
        mut decay: f32,
        mut dirtiness: f32,
        fm_envelope_amount: f32,
        mut fm_envelope_decay: f32,
        out: &mut [f32],
        size: usize,
        random: &mut Random,
    ) {
        decay *= decay;
        fm_envelope_decay *= fm_envelope_decay;

        let mut f0_mod = ParameterInterpolator::new(&mut self.f0_, f0, size);

        dirtiness *= max(1.0 - 8.0 * f0, 0.0);

        let fm_decay = 1.0 - 1.0 / (0.008 * (1.0 + fm_envelope_decay * 4.0) * K_SAMPLE_RATE);

        let body_env_decay = 1.0 - 1.0 / (0.02 * K_SAMPLE_RATE) * semitones_to_ratio(-decay * 60.0);
        let transient_env_decay = 1.0 - 1.0 / (0.005 * K_SAMPLE_RATE);
        let tone_f = min(4.0 * f0 * semitones_to_ratio(tone * 108.0), 1.0);
        let transient_level = tone;

        if trigger {
            self.fm_ = 1.0;
            self.transient_env_ = 0.3 + 0.7 * accent;
            self.body_env_ = self.transient_env_;
            self.body_env_pulse_width_ = cast_i32(K_SAMPLE_RATE * 0.001);
            self.fm_pulse_width_ = cast_i32(K_SAMPLE_RATE * 0.0013);
        }

        let mut sustain_gain =
            ParameterInterpolator::new(&mut self.sustain_gain_, accent * decay, size);

        for sample in &mut out[..size] {
            one_pole(&mut self.phase_noise_, random.get_float() - 0.5, 0.002);

            let mut mix = 0.0;

            if sustain {
                self.phase_ += f0_mod.next();
                if self.phase_ >= 1.0 {
                    self.phase_ -= 1.0;
                }
                let body = Self::distorted_sine(self.phase_, self.phase_noise_, dirtiness);
                mix -= Self::transistor_vca(body, sustain_gain.next());
            } else {
                if self.fm_pulse_width_ != 0 {
                    self.fm_pulse_width_ -= 1;
                    self.phase_ = 0.25;
                } else {
                    self.fm_ *= fm_decay;
                    let fm = 1.0 + fm_envelope_amount * 3.5 * self.fm_lp_;
                    self.phase_ += min(f0_mod.next() * fm, 0.5);
                    if self.phase_ >= 1.0 {
                        self.phase_ -= 1.0;
                    }
                }

                if self.body_env_pulse_width_ != 0 {
                    self.body_env_pulse_width_ -= 1;
                } else {
                    self.body_env_ *= body_env_decay;
                    self.transient_env_ *= transient_env_decay;
                }

                let envelope_lp_f = 0.1;
                one_pole(&mut self.body_env_lp_, self.body_env_, envelope_lp_f);
                one_pole(
                    &mut self.transient_env_lp_,
                    self.transient_env_,
                    envelope_lp_f,
                );
                one_pole(&mut self.fm_lp_, self.fm_, envelope_lp_f);

                let body = Self::distorted_sine(self.phase_, self.phase_noise_, dirtiness);
                // The click draws nothing, so which operand of `+` the C++ evaluates first
                // does not change the draws.
                let transient = self.click_.process(if self.body_env_pulse_width_ != 0 {
                    0.0
                } else {
                    1.0
                }) + self.noise_.render(random);

                mix -= Self::transistor_vca(body, self.body_env_lp_);
                mix -= transient * self.transient_env_lp_ * transient_level;
            }

            one_pole(&mut self.tone_lp_, mix, tone_f);
            *sample = self.tone_lp_;
        }
    }
}
