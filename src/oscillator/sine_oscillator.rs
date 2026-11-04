//! Ported from plaits/dsp/oscillator/sine_oscillator.h: simple sine oscillator (wavetable) +
//! fast sine oscillator (magic circle).
//!
//! The fast implementation might glitch a bit under heavy modulations of the
//! frequency.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The C++ overloads are named for what they do: `SineOscillator::Next(f)` is `next`, and
//! `Next(f, amplitude, &sin, &cos)` is `next_quadrature`; `Render(f, amplitude, out, size)`
//! (which adds to `out`) is `render_additive`, and `Render(f, out, size)` is `render`. The same
//! for `FastSineOscillator`, whose mode is a const generic.

use std::f64::consts::PI as M_PI;

use bytemuck::{Pod, Zeroable};

use crate::cpp::lut_sine_at;
use crate::resources::LUT_SINE;
use crate::stmlib::dsp::dsp::{cast_i32, cast_u32, interpolate, make_integral_fractional};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::rsqrt::fast_rsqrt_carmack;

pub const K_SINE_LUT_SIZE: f32 = 512.0;
pub const K_SINE_LUT_QUADRATURE: usize = 128;
pub const K_SINE_LUT_BITS: u32 = 9;

/// Safe for phase >= 0.0f, will wrap.
///
/// `InterpolateWrap(lut_sine, phase, kSineLUTSize)`, its two reads through [`lut_sine_at`]: a
/// negative phase reads before the table, as the wave terrain's AUX does.
#[inline]
pub fn sine(mut phase: f32) -> f32 {
    phase -= cast_i32(phase) as f32;
    phase *= K_SINE_LUT_SIZE;
    let (index_integral, index_fractional) = make_integral_fractional(phase);
    let a = lut_sine_at(index_integral);
    let b = lut_sine_at(index_integral.wrapping_add(1));
    a + (b - a) * index_fractional
}

/// Potentially unsafe, if phase >= 1.25.
#[inline]
pub fn sine_no_wrap(phase: f32) -> f32 {
    interpolate(&LUT_SINE, phase, K_SINE_LUT_SIZE)
}

/// With positive of negative phase modulation up to an index of 32.
#[inline]
pub fn sine_pm(mut phase: u32, pm: f32) -> f32 {
    let max_uint32 = 4294967296.0f32;
    let max_index: i32 = 32;
    let offset = max_index as f32;
    let scale = max_uint32 / (max_index * 2) as f32;

    // `uint32_t * int * int`: the ints convert to unsigned, modulo 2³².
    phase = phase.wrapping_add(
        cast_u32((pm + offset) * scale)
            .wrapping_mul(max_index as u32)
            .wrapping_mul(2),
    );

    let integral = phase >> (32 - K_SINE_LUT_BITS);
    let fractional = (phase << K_SINE_LUT_BITS) as f32 / max_uint32;
    let a = LUT_SINE[integral as usize];
    let b = LUT_SINE[integral as usize + 1];
    a + (b - a) * fractional
}

/// Direct lookup without interpolation.
#[inline]
pub fn sine_raw(phase: u32) -> f32 {
    LUT_SINE[(phase >> (32 - K_SINE_LUT_BITS)) as usize]
}

#[derive(Default)]
pub struct SineOscillator {
    // Oscillator state.
    phase_: f32,

    // For interpolation of parameters.
    frequency_: f32,
    amplitude_: f32,
}

impl SineOscillator {
    pub fn init(&mut self) {
        self.phase_ = 0.0;
        self.frequency_ = 0.0;
        self.amplitude_ = 0.0;
    }

    #[inline]
    pub fn next(&mut self, mut frequency: f32) -> f32 {
        if frequency >= 0.5 {
            frequency = 0.5;
        }

        self.phase_ += frequency;
        if self.phase_ >= 1.0 {
            self.phase_ -= 1.0;
        }

        sine_no_wrap(self.phase_)
    }

    #[inline]
    pub fn next_quadrature(
        &mut self,
        mut frequency: f32,
        amplitude: f32,
        sin: &mut f32,
        cos: &mut f32,
    ) {
        if frequency >= 0.5 {
            frequency = 0.5;
        }

        self.phase_ += frequency;
        if self.phase_ >= 1.0 {
            self.phase_ -= 1.0;
        }

        *sin = amplitude * sine_no_wrap(self.phase_);
        *cos = amplitude * sine_no_wrap(self.phase_ + 0.25);
    }

