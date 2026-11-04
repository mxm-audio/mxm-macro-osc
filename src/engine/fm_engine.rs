//! Ported from plaits/dsp/engine/fm_engine.{h,cc}: classic 2-op FM, as found in Braids, Rings
//! and Elements.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).

use super::engine::{Engine, EngineParameters, PostProcessingSettings, note_to_frequency};
use crate::downsampler::four_x_downsampler::{Downsampler, K_OVERSAMPLING};
use crate::dsp::A0;
use crate::oscillator::sine_oscillator::sine_pm;
use crate::resources::LUT_FM_FREQUENCY_QUANTIZER;
use crate::stmlib::dsp::dsp::{cast_u32, interpolate, one_pole};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct FMEngine {
    pub post_processing_settings: PostProcessingSettings,

    carrier_phase_: u32,
    modulator_phase_: u32,
    sub_phase_: u32,

    previous_carrier_frequency_: f32,
    previous_modulator_frequency_: f32,
    previous_amount_: f32,
    previous_feedback_: f32,
    previous_sample_: f32,

    sub_fir_: f32,
    carrier_fir_: f32,
}

impl Engine for FMEngine {
    fn init(&mut self, _allocator: &mut BufferAllocator<'_>) {
        self.carrier_phase_ = 0;
        self.modulator_phase_ = 0;
        self.sub_phase_ = 0;

        self.previous_carrier_frequency_ = A0;
        self.previous_modulator_frequency_ = A0;
        self.previous_amount_ = 0.0;
        self.previous_feedback_ = 0.0;
        self.previous_sample_ = 0.0;
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
        _random: &mut Random,
    ) {
        // 4x oversampling
        let note = parameters.note - 24.0;

        let ratio = interpolate(&LUT_FM_FREQUENCY_QUANTIZER, parameters.harmonics, 128.0);

        let modulator_note = note + ratio;
        let mut target_modulator_frequency = note_to_frequency(modulator_note);
        constrain(&mut target_modulator_frequency, 0.0, 0.5);

        // Reduce the maximum FM index for high pitched notes, to prevent aliasing.
        let mut hf_taming = 1.0 - (modulator_note - 72.0) * 0.025;
        constrain(&mut hf_taming, 0.0, 1.0);
        hf_taming *= hf_taming;

        let mut carrier_frequency = ParameterInterpolator::new(
            &mut self.previous_carrier_frequency_,
            note_to_frequency(note),
            size,
        );
        let mut modulator_frequency = ParameterInterpolator::new(
            &mut self.previous_modulator_frequency_,
            target_modulator_frequency,
            size,
        );
        let mut amount_modulation = ParameterInterpolator::new(
            &mut self.previous_amount_,
            2.0 * parameters.timbre * parameters.timbre * hf_taming,
            size,
        );
        let mut feedback_modulation = ParameterInterpolator::new(
            &mut self.previous_feedback_,
            2.0 * parameters.morph - 1.0,
            size,
        );

        let mut carrier_downsampler = Downsampler::new(&mut self.carrier_fir_);
        let mut sub_downsampler = Downsampler::new(&mut self.sub_fir_);

        for i in 0..size {
            let max_uint32 = 4294967296.0f32;
            let amount = amount_modulation.next();
            let feedback = feedback_modulation.next();
            let phase_feedback = if feedback < 0.0 {
                0.5 * feedback * feedback
            } else {
                0.0
            };
            let carrier_increment = cast_u32(max_uint32 * carrier_frequency.next());
            let _modulator_frequency = modulator_frequency.next();

            for j in 0..K_OVERSAMPLING {
                self.modulator_phase_ = self.modulator_phase_.wrapping_add(cast_u32(
                    max_uint32
                        * _modulator_frequency
                        * (1.0 + self.previous_sample_ * phase_feedback),
                ));
                self.carrier_phase_ = self.carrier_phase_.wrapping_add(carrier_increment);
                self.sub_phase_ = self.sub_phase_.wrapping_add(carrier_increment >> 1);
                let modulator_fb = if feedback > 0.0 {
                    0.25 * feedback * feedback
                } else {
                    0.0
                };
                let modulator =
                    sine_pm(self.modulator_phase_, modulator_fb * self.previous_sample_);
                let carrier = sine_pm(self.carrier_phase_, amount * modulator);
                let sub = sine_pm(self.sub_phase_, amount * carrier * 0.25);
                one_pole(&mut self.previous_sample_, carrier, 0.05);
                carrier_downsampler.accumulate(j as i32, carrier);
                sub_downsampler.accumulate(j as i32, sub);
            }

            out[i] = carrier_downsampler.read();
            aux[i] = sub_downsampler.read();
        }
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
