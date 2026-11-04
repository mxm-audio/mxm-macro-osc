//! Ported from plaits/dsp/drums/hi_hat.h: 808 HH, with a few extra parameters to push things to
//! the CY territory...
//!
//! The template parameter MetallicNoiseSource allows another kind of "metallic
//! noise" to be used, for results which are more similar to KR-55 or FM hi-hats.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The C++ template's type parameters are traits here ([`MetallicNoiseSource`], [`Vca`], whose
//! `call` is the functor's `operator()`), its two `bool`s const generics. `RenderPair` reads no
//! member, so it is an associated function, handed its two oscillators.

use crate::dsp::K_SAMPLE_RATE;
use crate::oscillator::oscillator::{OSCILLATOR_SHAPE_SAW, OSCILLATOR_SHAPE_SQUARE, Oscillator};
use crate::stmlib::dsp::dsp::cast_u32;
use crate::stmlib::dsp::filter::{
    FILTER_MODE_BAND_PASS, FILTER_MODE_HIGH_PASS, FREQUENCY_ACCURATE, Svf,
};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::random::Random;

/// The `MetallicNoiseSource` a [`HiHat`] is instantiated with.
pub trait MetallicNoiseSource: Default {
    fn init(&mut self);

    fn render(
        &mut self,
        f0: f32,
        temp_1: &mut [f32],
        temp_2: &mut [f32],
        out: &mut [f32],
        size: usize,
    );
}

/// The `VCA` functor a [`HiHat`] is instantiated with.
pub trait Vca: Default {
    fn call(&self, s: f32, gain: f32) -> f32;
}

/// 808 style "metallic noise" with 6 square oscillators.
#[derive(Default)]
pub struct SquareNoise {
    phase_: [u32; 6],
}

impl MetallicNoiseSource for SquareNoise {
    fn init(&mut self) {
        self.phase_.fill(0);
    }

    fn render(
        &mut self,
        f0: f32,
        _temp_1: &mut [f32],
        _temp_2: &mut [f32],
        out: &mut [f32],
        size: usize,
    ) {
        let ratios: [f32; 6] = [
            // Nominal f0: 414 Hz
            1.0, 1.304, 1.466, 1.787, 1.932, 2.536,
        ];

        let mut increment = [0u32; 6];
        let mut phase = [0u32; 6];
        for i in 0..6 {
            let mut f = f0 * ratios[i];
            if f >= 0.499 {
                f = 0.499;
            }
            increment[i] = cast_u32(f * 4294967296.0);
            phase[i] = self.phase_[i];
        }

        for sample in &mut out[..size] {
            phase[0] = phase[0].wrapping_add(increment[0]);
            phase[1] = phase[1].wrapping_add(increment[1]);
            phase[2] = phase[2].wrapping_add(increment[2]);
            phase[3] = phase[3].wrapping_add(increment[3]);
            phase[4] = phase[4].wrapping_add(increment[4]);
            phase[5] = phase[5].wrapping_add(increment[5]);
            let mut noise: u32 = 0;
            noise += phase[0] >> 31;
            noise += phase[1] >> 31;
            noise += phase[2] >> 31;
            noise += phase[3] >> 31;
            noise += phase[4] >> 31;
            noise += phase[5] >> 31;
            *sample = 0.33 * noise as f32 - 1.0;
        }

        self.phase_ = phase;
    }
}

#[derive(Default)]
pub struct RingModNoise {
    oscillator_: [Oscillator; 6],
}

impl RingModNoise {
    fn render_pair(
        osc: &mut [Oscillator],
        f: &[f32; 2],
        temp_1: &mut [f32],
        temp_2: &mut [f32],
        out: &mut [f32],
        size: usize,
    ) {
        osc[0].render::<OSCILLATOR_SHAPE_SQUARE>(f[0], 0.5, temp_1, size);
        osc[1].render::<OSCILLATOR_SHAPE_SAW>(f[1], 0.5, temp_2, size);
        for i in 0..size {
            out[i] += temp_1[i] * temp_2[i];
        }
    }
}

impl MetallicNoiseSource for RingModNoise {
    fn init(&mut self) {
        for oscillator in &mut self.oscillator_ {
            oscillator.init();
        }
    }

    fn render(
        &mut self,
        f0: f32,
        temp_1: &mut [f32],
        temp_2: &mut [f32],
        out: &mut [f32],
        size: usize,
    ) {
        let ratio = f0 / (0.01 + f0);
        let f1a = 200.0 / K_SAMPLE_RATE * ratio;
        let f1b = 7530.0 / K_SAMPLE_RATE * ratio;
        let f2a = 510.0 / K_SAMPLE_RATE * ratio;
        let f2b = 8075.0 / K_SAMPLE_RATE * ratio;
        let f3a = 730.0 / K_SAMPLE_RATE * ratio;
        let f3b = 10500.0 / K_SAMPLE_RATE * ratio;
        let f: [[f32; 2]; 3] = [[f1a, f1b], [f2a, f2b], [f3a, f3b]];

        out[..size].fill(0.0);

        for (i, f_i) in f.iter().enumerate() {
            Self::render_pair(
                &mut self.oscillator_[2 * i..2 * i + 2],
                f_i,
                temp_1,
                temp_2,
                out,
                size,
            );
        }
    }
}

