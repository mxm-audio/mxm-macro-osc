//! Ported from plaits/dsp/physical_modelling/modal_voice.{h,cc}: simple modal synthesis voice
//! with a mallet exciter: click -> LPF -> resonator.
//!
//! The click is replaced by continuous white noise when the trigger input
//! of the module is not patched.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `render` is handed the voice's `Random`, which the dust draws from.

use super::resonator::{K_MAX_NUM_MODES, Resonator, ResonatorSvf};
use crate::cpp::min;
use crate::noise::dust::dust;
use crate::stmlib::dsp::filter::FILTER_MODE_LOW_PASS;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct ModalVoice {
    excitation_filter_: ResonatorSvf<1>,
    resonator_: Resonator,
}

impl ModalVoice {
    pub fn init(&mut self) {
        self.excitation_filter_.init();
        self.resonator_.init(0.015, K_MAX_NUM_MODES);
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
        random: &mut Random,
    ) {
        let density = brightness * brightness;

        brightness += 0.25 * accent * (1.0 - brightness);
        damping += 0.25 * accent * (1.0 - damping);

        let range = if sustain { 36.0f32 } else { 60.0 };
        let f = if sustain { 4.0 * f0 } else { 2.0 * f0 };
        let cutoff = min(
            f * semitones_to_ratio((brightness * (2.0 - brightness) - 0.5) * range),
            0.499,
        );
        let q = if sustain { 0.7f32 } else { 1.5 };

        // Synthesize excitation signal.
        if sustain {
            let dust_f = 0.00005 + 0.99995 * density * density;
            for sample in &mut temp[..size] {
                *sample = dust(dust_f, random) * (4.0 - dust_f * 3.0) * accent;
            }
        } else {
            temp[..size].fill(0.0);
            if trigger {
                let attenuation = 1.0 - damping * 0.5;
                let amplitude = (0.12 + 0.08 * accent) * attenuation;
                temp[0] = amplitude * semitones_to_ratio(cutoff * cutoff * 24.0) / cutoff;
            }
        }
        let one = 1.0f32;
        self.excitation_filter_
            .process_in_place::<FILTER_MODE_LOW_PASS, false>(&[cutoff], &[q], &[one], temp, size);
        for i in 0..size {
            aux[i] += temp[i];
        }

        self.resonator_
            .process(f0, structure, brightness, damping, temp, out, size);
    }
}
