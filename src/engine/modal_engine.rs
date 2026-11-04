//! Ported from plaits/dsp/engine/modal_engine.{h,cc}: one voice of modal synthesis.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `temp_buffer_` is in the arena, as in the C++; nothing else of the engine is.

use super::engine::{
    Engine, EngineParameters, PostProcessingSettings, TRIGGER_RISING_EDGE, TRIGGER_UNPATCHED,
    note_to_frequency,
};
use crate::dsp::K_MAX_BLOCK_SIZE;
use crate::physical_modelling::modal_voice::ModalVoice;
use crate::stmlib::dsp::dsp::one_pole;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct ModalEngine {
    pub post_processing_settings: PostProcessingSettings,

    voice_: ModalVoice,
    /// Arena offset of `kMaxBlockSize` floats.
    temp_buffer_: usize,
    harmonics_lp_: f32,
}

impl Engine for ModalEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.temp_buffer_ = allocator
            .allocate::<f32>(K_MAX_BLOCK_SIZE)
            .expect("the arena holds the modal engine's buffer");
        self.harmonics_lp_ = 0.0;
        self.reset(allocator.arena_mut());
    }

    fn reset(&mut self, _arena: &mut Arena) {
        self.voice_.init();
    }

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
        out[..size].fill(0.0);
        aux[..size].fill(0.0);

        one_pole(&mut self.harmonics_lp_, parameters.harmonics, 0.01);

        self.voice_.render(
            parameters.trigger & TRIGGER_UNPATCHED != 0,
            parameters.trigger & TRIGGER_RISING_EDGE != 0,
            parameters.accent,
            note_to_frequency(parameters.note),
            self.harmonics_lp_,
            parameters.timbre,
            parameters.morph,
            arena.view_mut::<f32>(self.temp_buffer_, K_MAX_BLOCK_SIZE),
            out,
            aux,
            size,
            random,
        );
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
