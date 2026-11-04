//! Ported from plaits/dsp/physical_modelling/string_voice.{h,cc}: extended Karplus-Strong, with
//! all the niceties from Rings.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The string's delay lines are in the arena, so `reset` and `render` take it; `render` is
//! handed the voice's `Random`, which the noise burst, the dust and the string's dispersion
//! draw from, in that order.

use super::string::String;
use crate::cpp::min;
use crate::noise::dust::dust;
use crate::stmlib::dsp::dsp::cast_i64;
use crate::stmlib::dsp::filter::{FILTER_MODE_LOW_PASS, FREQUENCY_DIRTY, Svf};
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct StringVoice {
    excitation_filter_: Svf,
    string_: String,
    remaining_noise_samples_: usize,
}

impl StringVoice {
    pub fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.excitation_filter_.init();
        self.string_.init(allocator);
        self.remaining_noise_samples_ = 0;
    }

    pub fn reset(&mut self, arena: &mut Arena) {
        self.string_.reset(arena);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        sustain: bool,
        trigger: bool,
        accent: f32,
        f0: f32,
        structure: f32,
        mut brightness: f32,
        mut damping: f32,
        temp: &mut [f32],
        out: &mut [f32],
        aux: &mut [f32],
        size: usize,
        arena: &mut Arena,
        random: &mut Random,
    ) {
        let density = brightness * brightness;

        brightness += 0.25 * accent * (1.0 - brightness);
        damping += 0.25 * accent * (1.0 - damping);

        // Synthesize excitation signal.
        if trigger || sustain {
            let range = 72.0f32;
            let f = 4.0 * f0;
            let cutoff = min(
                f * semitones_to_ratio((brightness * (2.0 - brightness) - 0.5) * range),
                0.499,
            );
            let q = if sustain { 1.0 } else { 0.5 };
            // `static_cast<size_t>(1.0f / f0)`: the active string's `f0` is the note's, positive
            // and at least `NoteToFrequency(-128)`, so the value is below 2⁶³, where MSVC's
            // conversion to `size_t` is the 64-bit `cvttss2si`.
            self.remaining_noise_samples_ = cast_i64(1.0 / f0) as usize;
            self.excitation_filter_
                .set_f_q::<FREQUENCY_DIRTY>(cutoff, q);
        }

        if sustain {
            let dust_f = 0.00005 + 0.99995 * density * density;
            for sample in &mut temp[..size] {
                *sample = dust(dust_f, random) * (8.0 - dust_f * 6.0) * accent;
            }
        } else if self.remaining_noise_samples_ != 0 {
            let noise_samples = min(self.remaining_noise_samples_, size);
            self.remaining_noise_samples_ -= noise_samples;
            let (noise, tail) = temp[..size].split_at_mut(noise_samples);
            for sample in noise {
                *sample = 2.0 * random.get_float() - 1.0;
            }
            tail.fill(0.0);
        } else {
            temp[..size].fill(0.0);
        }

        self.excitation_filter_
            .process_buffer_in_place::<FILTER_MODE_LOW_PASS>(temp, size);
        for i in 0..size {
            aux[i] += temp[i];
        }

        let non_linearity = if structure < 0.24 {
            (structure - 0.24) * 4.166
        } else if structure > 0.26 {
            (structure - 0.26) * 1.35135
        } else {
            0.0
        };
        self.string_.process(
            f0,
            non_linearity,
            brightness,
            damping,
            temp,
            out,
            size,
            arena,
            random,
        );
    }
}