    pub fn render_additive(
        &mut self,
        frequency: f32,
        amplitude: f32,
        out: &mut [f32],
        size: usize,
    ) {
        self.render_internal::<true>(frequency, amplitude, out, size);
    }

    pub fn render(&mut self, frequency: f32, out: &mut [f32], size: usize) {
        self.render_internal::<false>(frequency, 1.0, out, size);
    }

    fn render_internal<const ADDITIVE: bool>(
        &mut self,
        mut frequency: f32,
        amplitude: f32,
        out: &mut [f32],
        size: usize,
    ) {
        if frequency >= 0.5 {
            frequency = 0.5;
        }
        let mut fm = ParameterInterpolator::new(&mut self.frequency_, frequency, size);
        let mut am = ParameterInterpolator::new(&mut self.amplitude_, amplitude, size);

        for sample in &mut out[..size] {
            self.phase_ += fm.next();
            if self.phase_ >= 1.0 {
                self.phase_ -= 1.0;
            }
            let s = sine_no_wrap(self.phase_);
            if ADDITIVE {
                *sample += am.next() * s;
            } else {
                *sample = s;
            }
        }
    }
}

pub type FastSineOscillatorMode = u8;
pub const NORMAL: FastSineOscillatorMode = 0;
pub const ADDITIVE: FastSineOscillatorMode = 1;
pub const QUADRATURE: FastSineOscillatorMode = 2;

/// `#[repr(C)]` and `Pod`: the swarm engine keeps it in the arena (the crate's `AGENTS.md`).
#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub struct FastSineOscillator {
    // Oscillator state.
    x_: f32,
    y_: f32,

    // For interpolation of parameters.
    epsilon_: f32,
    amplitude_: f32,
}

impl FastSineOscillator {
    pub fn init(&mut self) {
        self.x_ = 1.0;
        self.y_ = 0.0;
        self.epsilon_ = 0.0;
        self.amplitude_ = 0.0;
    }

    #[inline]
    pub fn fast_2_sin(f: f32) -> f32 {
        // In theory, epsilon = 2 sin(pi f)
        // Here, to avoid the call to sinf, we use a 3rd order polynomial
        // approximation, which looks like a Taylor expansion, but with a
        // correction term to give a good trade-off between average error
        // (1.13 cents) and maximum error (7.33 cents) when generating sinewaves
        // in the 16 Hz to 16kHz range (with sr = 48kHz).
        let f_pi = f * M_PI as f32;
        f_pi * (2.0 - (2.0 * 0.96 / 6.0) * f_pi * f_pi)
    }

    pub fn render(&mut self, frequency: f32, out: &mut [f32], size: usize) {
        self.render_internal::<NORMAL>(frequency, 1.0, out, &mut [], size);
    }

    pub fn render_additive(
        &mut self,
        frequency: f32,
        amplitude: f32,
        out: &mut [f32],
        size: usize,
    ) {
        self.render_internal::<ADDITIVE>(frequency, amplitude, out, &mut [], size);
    }

    pub fn render_quadrature(
        &mut self,
        frequency: f32,
        amplitude: f32,
        x: &mut [f32],
        y: &mut [f32],
        size: usize,
    ) {
        self.render_internal::<QUADRATURE>(frequency, amplitude, x, y, size);
    }

    fn render_internal<const MODE: FastSineOscillatorMode>(
        &mut self,
        mut frequency: f32,
        mut amplitude: f32,
        out: &mut [f32],
        out_2: &mut [f32],
        size: usize,
    ) {
        if frequency >= 0.25 {
            frequency = 0.25;
            amplitude = 0.0;
        } else {
            amplitude *= 1.0 - frequency * 4.0;
        }

        let mut epsilon =
            ParameterInterpolator::new(&mut self.epsilon_, Self::fast_2_sin(frequency), size);
        let mut am = ParameterInterpolator::new(&mut self.amplitude_, amplitude, size);
        let mut x = self.x_;
        let mut y = self.y_;

        let norm = x * x + y * y;
        if norm <= 0.5 || norm >= 2.0 {
            let scale = fast_rsqrt_carmack(norm);
            x *= scale;
            y *= scale;
        }

        for i in 0..size {
            let e = epsilon.next();
            x += e * y;
            y -= e * x;
            if MODE == ADDITIVE {
                out[i] += am.next() * x;
            } else if MODE == NORMAL {
                out[i] = x;
            } else if MODE == QUADRATURE {
                let amplitude = am.next();
                out[i] = x * amplitude;
                out_2[i] = y * amplitude;
            }
        }
        self.x_ = x;
        self.y_ = y;
    }
}
