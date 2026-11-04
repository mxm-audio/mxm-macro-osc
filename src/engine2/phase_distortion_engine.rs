//! Ported from plaits/dsp/engine2/phase_distortion_engine.{h,cc}: phase distortion and phase
//! modulation with an asymmetric triangle as the modulator.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).

use crate::cpp::min;
use crate::dsp::K_MAX_BLOCK_SIZE;
use crate::engine::engine::{Engine, EngineParameters, PostProcessingSettings, note_to_frequency};
use crate::oscillator::sine_oscillator::sine;
use crate::oscillator::variable_shape_oscillator::VariableShapeOscillator;
use crate::resources::LUT_FM_FREQUENCY_QUANTIZER;
use crate::stmlib::dsp::dsp::interpolate;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct PhaseDistortionEngine {
    pub post_processing_settings: PostProcessingSettings,

    shaper_: VariableShapeOscillator,
    modulator_: VariableShapeOscillator,
    /// Arena offset of `kMaxBlockSize * 4` floats.
    temp_buffer_: usize,
}

impl Engine for PhaseDistortionEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.modulator_.init();
        self.shaper_.init();
        self.temp_buffer_ = allocator
            .allocate::<f32>(K_MAX_BLOCK_SIZE * 4)
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
        _random: &mut Random,
    ) {
        let temp_buffer = arena.view_mut::<f32>(self.temp_buffer_, K_MAX_BLOCK_SIZE * 4);

        let f0 = 0.5 * note_to_frequency(parameters.note);
        let modulator_f = min(
            0.25,
            f0 * semitones_to_ratio(interpolate(
                &LUT_FM_FREQUENCY_QUANTIZER,
                parameters.harmonics,
                128.0,
            )),
        );
        let pw = 0.5 + parameters.morph * 0.49;
        let amount = 8.0 * parameters.timbre * parameters.timbre * (1.0 - modulator_f * 3.8);

        // Upsample by 2x
        let (synced, free_running) = temp_buffer.split_at_mut(2 * size);
        self.shaper_
            .render_with::<true, true>(f0, modulator_f, pw, 0.0, amount, synced, 2 * size);
        self.modulator_.render_with::<false, true>(
            f0,
            modulator_f,
            pw,
            0.0,
            amount,
            free_running,
            2 * size,
        );

        for i in 0..size {
            // Naive 0.5x downsampling.
            out[i] = 0.5 * sine(synced[2 * i] + 0.25);
            out[i] += 0.5 * sine(synced[2 * i + 1] + 0.25);

            aux[i] = 0.5 * sine(free_running[2 * i] + 0.25);
            aux[i] += 0.5 * sine(free_running[2 * i + 1] + 0.25);
        }
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
