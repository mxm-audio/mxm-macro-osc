//! Ported from plaits/dsp/engine/hi_hat_engine.{h,cc}: 808-style HH with two noise sources -
//! one faithful to the original, the other more metallic.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `temp_buffer_` is an arena offset of `kMaxBlockSize * 2` floats; the two hats take its first
//! `size` floats as `temp_1` and the next `size` as `temp_2` (`temp_buffer_ + size`), so the
//! ring-modulated hat leaves in the arena what the C++'s does.

use super::engine::{
    Engine, EngineParameters, PostProcessingSettings, TRIGGER_RISING_EDGE, TRIGGER_UNPATCHED,
    note_to_frequency,
};
use crate::drums::hi_hat::{HiHat, LinearVCA, RingModNoise, SquareNoise, SwingVCA};
use crate::dsp::K_MAX_BLOCK_SIZE;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct HiHatEngine {
    pub post_processing_settings: PostProcessingSettings,

    hi_hat_1_: HiHat<SquareNoise, SwingVCA, true, false>,
    hi_hat_2_: HiHat<RingModNoise, LinearVCA, false, true>,

    /// Arena offset of `kMaxBlockSize * 2` floats.
    temp_buffer_: usize,
}

impl Engine for HiHatEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.hi_hat_1_.init();
        self.hi_hat_2_.init();
        self.temp_buffer_ = allocator
            .allocate::<f32>(K_MAX_BLOCK_SIZE * 2)
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
        let temp_buffer = arena.view_mut::<f32>(self.temp_buffer_, K_MAX_BLOCK_SIZE * 2);
        let (temp_1, temp_2) = temp_buffer.split_at_mut(size);

        let f0 = note_to_frequency(parameters.note);

        self.hi_hat_1_.render(
            (parameters.trigger & TRIGGER_UNPATCHED) != 0,
            (parameters.trigger & TRIGGER_RISING_EDGE) != 0,
            parameters.accent,
            f0,
            parameters.timbre,
            parameters.morph,
            parameters.harmonics,
            temp_1,
            temp_2,
            out,
            size,
            random,
        );

        self.hi_hat_2_.render(
            (parameters.trigger & TRIGGER_UNPATCHED) != 0,
            (parameters.trigger & TRIGGER_RISING_EDGE) != 0,
            parameters.accent,
            f0,
            parameters.timbre,
            parameters.morph,
            parameters.harmonics,
            temp_1,
            temp_2,
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
