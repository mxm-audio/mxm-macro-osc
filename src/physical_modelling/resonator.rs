//! Ported from plaits/dsp/physical_modelling/resonator.{h,cc}: resonator, taken from Rings' code
//! but with fixed position.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `ResonatorSvf`'s template parameters are const generics: `process::<FILTER_MODE_BAND_PASS,
//! true>(…)`. Where the C++ passes one buffer as its input and output (the modal voice's
//! exciter), `process_in_place` does the same arithmetic on one slice: each sample is read
//! before it is written, as in the C++. The batch processor the C++ walks with a pointer is an
//! index into `mode_filters_`.

use crate::cpp::min;
use crate::resources::LUT_STIFFNESS;
use crate::stmlib::dsp::cosine_oscillator::{COSINE_OSCILLATOR_APPROXIMATE, CosineOscillator};
use crate::stmlib::dsp::dsp::interpolate;
use crate::stmlib::dsp::filter::{
    FILTER_MODE_BAND_PASS, FILTER_MODE_LOW_PASS, FREQUENCY_FAST, FilterMode, OnePole,
};
use crate::stmlib::dsp::units::semitones_to_ratio;

pub const K_MAX_NUM_MODES: i32 = 24;
pub const K_MODE_BATCH_SIZE: usize = 4;

/// We render 4 modes simultaneously since there are enough registers to hold
/// all state variables.
#[derive(Clone, Copy)]
pub struct ResonatorSvf<const BATCH_SIZE: usize> {
    state_1_: [f32; BATCH_SIZE],
    state_2_: [f32; BATCH_SIZE],
}

impl<const BATCH_SIZE: usize> Default for ResonatorSvf<BATCH_SIZE> {
    fn default() -> Self {
        Self {
            state_1_: [0.0; BATCH_SIZE],
            state_2_: [0.0; BATCH_SIZE],
        }
    }
}

/// The batch's coefficients and its local state, as the C++ computes them before its loop.
struct Coefficients<const BATCH_SIZE: usize> {
    g: [f32; BATCH_SIZE],
    r_plus_g: [f32; BATCH_SIZE],
    h: [f32; BATCH_SIZE],
    state_1: [f32; BATCH_SIZE],
    state_2: [f32; BATCH_SIZE],
    gains: [f32; BATCH_SIZE],
}

impl<const BATCH_SIZE: usize> ResonatorSvf<BATCH_SIZE> {
    pub fn init(&mut self) {
        for i in 0..BATCH_SIZE {
            self.state_2_[i] = 0.0;
            self.state_1_[i] = self.state_2_[i];
        }
    }

    #[inline]
    fn coefficients(&self, f: &[f32], q: &[f32], gain: &[f32]) -> Coefficients<BATCH_SIZE> {
        let mut c = Coefficients {
            g: [0.0; BATCH_SIZE],
            r_plus_g: [0.0; BATCH_SIZE],
            h: [0.0; BATCH_SIZE],
            state_1: [0.0; BATCH_SIZE],
            state_2: [0.0; BATCH_SIZE],
            gains: [0.0; BATCH_SIZE],
        };
        for i in 0..BATCH_SIZE {
            c.g[i] = OnePole::tan::<FREQUENCY_FAST>(f[i]);
            let r = 1.0 / q[i];
            c.h[i] = 1.0 / (1.0 + r * c.g[i] + c.g[i] * c.g[i]);
            c.r_plus_g[i] = r + c.g[i];
            c.state_1[i] = self.state_1_[i];
            c.state_2[i] = self.state_2_[i];
            c.gains[i] = gain[i];
        }
        c
    }

    /// One sample through the batch: the sum of the modes' outputs.
    #[inline]
    fn tick<const MODE: FilterMode>(c: &mut Coefficients<BATCH_SIZE>, s_in: f32) -> f32 {
        let mut s_out = 0.0f32;
        for i in 0..BATCH_SIZE {
            let hp = (s_in - c.r_plus_g[i] * c.state_1[i] - c.state_2[i]) * c.h[i];
            let bp = c.g[i] * hp + c.state_1[i];
            c.state_1[i] = c.g[i] * hp + bp;
            let lp = c.g[i] * bp + c.state_2[i];
            c.state_2[i] = c.g[i] * bp + lp;
            s_out += c.gains[i] * (if MODE == FILTER_MODE_LOW_PASS { lp } else { bp });
        }
        s_out
    }

    #[inline]
    fn store(&mut self, c: &Coefficients<BATCH_SIZE>) {
        for i in 0..BATCH_SIZE {
            self.state_1_[i] = c.state_1[i];
            self.state_2_[i] = c.state_2[i];
        }
    }

