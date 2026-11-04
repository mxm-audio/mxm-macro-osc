//! Ported from plaits/dsp/engine/waveshaping_engine.{h,cc}: slope -> waveshaper -> wavefolder.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `lut_fold + 1` and `lut_fold_2 + 1`, the tables from their second element, are the slices
//! `LUT_FOLD[1..]` and `LUT_FOLD_2[1..]`.

use super::engine::{Engine, EngineParameters, PostProcessingSettings, note_to_frequency};
use crate::cpp::at;
use crate::oscillator::oscillator::{OSCILLATOR_SHAPE_SLOPE, Oscillator};
use crate::oscillator::sine_oscillator::sine;
use crate::resources::{LOOKUP_TABLE_I16_TABLE, LUT_FOLD, LUT_FOLD_2};
use crate::stmlib::dsp::dsp::{interpolate_hermite, make_integral_fractional};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct WaveshapingEngine {
    pub post_processing_settings: PostProcessingSettings,

    slope_: Oscillator,
    triangle_: Oscillator,
    previous_shape_: f32,
    previous_wavefolder_gain_: f32,
    previous_overtone_gain_: f32,
}

pub fn tame(mut f0: f32, harmonics: f32, order: f32) -> f32 {
    f0 *= harmonics;
    let max_f = 0.5 / order;
    let mut max_amount = 1.0 - (f0 - max_f) / (0.5 - max_f);
    constrain(&mut max_amount, 0.0, 1.0);
    max_amount * max_amount * max_amount
}

impl Engine for WaveshapingEngine {
    fn init(&mut self, _allocator: &mut BufferAllocator<'_>) {
        self.slope_.init();
        self.triangle_.init();
        self.previous_shape_ = 0.0;
        self.previous_wavefolder_gain_ = 0.0;
        self.previous_overtone_gain_ = 0.0;
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
        let root = parameters.note;

        let f0 = note_to_frequency(root);
        let pw = parameters.morph * 0.45 + 0.5;

        // Start from bandlimited slope signal.
        self.slope_
            .render::<OSCILLATOR_SHAPE_SLOPE>(f0, pw, out, size);
        self.triangle_
            .render::<OSCILLATOR_SHAPE_SLOPE>(f0, 0.5, aux, size);

        // Try to estimate how rich the spectrum is, and reduce the range of the
        // waveshaping control accordingly.
        let slope = 3.0 + (parameters.morph - 0.5).abs() * 5.0;
        let shape_amount = (parameters.harmonics - 0.5).abs() * 2.0;
        let shape_amount_attenuation = tame(f0, slope, 16.0);
        let wavefolder_gain = parameters.timbre;
        let wavefolder_gain_attenuation = tame(
            f0,
            slope * (3.0 + shape_amount * shape_amount_attenuation * 5.0),
            12.0,
        );

        // Apply waveshaper / wavefolder.
        let mut shape_modulation = ParameterInterpolator::new(
            &mut self.previous_shape_,
            0.5 + (parameters.harmonics - 0.5) * shape_amount_attenuation,
            size,
        );
        let mut wf_gain_modulation = ParameterInterpolator::new(
            &mut self.previous_wavefolder_gain_,
            0.03 + 0.46 * wavefolder_gain * wavefolder_gain_attenuation,
            size,
        );
        let overtone_gain = parameters.timbre * (2.0 - parameters.timbre);
        let mut overtone_gain_modulation = ParameterInterpolator::new(
            &mut self.previous_overtone_gain_,
            overtone_gain * (2.0 - overtone_gain),
            size,
        );

        for i in 0..size {
            let shape = shape_modulation.next() * 3.9999;
            let (shape_integral, shape_fractional) = make_integral_fractional(shape);

            let shape_1 = at(&LOOKUP_TABLE_I16_TABLE, shape_integral);
            let shape_2 = at(&LOOKUP_TABLE_I16_TABLE, shape_integral.wrapping_add(1));

            let ws_index = 127.0 * out[i] + 128.0;
            let (mut ws_index_integral, ws_index_fractional) = make_integral_fractional(ws_index);
            ws_index_integral &= 255;

            let x0 = at(shape_1, ws_index_integral) as f32 / 32768.0;
            let x1 = at(shape_1, ws_index_integral + 1) as f32 / 32768.0;
            let x = x0 + (x1 - x0) * ws_index_fractional;

            let y0 = at(shape_2, ws_index_integral) as f32 / 32768.0;
            let y1 = at(shape_2, ws_index_integral + 1) as f32 / 32768.0;
            let y = y0 + (y1 - y0) * ws_index_fractional;

            let mix = x + (y - x) * shape_fractional;
            let index = mix * wf_gain_modulation.next() + 0.5;
            let fold = interpolate_hermite(&LUT_FOLD[1..], index, 512.0);
            let fold_2 = -interpolate_hermite(&LUT_FOLD_2[1..], index, 512.0);

            let sine = sine(aux[i] * 0.25 + 0.5);
            out[i] = fold;
            aux[i] = sine + (fold_2 - sine) * overtone_gain_modulation.next();
        }
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
