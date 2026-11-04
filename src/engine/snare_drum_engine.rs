//! Ported from plaits/dsp/engine/snare_drum_engine.{h,cc}: 808 and synthetic snare drum
//! generators.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).

use super::engine::{
    Engine, EngineParameters, PostProcessingSettings, TRIGGER_RISING_EDGE, TRIGGER_UNPATCHED,
    note_to_frequency,
};
use crate::drums::analog_snare_drum::AnalogSnareDrum;
use crate::drums::synthetic_snare_drum::SyntheticSnareDrum;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct SnareDrumEngine {
    pub post_processing_settings: PostProcessingSettings,

    analog_snare_drum_: AnalogSnareDrum,
    synthetic_snare_drum_: SyntheticSnareDrum,
}

impl Engine for SnareDrumEngine {
    fn init(&mut self, _allocator: &mut BufferAllocator<'_>) {
        self.analog_snare_drum_.init();
        self.synthetic_snare_drum_.init();
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
        _arena: &mut Arena,
        random: &mut Random,
    ) {
        let f0 = note_to_frequency(parameters.note);

        self.analog_snare_drum_.render(
            (parameters.trigger & TRIGGER_UNPATCHED) != 0,
            (parameters.trigger & TRIGGER_RISING_EDGE) != 0,
            parameters.accent,
            f0,
            parameters.timbre,
            parameters.morph,
            parameters.harmonics,
            out,
            size,
            random,
        );

        self.synthetic_snare_drum_.render(
            (parameters.trigger & TRIGGER_UNPATCHED) != 0,
            (parameters.trigger & TRIGGER_RISING_EDGE) != 0,
            parameters.accent,
            f0,
            parameters.timbre,
            parameters.morph,
            parameters.harmonics,
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
