//! Ported from plaits/dsp/oscillator/oscillator.h: single waveform oscillator. Can optionally
//! do audio-rate linear FM, with through-zero capabilities (negative frequencies).
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The shape is a const generic: `render::<OSCILLATOR_SHAPE_SAW>(f, pw, out, size)`. The C++
//! `Render(frequency, pw, fm, out, size)` overload is `render_fm`, with `None` for `NULL`, and
//! the three-parameter template is `render_with`.

use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::polyblep::{
    next_blep_sample, next_integrated_blep_sample, this_blep_sample, this_integrated_blep_sample,
};
use crate::stmlib::stmlib::constrain;

pub type OscillatorShape = u8;
pub const OSCILLATOR_SHAPE_IMPULSE_TRAIN: OscillatorShape = 0;
pub const OSCILLATOR_SHAPE_SAW: OscillatorShape = 1;
pub const OSCILLATOR_SHAPE_TRIANGLE: OscillatorShape = 2;
pub const OSCILLATOR_SHAPE_SLOPE: OscillatorShape = 3;
pub const OSCILLATOR_SHAPE_SQUARE: OscillatorShape = 4;
pub const OSCILLATOR_SHAPE_SQUARE_BRIGHT: OscillatorShape = 5;
pub const OSCILLATOR_SHAPE_SQUARE_DARK: OscillatorShape = 6;
pub const OSCILLATOR_SHAPE_SQUARE_TRIANGLE: OscillatorShape = 7;

pub const K_MAX_FREQUENCY: f32 = 0.25;
pub const K_MIN_FREQUENCY: f32 = 0.000001;

#[derive(Default)]
pub struct Oscillator {
    // Oscillator state.
    phase_: f32,
    next_sample_: f32,
    lp_state_: f32,
    hp_state_: f32,
    high_: bool,

    // For interpolation of parameters.
    frequency_: f32,
    pw_: f32,
}

impl Oscillator {
    pub fn init(&mut self) {
        self.phase_ = 0.5;
        self.next_sample_ = 0.0;
        self.lp_state_ = 1.0;
        self.hp_state_ = 0.0;
        self.high_ = true;

        self.frequency_ = 0.001;
        self.pw_ = 0.5;
    }

    pub fn render<const SHAPE: OscillatorShape>(
        &mut self,
        frequency: f32,
        pw: f32,
        out: &mut [f32],
        size: usize,
    ) {
        self.render_with::<SHAPE, false, false>(frequency, pw, &[], out, size);
    }

    pub fn render_fm<const SHAPE: OscillatorShape>(
        &mut self,
        frequency: f32,
        pw: f32,
        fm: Option<&[f32]>,
        out: &mut [f32],
        size: usize,
    ) {
        match fm {
            None => self.render_with::<SHAPE, false, false>(frequency, pw, &[], out, size),
            Some(fm) => self.render_with::<SHAPE, true, true>(frequency, pw, fm, out, size),
        }
    }

