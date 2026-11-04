//! Ported from plaits/dsp/engine/virtual_analog_engine.{h,cc}: 2 variable shape oscillators
//! with sync, FM and crossfading.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `VA_VARIANT` is 2 in the C++, the variant built; the other two are left out.

use super::engine::{Engine, EngineParameters, PostProcessingSettings, note_to_frequency};
use crate::cpp::{at, max, min};
use crate::dsp::K_MAX_BLOCK_SIZE;
use crate::oscillator::variable_saw_oscillator::VariableSawOscillator;
use crate::oscillator::variable_shape_oscillator::VariableShapeOscillator;
use crate::stmlib::dsp::dsp::make_integral_fractional;
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct VirtualAnalogEngine {
    pub post_processing_settings: PostProcessingSettings,

    primary_: VariableShapeOscillator,
    auxiliary_: VariableShapeOscillator,

    sync_: VariableShapeOscillator,
    variable_saw_: VariableSawOscillator,

    auxiliary_amount_: f32,
    xmod_amount_: f32,
    /// Arena offset of `kMaxBlockSize` floats.
    temp_buffer_: usize,
}

const INTERVALS: [f32; 5] = [0.0, 7.01, 12.01, 19.01, 24.01];

#[inline]
fn squash(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

impl VirtualAnalogEngine {
    fn compute_detuning(&self, mut detune: f32) -> f32 {
        detune = 2.05 * detune - 1.025;
        constrain(&mut detune, -1.0, 1.0);

        let sign = if detune < 0.0 { -1.0 } else { 1.0 };
        detune = detune * sign * 3.9999;
        let (detune_integral, detune_fractional) = make_integral_fractional(detune);

        let a = at(&INTERVALS, detune_integral);
        let b = at(&INTERVALS, detune_integral.wrapping_add(1));
        (a + (b - a) * squash(squash(detune_fractional))) * sign
    }
}

impl Engine for VirtualAnalogEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.primary_.init();
        self.auxiliary_.init();
        self.auxiliary_.set_master_phase(0.25);
        self.sync_.init();
        self.variable_saw_.init();

        self.auxiliary_amount_ = 0.0;
        self.xmod_amount_ = 0.0;

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
        _random: &mut Random,
    ) {
        let temp_buffer = arena.view_mut::<f32>(self.temp_buffer_, K_MAX_BLOCK_SIZE);

        // 1 = variable square controlled by TIMBRE.
        // 2 = variable saw controlled by MORPH.
        // OUT = 1 + 2.
        // AUX = dual variable waveshape controlled by MORPH, self sync by TIMBRE.

        let sync_amount = parameters.timbre * parameters.timbre;
        let auxiliary_detune = self.compute_detuning(parameters.harmonics);
        let primary_f = note_to_frequency(parameters.note);
        let auxiliary_f = note_to_frequency(parameters.note + auxiliary_detune);
        let primary_sync_f = note_to_frequency(parameters.note + sync_amount * 48.0);
        let auxiliary_sync_f =
            note_to_frequency(parameters.note + auxiliary_detune + sync_amount * 48.0);

        let mut shape = parameters.morph * 1.5;
        constrain(&mut shape, 0.0, 1.0);

        let mut pw = 0.5 + (parameters.morph - 0.66) * 1.46;
        constrain(&mut pw, 0.5, 0.995);

        // Render monster sync to AUX.
        self.primary_
            .render_sync(primary_f, primary_sync_f, pw, shape, out, size);
        self.auxiliary_
            .render_sync(auxiliary_f, auxiliary_sync_f, pw, shape, aux, size);
        for i in 0..size {
            aux[i] = (aux[i] - out[i]) * 0.5;
        }

        // Render double varishape to OUT.
        let mut square_pw = 1.3 * parameters.timbre - 0.15;
        constrain(&mut square_pw, 0.005, 0.5);

        let square_sync_ratio = if parameters.timbre < 0.5 {
            0.0
        } else {
            (parameters.timbre - 0.5) * (parameters.timbre - 0.5) * 4.0 * 48.0
        };

        let square_gain = min(parameters.timbre * 8.0, 1.0);

        let mut saw_pw = if parameters.morph < 0.5 {
            parameters.morph + 0.5
        } else {
            1.0 - (parameters.morph - 0.5) * 2.0
        };
        saw_pw *= 1.1;
        constrain(&mut saw_pw, 0.005, 1.0);

        let mut saw_shape = 10.0 - 21.0 * parameters.morph;
        constrain(&mut saw_shape, 0.0, 1.0);

        let mut saw_gain = 8.0 * (1.0 - parameters.morph);
        constrain(&mut saw_gain, 0.02, 1.0);

        let square_sync_f = note_to_frequency(parameters.note + square_sync_ratio);

        self.sync_
            .render_sync(primary_f, square_sync_f, square_pw, 1.0, temp_buffer, size);
        self.variable_saw_
            .render(auxiliary_f, saw_pw, saw_shape, out, size);

        let norm = 1.0 / max(square_gain, saw_gain);

        let mut square_gain_modulation =
            ParameterInterpolator::new(&mut self.auxiliary_amount_, square_gain * 0.3 * norm, size);

        let mut saw_gain_modulation =
            ParameterInterpolator::new(&mut self.xmod_amount_, saw_gain * 0.5 * norm, size);

        for i in 0..size {
            out[i] = out[i] * saw_gain_modulation.next()
                + square_gain_modulation.next() * temp_buffer[i];
        }
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
