//! Ported from plaits/dsp/oscillator/wavetable_oscillator.h: integrated wavetable synthesis.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! A wave is a slice of `wav_integrated_waves` starting where the C++ pointer points, and a
//! wavetable (`const int16_t* const*`) a slice of them. A table read whose index came from a
//! float goes through [`at`], so it never panics. The class template's `wavetable_size`,
//! `num_waves`, `approximate_scale` and `attenuate_high_frequencies` are const generics, the last
//! two defaulting to `true` as in the C++.

use crate::cpp::{at, min};
use crate::oscillator::oscillator::K_MAX_FREQUENCY;
use crate::stmlib::dsp::dsp::{make_integral_fractional, one_pole};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::stmlib::constrain;

#[derive(Default)]
pub struct Differentiator {
    lp_: f32,
    previous_: f32,
}

impl Differentiator {
    pub fn init(&mut self) {
        self.previous_ = 0.0;
        self.lp_ = 0.0;
    }

    #[inline]
    pub fn process(&mut self, coefficient: f32, s: f32) -> f32 {
        one_pole(&mut self.lp_, s - self.previous_, coefficient);
        self.previous_ = s;
        self.lp_
    }
}

#[inline]
pub fn interpolate_wave(table: &[i16], index_integral: i32, index_fractional: f32) -> f32 {
    let a = at(table, index_integral) as f32;
    let b = at(table, index_integral.wrapping_add(1)) as f32;
    let t = index_fractional;
    a + (b - a) * t
}

#[inline]
pub fn interpolate_wave_hermite(table: &[i16], index_integral: i32, index_fractional: f32) -> f32 {
    let xm1 = at(table, index_integral) as f32;
    let x0 = at(table, index_integral.wrapping_add(1)) as f32;
    let x1 = at(table, index_integral.wrapping_add(2)) as f32;
    let x2 = at(table, index_integral.wrapping_add(3)) as f32;
    let c = (x1 - xm1) * 0.5;
    let v = x0 - x1;
    let w = c + v;
    let a = w + v + (x2 - x0) * 0.5;
    let b_neg = w + a;
    let f = index_fractional;
    (((a * f) - b_neg) * f + c) * f + x0
}

#[derive(Default)]
pub struct WavetableOscillator<
    const WAVETABLE_SIZE: usize,
    const NUM_WAVES: usize,
    const APPROXIMATE_SCALE: bool = true,
    const ATTENUATE_HIGH_FREQUENCIES: bool = true,
> {
    // Oscillator state.
    phase_: f32,

    // For interpolation of parameters.
    frequency_: f32,
    amplitude_: f32,
    waveform_: f32,
    lp_: f32,

    differentiator_: Differentiator,
}

impl<
    const WAVETABLE_SIZE: usize,
    const NUM_WAVES: usize,
    const APPROXIMATE_SCALE: bool,
    const ATTENUATE_HIGH_FREQUENCIES: bool,
> WavetableOscillator<WAVETABLE_SIZE, NUM_WAVES, APPROXIMATE_SCALE, ATTENUATE_HIGH_FREQUENCIES>
{
    pub fn init(&mut self) {
        self.phase_ = 0.0;
        self.frequency_ = 0.0;
        self.amplitude_ = 0.0;
        self.waveform_ = 0.0;
        self.lp_ = 0.0;
        self.differentiator_.init();
    }

    /// `Render(frequency, amplitude, waveform, wavetable, out, size)`: adds to `out`.
    pub fn render(
        &mut self,
        mut frequency: f32,
        mut amplitude: f32,
        waveform: f32,
        wavetable: &[&[i16]],
        out: &mut [f32],
        size: usize,
    ) {
        constrain(&mut frequency, 0.0000001, K_MAX_FREQUENCY);

        if ATTENUATE_HIGH_FREQUENCIES {
            amplitude *= 1.0 - 2.0 * frequency;
        }
        if APPROXIMATE_SCALE {
            amplitude *= 1.0 / (frequency * 131072.0);
        }

        let mut frequency_modulation =
            ParameterInterpolator::new(&mut self.frequency_, frequency, size);
        let mut amplitude_modulation =
            ParameterInterpolator::new(&mut self.amplitude_, amplitude, size);
        // `float(num_waves - 1.0001f)`: the size_t promoted to float.
        let mut waveform_modulation = ParameterInterpolator::new(
            &mut self.waveform_,
            waveform * (NUM_WAVES as f32 - 1.0001),
            size,
        );

        let mut lp = self.lp_;
        let mut phase = self.phase_;
        for out in &mut out[..size] {
            let f0 = frequency_modulation.next();
            let cutoff = min(WAVETABLE_SIZE as f32 * f0, 1.0);
            let scale = if APPROXIMATE_SCALE {
                1.0
            } else {
                1.0 / (f0 * 131072.0)
            };

            phase += f0;
            if phase >= 1.0 {
                phase -= 1.0;
            }

            let waveform = waveform_modulation.next();
            let (waveform_integral, waveform_fractional) = make_integral_fractional(waveform);

            let p = phase * WAVETABLE_SIZE as f32;
            let (p_integral, p_fractional) = make_integral_fractional(p);

            let x0 = interpolate_wave(at(wavetable, waveform_integral), p_integral, p_fractional);
            let x1 = interpolate_wave(
                at(wavetable, waveform_integral.wrapping_add(1)),
                p_integral,
                p_fractional,
            );

            let s = self
                .differentiator_
                .process(cutoff, (x0 + (x1 - x0) * waveform_fractional) * scale);
            one_pole(&mut lp, s, cutoff);
            *out += amplitude_modulation.next() * lp;
        }
        self.lp_ = lp;
        self.phase_ = phase;
    }
}
