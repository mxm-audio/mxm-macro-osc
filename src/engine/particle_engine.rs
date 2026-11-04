//! Ported from plaits/dsp/engine/particle_engine.{h,cc}: filtered random pulses.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The diffuser's delay memory is the whole arena, 8 192 `uint16_t` from byte 0; `reset` clears
//! it, as in the C++.

use super::engine::{
    Engine, EngineParameters, PostProcessingSettings, TRIGGER_RISING_EDGE, note_to_frequency,
};
use crate::cpp::min;
use crate::fx::diffuser::Diffuser;
use crate::noise::particle::Particle;
use crate::stmlib::dsp::filter::{FILTER_MODE_LOW_PASS, FREQUENCY_DIRTY, Svf};
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

pub const K_NUM_PARTICLES: usize = 6;

#[derive(Default)]
pub struct ParticleEngine {
    pub post_processing_settings: PostProcessingSettings,

    particle_: [Particle; K_NUM_PARTICLES],
    diffuser_: Diffuser,
    post_filter_: Svf,
}

impl Engine for ParticleEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        for particle in &mut self.particle_ {
            particle.init();
        }
        self.diffuser_.init(
            allocator
                .allocate::<u16>(8192)
                .expect("the arena holds the diffuser's memory"),
        );
        self.post_filter_.init();
    }

    fn reset(&mut self, arena: &mut Arena) {
        self.diffuser_.reset(arena);
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
        let f0 = note_to_frequency(parameters.note);
        let density_sqrt = note_to_frequency(60.0 + parameters.timbre * parameters.timbre * 72.0);
        let density = density_sqrt * density_sqrt * (1.0 / K_NUM_PARTICLES as f32);
        let gain = 1.0 / density;
        let q_sqrt = semitones_to_ratio(if parameters.morph >= 0.5 {
            (parameters.morph - 0.5) * 120.0
        } else {
            0.0
        });
        let q = 0.5 + q_sqrt * q_sqrt;
        let spread = 48.0 * parameters.harmonics * parameters.harmonics;
        let raw_diffusion_sqrt = 2.0 * (parameters.morph - 0.5).abs();
        let raw_diffusion = raw_diffusion_sqrt * raw_diffusion_sqrt;
        let diffusion = if parameters.morph < 0.5 {
            raw_diffusion
        } else {
            0.0
        };
        let sync = parameters.trigger & TRIGGER_RISING_EDGE != 0;

        out[..size].fill(0.0);
        aux[..size].fill(0.0);

        for particle in &mut self.particle_ {
            particle.render(sync, density, gain, f0, spread, q, out, aux, size, random);
        }

        self.post_filter_
            .set_f_q::<FREQUENCY_DIRTY>(min(f0, 0.49), 0.5);
        self.post_filter_
            .process_buffer_in_place::<FILTER_MODE_LOW_PASS>(out, size);

        self.diffuser_.process(
            0.8 * diffusion * diffusion,
            0.5 * diffusion + 0.25,
            out,
            size,
            arena,
        );
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
