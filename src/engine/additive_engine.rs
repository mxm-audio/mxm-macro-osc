//! Ported from plaits/dsp/engine/additive_engine.{h,cc}: additive synthesis with 24+8 partials.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `amplitudes_` is an arena offset of `kNumHarmonics` floats. `UpdateAmplitudes` reads no
//! member, so it is an associated function, handed the region from `amplitudes` on.

use super::engine::{Engine, EngineParameters, PostProcessingSettings, note_to_frequency};
use crate::oscillator::harmonic_oscillator::HarmonicOscillator;
use crate::oscillator::sine_oscillator::sine;
use crate::stmlib::dsp::dsp::one_pole;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

pub const K_HARMONIC_BATCH_SIZE: usize = 12;
pub const K_NUM_HARMONICS: usize = 36;
pub const K_NUM_HARMONIC_OSCILLATORS: usize = K_NUM_HARMONICS / K_HARMONIC_BATCH_SIZE;

#[derive(Default)]
pub struct AdditiveEngine {
    pub post_processing_settings: PostProcessingSettings,

    harmonic_oscillator_: [HarmonicOscillator<K_HARMONIC_BATCH_SIZE>; K_NUM_HARMONIC_OSCILLATORS],

    /// Arena offset of `kNumHarmonics` floats.
    amplitudes_: usize,
}

impl AdditiveEngine {
    fn update_amplitudes(
        centroid: f32,
        slope: f32,
        bumps: f32,
        amplitudes: &mut [f32],
        harmonic_indices: &[i32],
        num_harmonics: usize,
    ) {
        let n = num_harmonics as f32 - 1.0;
        let margin = (1.0 / slope - 1.0) / (1.0 + bumps);
        let center = centroid * (n + margin) - 0.5 * margin;

        let mut sum = 0.001f32;

        for (i, &harmonic_index) in harmonic_indices[..num_harmonics].iter().enumerate() {
            let order = (i as f32 - center).abs() * slope;
            let mut gain = 1.0 - order;
            gain += gain.abs();
            gain *= gain;

            let b = 0.25 + order * bumps;
            let bump_factor = 1.0 + sine(b);

            gain *= bump_factor;
            gain *= gain;
            gain *= gain;

            let j = harmonic_index as usize;

            // Warning about the following line: this is not a proper LP filter because
            // of the normalization. But in spite of its strange working, this line
            // turns out to be absolutely essential.
            //
            // I have tried both normalizing the LP-ed spectrum, and LP-ing the
            // normalized spectrum, and both of them cause more annoyances than this
            // "incorrect" solution.

            one_pole(&mut amplitudes[j], gain, 0.001);
            sum += amplitudes[j];
        }

        sum = 1.0 / sum;

        for &j in &harmonic_indices[..num_harmonics] {
            amplitudes[j as usize] *= sum;
        }
    }
}

#[inline]
pub fn bump(x: f32, centroid: f32, slope: f32) -> f32 {
    let d = (x - centroid).abs();
    let bump = 1.0 - d * slope;
    bump + bump.abs()
}

const INTEGER_HARMONICS: [i32; 24] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
];

const ORGAN_HARMONICS: [i32; 8] = [0, 1, 2, 3, 5, 7, 9, 11];

impl Engine for AdditiveEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.amplitudes_ = allocator
            .allocate::<f32>(K_NUM_HARMONICS)
            .expect("the arena holds the engine's amplitudes");
        for harmonic_oscillator in self.harmonic_oscillator_.iter_mut() {
            harmonic_oscillator.init();
        }
    }

    fn reset(&mut self, arena: &mut Arena) {
        arena
            .view_mut::<f32>(self.amplitudes_, K_NUM_HARMONICS)
            .fill(0.0);
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
        _random: &mut Random,
    ) {
        let amplitudes_ = arena.view_mut::<f32>(self.amplitudes_, K_NUM_HARMONICS);

        let f0 = note_to_frequency(parameters.note);

        let centroid = parameters.timbre;
        let raw_bumps = parameters.harmonics;
        let raw_slope = (1.0 - 0.6 * raw_bumps) * parameters.morph;
        let slope = 0.01 + 1.99 * raw_slope * raw_slope * raw_slope;
        let bumps = 16.0 * raw_bumps * raw_bumps;
        Self::update_amplitudes(
            centroid,
            slope,
            bumps,
            &mut amplitudes_[0..],
            &INTEGER_HARMONICS,
            24,
        );
        self.harmonic_oscillator_[0].render::<1>(f0, &amplitudes_[0..], out, size);
        self.harmonic_oscillator_[1].render::<13>(f0, &amplitudes_[12..], out, size);

        Self::update_amplitudes(
            centroid,
            slope,
            bumps,
            &mut amplitudes_[24..],
            &ORGAN_HARMONICS,
            8,
        );

        self.harmonic_oscillator_[2].render::<1>(f0, &amplitudes_[24..], aux, size);
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
