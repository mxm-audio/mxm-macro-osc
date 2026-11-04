//! Ported from plaits/dsp/noise/particle.h: random impulse train processed by a resonant
//! filter.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! Each `render` draws one float from the voice's `Random` before its loop, one per sample, and
//! one more when the particle first fires in the block: the order is part of the output.
//! `GetFloat` can return exactly 1.0, which `u <= density` compares as the C++ does.

use crate::cpp::min;
use crate::stmlib::dsp::dsp::sqrt;
use crate::stmlib::dsp::filter::{FILTER_MODE_BAND_PASS, FREQUENCY_DIRTY, Svf};
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct Particle {
    pre_gain_: f32,
    filter_: Svf,
}

impl Particle {
    #[inline]
    pub fn init(&mut self) {
        self.pre_gain_ = 0.0;
        self.filter_.init();
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub fn render(
        &mut self,
        sync: bool,
        density: f32,
        gain: f32,
        frequency: f32,
        spread: f32,
        q: f32,
        out: &mut [f32],
        aux: &mut [f32],
        size: usize,
        random: &mut Random,
    ) {
        let mut u = random.get_float();
        if sync {
            u = density;
        }
        let mut can_radomize_frequency = true;
        for (out, aux) in out[..size].iter_mut().zip(&mut aux[..size]) {
            let mut s = 0.0f32;
            if u <= density {
                s = u * gain;
                if can_radomize_frequency {
                    let u = 2.0 * random.get_float() - 1.0;
                    let f = min(semitones_to_ratio(spread * u) * frequency, 0.25);
                    self.pre_gain_ = 0.5 / sqrt(q * f * sqrt(density));
                    self.filter_.set_f_q::<FREQUENCY_DIRTY>(f, q);
                    // Keep the cutoff constant for this whole block.
                    can_radomize_frequency = false;
                }
            }
            *aux += s;
            *out += self
                .filter_
                .process::<FILTER_MODE_BAND_PASS>(self.pre_gain_ * s);
            u = random.get_float();
        }
    }
}
