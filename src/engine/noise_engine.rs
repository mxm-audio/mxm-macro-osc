//! Ported from plaits/dsp/engine/noise_engine.{h,cc}: clocked noise processed by a multimode
//! filter.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `temp_buffer_`, `kMaxBlockSize` floats, is in the arena at the offset `init` allocates; the
//! second clocked noise writes it before it is read, as in the C++.

use super::engine::{
    Engine, EngineParameters, PostProcessingSettings, TRIGGER_RISING_EDGE, TRIGGER_UNPATCHED,
    note_to_frequency,
};
use crate::dsp::K_MAX_BLOCK_SIZE;
use crate::noise::clocked_noise::ClockedNoise;
use crate::stmlib::dsp::dsp::sqrt;
use crate::stmlib::dsp::filter::{FILTER_MODE_BAND_PASS, FREQUENCY_ACCURATE, Svf};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct NoiseEngine {
    pub post_processing_settings: PostProcessingSettings,

    clocked_noise_: [ClockedNoise; 2],
    lp_hp_filter_: Svf,
    bp_filter_: [Svf; 2],

    previous_f0_: f32,
    previous_f1_: f32,
    previous_q_: f32,
    previous_mode_: f32,

    /// Arena offset of `kMaxBlockSize` floats.
    temp_buffer_: usize,
}

impl Engine for NoiseEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.clocked_noise_[0].init();
        self.clocked_noise_[1].init();
        self.lp_hp_filter_.init();
        self.bp_filter_[0].init();
        self.bp_filter_[1].init();

        self.previous_f0_ = 0.0;
        self.previous_f1_ = 0.0;
        self.previous_q_ = 0.0;
        self.previous_mode_ = 0.0;

        self.temp_buffer_ = allocator
            .allocate::<f32>(K_MAX_BLOCK_SIZE)
            .expect("the arena holds the engine's temp buffer");
    }

    fn reset(&mut self, _arena: &mut Arena) {}

    fn load_user_data(&mut self, _user_data: Option<&'static [u8]>, _arena: &mut Arena) {}

    fn render(
        &mut self,
        parameters: &EngineParameters,
        out: &mut [f32],
        aux: &mut [f32],
        size: usize,
        _already_enveloped: &mut bool,
        arena: &mut Arena,
        random: &mut Random,
    ) {
        let temp_buffer = arena.view_mut::<f32>(self.temp_buffer_, K_MAX_BLOCK_SIZE);

        let f0 = note_to_frequency(parameters.note);
        let f1 = note_to_frequency(parameters.note + parameters.harmonics * 48.0 - 24.0);
        let clock_lowest_note = if parameters.trigger & TRIGGER_UNPATCHED != 0 {
            0.0
        } else {
            -24.0
        };
        let clock_f =
            note_to_frequency(parameters.timbre * (128.0 - clock_lowest_note) + clock_lowest_note);
        let q = 0.5 * semitones_to_ratio(parameters.morph * 120.0);
        let sync = parameters.trigger & TRIGGER_RISING_EDGE != 0;
        self.clocked_noise_[0].render(sync, clock_f, aux, size, random);
        self.clocked_noise_[1].render(sync, clock_f * f1 / f0, temp_buffer, size, random);

        let mut f0_modulation = ParameterInterpolator::new(&mut self.previous_f0_, f0, size);
        let mut f1_modulation = ParameterInterpolator::new(&mut self.previous_f1_, f1, size);
        let mut q_modulation = ParameterInterpolator::new(&mut self.previous_q_, q, size);
        let mut mode_modulation =
            ParameterInterpolator::new(&mut self.previous_mode_, parameters.harmonics, size);

        // `in_1` is `aux`, read at each sample before that sample of `aux` is written.
        for i in 0..size {
            let f0 = f0_modulation.next();
            let f1 = f1_modulation.next();
            let q = q_modulation.next();
            let gain = 1.0 / sqrt((0.5 + q) * 40.0 * f0);
            self.lp_hp_filter_.set_f_q::<FREQUENCY_ACCURATE>(f0, q);
            self.bp_filter_[0].set_f_q::<FREQUENCY_ACCURATE>(f0, q);
            self.bp_filter_[1].set_f_q::<FREQUENCY_ACCURATE>(f1, q);

            let input_1 = aux[i] * gain;
            let input_2 = temp_buffer[i] * gain;
            self.lp_hp_filter_.process_multimode_lp_to_hp(
                &[input_1],
                &mut out[i..],
                1,
                mode_modulation.next(),
            );
            aux[i] = self.bp_filter_[0].process::<FILTER_MODE_BAND_PASS>(input_1)
                + self.bp_filter_[1].process::<FILTER_MODE_BAND_PASS>(input_2);
        }
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
