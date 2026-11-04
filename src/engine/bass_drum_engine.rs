//! Ported from plaits/dsp/engine/bass_drum_engine.{h,cc}: 808 and synthetic bass drum
//! generators.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).

use super::engine::{
    Engine, EngineParameters, PostProcessingSettings, TRIGGER_RISING_EDGE, TRIGGER_UNPATCHED,
    note_to_frequency,
};
use crate::cpp::{max, min};
use crate::drums::analog_bass_drum::AnalogBassDrum;
use crate::drums::synthetic_bass_drum::SyntheticBassDrum;
use crate::fx::overdrive::Overdrive;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct BassDrumEngine {
    pub post_processing_settings: PostProcessingSettings,

    analog_bass_drum_: AnalogBassDrum,
    synthetic_bass_drum_: SyntheticBassDrum,

    overdrive_: Overdrive,
}

impl Engine for BassDrumEngine {
    fn init(&mut self, _allocator: &mut BufferAllocator<'_>) {
        self.analog_bass_drum_.init();
        self.synthetic_bass_drum_.init();
        self.overdrive_.init();
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

        let attack_fm_amount = min(parameters.harmonics * 4.0, 1.0);
        let self_fm_amount = max(min(parameters.harmonics * 4.0 - 1.0, 1.0), 0.0);
        let drive = max(parameters.harmonics * 2.0 - 1.0, 0.0) * max(1.0 - 16.0 * f0, 0.0);

        let sustain = (parameters.trigger & TRIGGER_UNPATCHED) != 0;

        self.analog_bass_drum_.render(
            sustain,
            (parameters.trigger & TRIGGER_RISING_EDGE) != 0,
            parameters.accent,
            f0,
            parameters.timbre,
            parameters.morph,
            attack_fm_amount,
            self_fm_amount,
            out,
            size,
        );

        self.overdrive_.process(0.5 + 0.5 * drive, out, size);

        self.synthetic_bass_drum_.render(
            sustain,
            (parameters.trigger & TRIGGER_RISING_EDGE) != 0,
            parameters.accent,
            f0,
            parameters.timbre,
            parameters.morph,
            if sustain {
                parameters.harmonics
            } else {
                0.4 - 0.25 * parameters.morph * parameters.morph
            },
            min(parameters.harmonics * 2.0, 1.0),
            max(parameters.harmonics * 2.0 - 1.0, 0.0),
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