    pub fn render_with<
        const SHAPE: OscillatorShape,
        const HAS_EXTERNAL_FM: bool,
        const THROUGH_ZERO_FM: bool,
    >(
        &mut self,
        mut frequency: f32,
        mut pw: f32,
        external_fm: &[f32],
        out: &mut [f32],
        size: usize,
    ) {
        if !HAS_EXTERNAL_FM {
            if !THROUGH_ZERO_FM {
                constrain(&mut frequency, K_MIN_FREQUENCY, K_MAX_FREQUENCY);
            } else {
                constrain(&mut frequency, -K_MAX_FREQUENCY, K_MAX_FREQUENCY);
            }
            constrain(&mut pw, frequency.abs() * 2.0, 1.0 - 2.0 * frequency.abs());
        }

        let mut fm = ParameterInterpolator::new(&mut self.frequency_, frequency, size);
        let mut pwm = ParameterInterpolator::new(&mut self.pw_, pw, size);

        let mut next_sample = self.next_sample_;

        for i in 0..size {
            let mut this_sample = next_sample;
            next_sample = 0.0;

            let mut frequency = fm.next();
            if HAS_EXTERNAL_FM {
                frequency *= 1.0 + external_fm[i];
                if !THROUGH_ZERO_FM {
                    constrain(&mut frequency, K_MIN_FREQUENCY, K_MAX_FREQUENCY);
                } else {
                    constrain(&mut frequency, -K_MAX_FREQUENCY, K_MAX_FREQUENCY);
                }
            }
            let mut pw = if SHAPE == OSCILLATOR_SHAPE_SQUARE_TRIANGLE
                || SHAPE == OSCILLATOR_SHAPE_TRIANGLE
            {
                0.5
            } else {
                pwm.next()
            };
            if HAS_EXTERNAL_FM {
                constrain(&mut pw, frequency.abs() * 2.0, 1.0 - 2.0 * frequency.abs());
            }
            self.phase_ += frequency;

            if SHAPE <= OSCILLATOR_SHAPE_SAW {
                if self.phase_ >= 1.0 {
                    self.phase_ -= 1.0;
                    let t = self.phase_ / frequency;
                    this_sample -= this_blep_sample(t);
                    next_sample -= next_blep_sample(t);
                } else if THROUGH_ZERO_FM && self.phase_ < 0.0 {
                    let t = self.phase_ / frequency;
                    self.phase_ += 1.0;
                    this_sample += this_blep_sample(t);
                    next_sample += next_blep_sample(t);
                }
                next_sample += self.phase_;

                if SHAPE == OSCILLATOR_SHAPE_SAW {
                    out[i] = 2.0 * this_sample - 1.0;
                } else {
                    self.lp_state_ += 0.25 * ((self.hp_state_ - this_sample) - self.lp_state_);
                    out[i] = 4.0 * self.lp_state_;
                    self.hp_state_ = this_sample;
                }
            } else if SHAPE <= OSCILLATOR_SHAPE_SLOPE {
                let mut slope_up = 2.0f32;
                let mut slope_down = 2.0f32;
                if SHAPE == OSCILLATOR_SHAPE_SLOPE {
                    slope_up = 1.0 / pw;
                    slope_down = 1.0 / (1.0 - pw);
                }
                if self.high_ ^ (self.phase_ < pw) {
                    let t = (self.phase_ - pw) / frequency;
                    let mut discontinuity = (slope_up + slope_down) * frequency;
                    if THROUGH_ZERO_FM && frequency < 0.0 {
                        discontinuity = -discontinuity;
                    }
                    this_sample -= this_integrated_blep_sample(t) * discontinuity;
                    next_sample -= next_integrated_blep_sample(t) * discontinuity;
                    self.high_ = self.phase_ < pw;
                }
                if self.phase_ >= 1.0 {
                    self.phase_ -= 1.0;
                    let t = self.phase_ / frequency;
                    let discontinuity = (slope_up + slope_down) * frequency;
                    this_sample += this_integrated_blep_sample(t) * discontinuity;
                    next_sample += next_integrated_blep_sample(t) * discontinuity;
                    self.high_ = true;
                } else if THROUGH_ZERO_FM && self.phase_ < 0.0 {
                    let t = self.phase_ / frequency;
                    self.phase_ += 1.0;
                    let discontinuity = (slope_up + slope_down) * frequency;
                    this_sample -= this_integrated_blep_sample(t) * discontinuity;
                    next_sample -= next_integrated_blep_sample(t) * discontinuity;
                    self.high_ = false;
                }
                next_sample += if self.high_ {
                    self.phase_ * slope_up
                } else {
                    1.0 - (self.phase_ - pw) * slope_down
                };
                out[i] = 2.0 * this_sample - 1.0;
            } else {
                if self.high_ ^ (self.phase_ >= pw) {
                    let t = (self.phase_ - pw) / frequency;
                    let mut discontinuity = 1.0f32;
                    if THROUGH_ZERO_FM && frequency < 0.0 {
                        discontinuity = -discontinuity;
                    }
                    this_sample += this_blep_sample(t) * discontinuity;
                    next_sample += next_blep_sample(t) * discontinuity;
                    self.high_ = self.phase_ >= pw;
                }
                if self.phase_ >= 1.0 {
                    self.phase_ -= 1.0;
                    let t = self.phase_ / frequency;
                    this_sample -= this_blep_sample(t);
                    next_sample -= next_blep_sample(t);
                    self.high_ = false;
                } else if THROUGH_ZERO_FM && self.phase_ < 0.0 {
                    let t = self.phase_ / frequency;
                    self.phase_ += 1.0;
                    this_sample += this_blep_sample(t);
                    next_sample += next_blep_sample(t);
                    self.high_ = true;
                }
                next_sample += if self.phase_ < pw { 0.0 } else { 1.0 };

                if SHAPE == OSCILLATOR_SHAPE_SQUARE_TRIANGLE {
                    let integrator_coefficient = frequency * 0.0625;
                    this_sample = 128.0 * (this_sample - 0.5);
                    self.lp_state_ += integrator_coefficient * (this_sample - self.lp_state_);
                    out[i] = self.lp_state_;
                } else if SHAPE == OSCILLATOR_SHAPE_SQUARE_DARK {
                    let integrator_coefficient = frequency * 2.0;
                    this_sample = 4.0 * (this_sample - 0.5);
                    self.lp_state_ += integrator_coefficient * (this_sample - self.lp_state_);
                    out[i] = self.lp_state_;
                } else if SHAPE == OSCILLATOR_SHAPE_SQUARE_BRIGHT {
                    let integrator_coefficient = frequency * 2.0;
                    this_sample = 2.0 * this_sample - 1.0;
                    self.lp_state_ += integrator_coefficient * (this_sample - self.lp_state_);
                    out[i] = (this_sample - self.lp_state_) * 0.5;
                } else {
                    this_sample = 2.0 * this_sample - 1.0;
                    out[i] = this_sample;
                }
            }
        }
        self.next_sample_ = next_sample;
    }
}