#[derive(Default)]
pub struct SwingVCA;

impl Vca for SwingVCA {
    #[inline]
    fn call(&self, mut s: f32, gain: f32) -> f32 {
        s *= if s > 0.0 { 4.0 } else { 0.1 };
        s = s / (1.0 + s.abs());
        (s + 0.1) * gain
    }
}

#[derive(Default)]
pub struct LinearVCA;

impl Vca for LinearVCA {
    #[inline]
    fn call(&self, s: f32, gain: f32) -> f32 {
        s * gain
    }
}

#[derive(Default)]
pub struct HiHat<
    MetallicNoise: MetallicNoiseSource,
    V: Vca,
    const RESONANCE: bool,
    const TWO_STAGE_ENVELOPE: bool,
> {
    envelope_: f32,
    noise_clock_: f32,
    noise_sample_: f32,
    sustain_gain_: f32,

    metallic_noise_: MetallicNoise,
    noise_coloration_svf_: Svf,
    hpf_: Svf,

    vca_: std::marker::PhantomData<V>,
}

impl<MetallicNoise, V, const RESONANCE: bool, const TWO_STAGE_ENVELOPE: bool>
    HiHat<MetallicNoise, V, RESONANCE, TWO_STAGE_ENVELOPE>
where
    MetallicNoise: MetallicNoiseSource,
    V: Vca,
{
    pub fn init(&mut self) {
        self.envelope_ = 0.0;
        self.noise_clock_ = 0.0;
        self.noise_sample_ = 0.0;
        self.sustain_gain_ = 0.0;

        self.metallic_noise_.init();
        self.noise_coloration_svf_.init();
        self.hpf_.init();
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
        mut noisiness: f32,
        temp_1: &mut [f32],
        temp_2: &mut [f32],
        out: &mut [f32],
        size: usize,
        random: &mut Random,
    ) {
        let envelope_decay = 1.0 - 0.003 * semitones_to_ratio(-decay * 84.0);
        let cut_decay = 1.0 - 0.0025 * semitones_to_ratio(-decay * 36.0);

        if trigger {
            self.envelope_ = (1.5 + 0.5 * (1.0 - decay)) * (0.3 + 0.7 * accent);
        }

        // Render the metallic noise.
        self.metallic_noise_
            .render(2.0 * f0, temp_1, temp_2, out, size);

        // Apply BPF on the metallic noise.
        let mut cutoff = 150.0 / K_SAMPLE_RATE * semitones_to_ratio(tone * 72.0);
        constrain(&mut cutoff, 0.0, 16000.0 / K_SAMPLE_RATE);
        self.noise_coloration_svf_
            .set_f_q::<FREQUENCY_ACCURATE>(cutoff, if RESONANCE { 3.0 + 3.0 * tone } else { 1.0 });
        self.noise_coloration_svf_
            .process_buffer_in_place::<FILTER_MODE_BAND_PASS>(out, size);

        // This is not at all part of the 808 circuit! But to add more variety, we
        // add a variable amount of clocked noise to the output of the 6 schmitt
        // trigger oscillators.
        noisiness *= noisiness;
        let mut noise_f = f0 * (16.0 + 16.0 * (1.0 - noisiness));
        constrain(&mut noise_f, 0.0, 0.5);

        for sample in &mut out[..size] {
            self.noise_clock_ += noise_f;
            if self.noise_clock_ >= 1.0 {
                self.noise_clock_ -= 1.0;
                self.noise_sample_ = random.get_float() - 0.5;
            }
            *sample += noisiness * (self.noise_sample_ - *sample);
        }

        // Apply VCA.
        let mut sustain_gain =
            ParameterInterpolator::new(&mut self.sustain_gain_, accent * decay, size);
        for sample in &mut out[..size] {
            let vca = V::default();
            self.envelope_ *= if self.envelope_ > 0.5 || !TWO_STAGE_ENVELOPE {
                envelope_decay
            } else {
                cut_decay
            };
            *sample = vca.call(
                *sample,
                if sustain {
                    sustain_gain.next()
                } else {
                    self.envelope_
                },
            );
        }

        self.hpf_.set_f_q::<FREQUENCY_ACCURATE>(cutoff, 0.5);
        self.hpf_
            .process_buffer_in_place::<FILTER_MODE_HIGH_PASS>(out, size);
    }
}
