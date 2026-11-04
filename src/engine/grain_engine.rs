//! Ported from plaits/dsp/engine/grain_engine.{h,cc}: windowed sine segments.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The C++ keeps a `VOSIMOscillator` commented out, and so includes `vosim_oscillator.h`
//! without using it; it is not ported. `grain_balance_` is never read or written, as in the
//! C++.

use super::engine::{Engine, EngineParameters, PostProcessingSettings, note_to_frequency};
use crate::cpp::max;
use crate::oscillator::grainlet_oscillator::GrainletOscillator;
use crate::oscillator::z_oscillator::ZOscillator;
use crate::stmlib::dsp::filter::{FILTER_MODE_HIGH_PASS, FREQUENCY_DIRTY, OnePole};
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct GrainEngine {
    pub post_processing_settings: PostProcessingSettings,

    grainlet_: [GrainletOscillator; 2],
    // VOSIMOscillator vosim_oscillator_;
    z_oscillator_: ZOscillator,
    dc_blocker_: [OnePole; 2],

    #[allow(dead_code)]
    grain_balance_: f32,
}

impl Engine for GrainEngine {
    fn init(&mut self, _allocator: &mut BufferAllocator<'_>) {
        self.grainlet_[0].init();
        self.grainlet_[1].init();
        // vosim_oscillator_.Init();
        self.z_oscillator_.init();
        self.dc_blocker_[0].init();
        self.dc_blocker_[1].init();
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

        let f1 = note_to_frequency(24.0 + 84.0 * parameters.timbre);
        let ratio = semitones_to_ratio(-24.0 + 48.0 * parameters.harmonics);
        let carrier_bleed = if parameters.harmonics < 0.5 {
            1.0 - 2.0 * parameters.harmonics
        } else {
            0.0
        };
        let carrier_bleed_fixed = carrier_bleed * (2.0 - carrier_bleed);
        let carrier_shape = 0.33 + (parameters.morph - 0.33) * max(1.0 - f0 * 24.0, 0.0);

        self.grainlet_[0].render(f0, f1, carrier_shape, carrier_bleed_fixed, out, size);
        self.grainlet_[1].render(
            f0,
            f1 * ratio,
            carrier_shape,
            carrier_bleed_fixed,
            aux,
            size,
        );
        self.dc_blocker_[0].set_f::<FREQUENCY_DIRTY>(0.3 * f0);
        for i in 0..size {
            out[i] = self.dc_blocker_[0].process::<FILTER_MODE_HIGH_PASS>(out[i] + aux[i]);
        }

        let cutoff = note_to_frequency(root + 96.0 * parameters.timbre);
        self.z_oscillator_.render(
            f0,
            cutoff,
            parameters.morph,
            parameters.harmonics,
            aux,
            size,
        );

        self.dc_blocker_[1].set_f::<FREQUENCY_DIRTY>(0.3 * f0);
        self.dc_blocker_[1].process_buffer::<FILTER_MODE_HIGH_PASS>(aux, size);
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
