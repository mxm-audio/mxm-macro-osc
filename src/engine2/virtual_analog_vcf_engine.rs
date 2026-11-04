//! Ported from plaits/dsp/engine2/virtual_analog_vcf_engine.{h,cc}: virtual analog oscillator
//! with VCF.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).

use crate::cpp::{max, min};
use crate::engine::engine::{Engine, EngineParameters, PostProcessingSettings, note_to_frequency};
use crate::oscillator::variable_shape_oscillator::VariableShapeOscillator;
use crate::stmlib::dsp::dsp::soft_clip;
use crate::stmlib::dsp::filter::{
    FILTER_MODE_HIGH_PASS, FILTER_MODE_LOW_PASS, FREQUENCY_FAST, Svf,
};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct VirtualAnalogVCFEngine {
    pub post_processing_settings: PostProcessingSettings,

    svf_: [Svf; 2],
    oscillator_: VariableShapeOscillator,
    sub_oscillator_: VariableShapeOscillator,

    previous_cutoff_: f32,
    previous_stage2_gain_: f32,
    previous_q_: f32,
    previous_gain_: f32,
    previous_sub_gain_: f32,
}

impl Engine for VirtualAnalogVCFEngine {
    fn init(&mut self, _allocator: &mut BufferAllocator<'_>) {
        self.oscillator_.init();
        self.sub_oscillator_.init();

        self.svf_[0].init();
        self.svf_[1].init();

        self.previous_sub_gain_ = 0.0;
        self.previous_cutoff_ = 0.0;
        self.previous_stage2_gain_ = 0.0;
        self.previous_q_ = 0.0;
        self.previous_gain_ = 0.0;
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
        // VA Oscillator (saw or PW square) + sub
        let f0 = note_to_frequency(parameters.note);

        let mut shape = (parameters.morph - 0.25) * 2.0 + 0.5;
        constrain(&mut shape, 0.5, 1.0);

        let mut pw = (parameters.morph - 0.5) * 2.0 + 0.5;
        if parameters.morph > 0.75 {
            pw = 2.5 - parameters.morph * 2.0;
        }
        constrain(&mut pw, 0.5, 0.98);

        let sub_gain = max((parameters.morph - 0.5).abs() - 0.3, 0.0) * 5.0;

        self.oscillator_.render(f0, pw, shape, out, size);
        self.sub_oscillator_.render(f0 * 0.501, 0.5, 1.0, aux, size);

        let cutoff = f0 * semitones_to_ratio((parameters.timbre - 0.2) * 120.0);

        let mut stage2_gain = 1.0 - (parameters.harmonics - 0.4) * 4.0;
        constrain(&mut stage2_gain, 0.0, 1.0);

        let resonance = 2.667 * max((parameters.harmonics - 0.5).abs() - 0.125, 0.0);
        let resonance_sqr = resonance * resonance;
        let q = resonance_sqr * resonance_sqr * 48.0;
        let mut gain = (parameters.harmonics - 0.7) + 0.85;
        constrain(&mut gain, 0.7 - resonance_sqr * 0.3, 1.0);

        let mut sub_gain_modulation =
            ParameterInterpolator::new(&mut self.previous_sub_gain_, sub_gain, size);
        let mut cutoff_modulation =
            ParameterInterpolator::new(&mut self.previous_cutoff_, cutoff, size);
        let mut stage2_gain_modulation =
            ParameterInterpolator::new(&mut self.previous_stage2_gain_, stage2_gain, size);
        let mut q_modulation = ParameterInterpolator::new(&mut self.previous_q_, q, size);
        let mut gain_modulation = ParameterInterpolator::new(&mut self.previous_gain_, gain, size);

        for i in 0..size {
            let cutoff = min(cutoff_modulation.next(), 0.25);
            let q = q_modulation.next();
            let stage2_gain = stage2_gain_modulation.next();

            self.svf_[0].set_f_q::<FREQUENCY_FAST>(cutoff, 0.5 + q);
            self.svf_[1].set_f_q::<FREQUENCY_FAST>(cutoff, 0.5 + 0.025 * q);

            let gain = gain_modulation.next();
            let input = soft_clip((out[i] + aux[i] * sub_gain_modulation.next()) * gain);

            let mut lp = 0.0f32;
            let mut hp = 0.0f32;
            self.svf_[0]
                .process2::<FILTER_MODE_LOW_PASS, FILTER_MODE_HIGH_PASS>(input, &mut lp, &mut hp);

            lp = soft_clip(lp * gain);
            lp += stage2_gain * (soft_clip(self.svf_[1].process::<FILTER_MODE_LOW_PASS>(lp)) - lp);

            out[i] = lp;
            aux[i] = soft_clip(hp * gain);
        }
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
