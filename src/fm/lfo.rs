//! Ported from plaits/dsp/fm/lfo.h: DX7-compatible LFO.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `Step` and `Scrub` draw from stmlib's one static generator, which is the voice's `Random`
//! here, passed in. `Clone` and `Copy`, where the C++ disallows copies, so that the six-op engine
//! can hand one voice's LFO to `set_modulations` of the same voice.

use super::dx_units::{lfo_delay, lfo_frequency, pitch_mod_sensitivity};
use super::patch::ModulationParameters;
use crate::oscillator::sine_oscillator::sine;
use crate::stmlib::dsp::dsp::make_integral_fractional;
use crate::stmlib::utils::random::Random;

pub type Waveform = i32;
pub const WAVEFORM_TRIANGLE: Waveform = 0;
pub const WAVEFORM_RAMP_DOWN: Waveform = 1;
pub const WAVEFORM_RAMP_UP: Waveform = 2;
pub const WAVEFORM_SQUARE: Waveform = 3;
pub const WAVEFORM_SINE: Waveform = 4;
pub const WAVEFORM_S_AND_H: Waveform = 5;

#[derive(Clone, Copy, Default)]
pub struct Lfo {
    phase_: f32,
    frequency_: f32,
    delay_phase_: f32,
    delay_increment_: [f32; 2],
    value_: f32,

    random_value_: f32,
    one_hz_: f32,

    amp_mod_depth_: f32,
    pitch_mod_depth_: f32,

    waveform_: Waveform,
    reset_phase_: bool,

    phase_integral_: i32,
}

impl Lfo {
    #[inline]
    pub fn init(&mut self, sample_rate: f32) {
        self.phase_ = 0.0;
        self.frequency_ = 0.1;
        self.delay_phase_ = 0.0;
        self.delay_increment_ = [0.1, 0.1];
        self.value_ = 0.0;
        self.random_value_ = 0.0;

        self.one_hz_ = 1.0 / sample_rate;

        self.amp_mod_depth_ = 0.0;
        self.pitch_mod_depth_ = 0.0;

        self.waveform_ = WAVEFORM_TRIANGLE;
        self.reset_phase_ = false;

        self.phase_integral_ = 0;
    }

    #[inline]
    pub fn set(&mut self, modulations: &ModulationParameters) {
        self.frequency_ = lfo_frequency(i32::from(modulations.rate)) * self.one_hz_;

        lfo_delay(i32::from(modulations.delay), &mut self.delay_increment_);
        self.delay_increment_[0] *= self.one_hz_;
        self.delay_increment_[1] *= self.one_hz_;

        self.waveform_ = Waveform::from(modulations.waveform);
        self.reset_phase_ = modulations.reset_phase != 0;

        self.amp_mod_depth_ = modulations.amp_mod_depth as f32 * 0.01;

        self.pitch_mod_depth_ = modulations.pitch_mod_depth as f32
            * 0.01
            * pitch_mod_sensitivity(i32::from(modulations.pitch_mod_sensitivity));
    }

    #[inline]
    pub fn reset(&mut self) {
        if self.reset_phase_ {
            self.phase_ = 0.0;
        }
        self.delay_phase_ = 0.0;
    }

    #[inline]
    pub fn step(&mut self, scale: f32, random: &mut Random) {
        self.phase_ += scale * self.frequency_;
        if self.phase_ >= 1.0 {
            self.phase_ -= 1.0;
            self.random_value_ = random.get_float();
        }
        self.value_ = self.value();

        self.delay_phase_ +=
            scale * self.delay_increment_[if self.delay_phase_ < 0.5 { 0 } else { 1 }];
        if self.delay_phase_ >= 1.0 {
            self.delay_phase_ = 1.0;
        }
    }

    #[inline]
    pub fn scrub(&mut self, mut sample: f32, random: &mut Random) {
        let phase = sample * self.frequency_;
        let (phase_integral, phase_fractional) = make_integral_fractional(phase);
        self.phase_ = phase_fractional;
        if phase_integral != self.phase_integral_ {
            self.phase_integral_ = phase_integral;
            self.random_value_ = random.get_float();
        }
        self.value_ = self.value();

        self.delay_phase_ = sample * self.delay_increment_[0];
        if self.delay_phase_ > 0.5 {
            sample -= 0.5 / self.delay_increment_[0];
            self.delay_phase_ = 0.5 + sample * self.delay_increment_[1];
            if self.delay_phase_ >= 1.0 {
                self.delay_phase_ = 1.0;
            }
        }
    }

    #[inline]
    pub fn value(&self) -> f32 {
        match self.waveform_ {
            WAVEFORM_TRIANGLE => {
                2.0 * (if self.phase_ < 0.5 {
                    0.5 - self.phase_
                } else {
                    self.phase_ - 0.5
                })
            }

            WAVEFORM_RAMP_DOWN => 1.0 - self.phase_,

            WAVEFORM_RAMP_UP => self.phase_,

            WAVEFORM_SQUARE => {
                if self.phase_ < 0.5 {
                    0.0
                } else {
                    1.0
                }
            }

            WAVEFORM_SINE => 0.5 + 0.5 * sine(self.phase_ + 0.5),

            WAVEFORM_S_AND_H => self.random_value_,

            _ => 0.0,
        }
    }

    #[inline]
    pub fn delay_ramp(&self) -> f32 {
        if self.delay_phase_ < 0.5 {
            0.0
        } else {
            (self.delay_phase_ - 0.5) * 2.0
        }
    }

    #[inline]
    pub fn pitch_mod(&self) -> f32 {
        (self.value_ - 0.5) * self.delay_ramp() * self.pitch_mod_depth_
    }

    #[inline]
    pub fn amp_mod(&self) -> f32 {
        (1.0 - self.value_) * self.delay_ramp() * self.amp_mod_depth_
    }
}