    /// `Process<mode, add>(f, q, gain, in, out, size)`.
    #[inline]
    pub fn process<const MODE: FilterMode, const ADD: bool>(
        &mut self,
        f: &[f32],
        q: &[f32],
        gain: &[f32],
        in_: &[f32],
        out: &mut [f32],
        size: usize,
    ) {
        let mut c = self.coefficients(f, q, gain);
        for (x, y) in in_[..size].iter().zip(&mut out[..size]) {
            let s_out = Self::tick::<MODE>(&mut c, *x);
            if ADD {
                *y += s_out;
            } else {
                *y = s_out;
            }
        }
        self.store(&c);
    }

    /// `Process<mode, add>(f, q, gain, buffer, buffer, size)`, the input and the output one
    /// buffer.
    #[inline]
    pub fn process_in_place<const MODE: FilterMode, const ADD: bool>(
        &mut self,
        f: &[f32],
        q: &[f32],
        gain: &[f32],
        in_out: &mut [f32],
        size: usize,
    ) {
        let mut c = self.coefficients(f, q, gain);
        for sample in &mut in_out[..size] {
            let s_out = Self::tick::<MODE>(&mut c, *sample);
            if ADD {
                *sample += s_out;
            } else {
                *sample = s_out;
            }
        }
        self.store(&c);
    }
}

#[derive(Default)]
pub struct Resonator {
    resolution_: i32,

    mode_amplitude_: [f32; K_MAX_NUM_MODES as usize],
    mode_filters_: [ResonatorSvf<K_MODE_BATCH_SIZE>; K_MAX_NUM_MODES as usize / K_MODE_BATCH_SIZE],
}

#[inline]
fn nth_harmonic_compensation(n: i32, mut stiffness: f32) -> f32 {
    let mut stretch_factor = 1.0f32;
    for _ in 0..n - 1 {
        stretch_factor += stiffness;
        if stiffness < 0.0 {
            stiffness *= 0.93;
        } else {
            stiffness *= 0.98;
        }
    }
    1.0 / stretch_factor
}

impl Resonator {
    pub fn init(&mut self, position: f32, resolution: i32) {
        self.resolution_ = min(resolution, K_MAX_NUM_MODES);

        let mut amplitudes = CosineOscillator::default();
        amplitudes.init::<COSINE_OSCILLATOR_APPROXIMATE>(position);

        // `resolution`, not `resolution_`, as the C++ writes it: the voice passes
        // `kMaxNumModes`, so the two are equal.
        for i in 0..resolution {
            self.mode_amplitude_[i as usize] = amplitudes.next() * 0.25;
        }

        for i in 0..K_MAX_NUM_MODES as usize / K_MODE_BATCH_SIZE {
            self.mode_filters_[i].init();
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        mut f0: f32,
        structure: f32,
        mut brightness: f32,
        damping: f32,
        in_: &[f32],
        out: &mut [f32],
        size: usize,
    ) {
        // `structure` reaches 1 only as a limit (the modal engine smooths it), and at 1 the
        // read past the 65-entry table is multiplied by a zero fraction.
        let mut stiffness = interpolate(&LUT_STIFFNESS, structure, 64.0);
        f0 *= nth_harmonic_compensation(3, stiffness);

        let mut harmonic = f0;
        let mut stretch_factor = 1.0f32;
        let q_sqrt = semitones_to_ratio(damping * 79.7);
        let mut q = 500.0 * q_sqrt * q_sqrt;
        brightness *= 1.0 - structure * 0.3;
        brightness *= 1.0 - damping * 0.3;
        let q_loss = brightness * (2.0 - brightness) * 0.85 + 0.15;

        let mut mode_q = [0.0f32; K_MODE_BATCH_SIZE];
        let mut mode_f = [0.0f32; K_MODE_BATCH_SIZE];
        let mut mode_a = [0.0f32; K_MODE_BATCH_SIZE];
        let mut batch_counter = 0;

        let mut batch_processor = 0;

        for i in 0..self.resolution_ as usize {
            let mut mode_frequency = harmonic * stretch_factor;
            if mode_frequency >= 0.499 {
                mode_frequency = 0.499;
            }
            let mode_attenuation = 1.0 - mode_frequency * 2.0;

            mode_f[batch_counter] = mode_frequency;
            mode_q[batch_counter] = 1.0 + mode_frequency * q;
            mode_a[batch_counter] = self.mode_amplitude_[i] * mode_attenuation;
            batch_counter += 1;

            if batch_counter == K_MODE_BATCH_SIZE {
                batch_counter = 0;
                self.mode_filters_[batch_processor].process::<FILTER_MODE_BAND_PASS, true>(
                    &mode_f, &mode_q, &mode_a, in_, out, size,
                );
                batch_processor += 1;
            }

            stretch_factor += stiffness;
            if stiffness < 0.0 {
                // Make sure that the partials do not fold back into negative frequencies.
                stiffness *= 0.93;
            } else {
                // This helps adding a few extra partials in the highest frequencies.
                stiffness *= 0.98;
            }
            harmonic += f0;
            q *= q_loss;
        }
    }
}
