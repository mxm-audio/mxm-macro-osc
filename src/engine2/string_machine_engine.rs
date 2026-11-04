//! Ported from plaits/dsp/engine2/string_machine_engine.{h,cc}: string machine emulation with
//! filter and chorus.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The chord tables and the ensemble's delay memory are in the arena, as in the C++.
//! `ComputeRegistration` reads no member, so it is an associated function.

use crate::chords::chord_bank::{ChordBank, K_CHORD_NUM_NOTES};
use crate::cpp::{at, max};
use crate::engine::chord_engine::K_CHORD_NUM_HARMONICS;
use crate::engine::engine::{Engine, EngineParameters, PostProcessingSettings, note_to_frequency};
use crate::fx::ensemble::Ensemble;
use crate::oscillator::string_synth_oscillator::StringSynthOscillator;
use crate::stmlib::dsp::dsp::{make_integral_fractional, one_pole};
use crate::stmlib::dsp::filter::{FILTER_MODE_LOW_PASS, FREQUENCY_DIRTY, NaiveSvf};
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct StringMachineEngine {
    pub post_processing_settings: PostProcessingSettings,

    chords_: ChordBank,

    ensemble_: Ensemble,
    divide_down_voice_: [StringSynthOscillator; K_CHORD_NUM_NOTES],
    svf_: [NaiveSvf; 2],

    morph_lp_: f32,
    timbre_lp_: f32,
}

const K_REGISTRATION_TABLE_SIZE: i32 = 11;
const REGISTRATIONS: [[f32; K_CHORD_NUM_HARMONICS * 2]; K_REGISTRATION_TABLE_SIZE as usize] = [
    [1.0, 0.0, 0.0, 0.0, 0.0, 0.0], // Saw
    [0.5, 0.0, 0.5, 0.0, 0.0, 0.0], // Saw + saw
    [0.4, 0.0, 0.2, 0.0, 0.4, 0.0], // Full saw
    [0.3, 0.0, 0.0, 0.3, 0.0, 0.4], // Full saw + square hybrid
    [0.3, 0.0, 0.0, 0.0, 0.0, 0.7], // Saw + high square harmo
    [0.2, 0.0, 0.0, 0.2, 0.0, 0.6], // Weird hybrid
    [0.0, 0.2, 0.1, 0.0, 0.2, 0.5], // Sawsquare high harmo
    [0.0, 0.3, 0.0, 0.3, 0.0, 0.4], // Square high armo
    [0.0, 0.4, 0.0, 0.3, 0.0, 0.3], // Full square
    [0.0, 0.5, 0.0, 0.5, 0.0, 0.0], // Square + Square
    [0.0, 1.0, 0.0, 0.0, 0.0, 0.0], // Square
];

impl StringMachineEngine {
    fn compute_registration(mut registration: f32, amplitudes: &mut [f32]) {
        // `kRegistrationTableSize - 1.001f`: the int promoted to float.
        registration *= K_REGISTRATION_TABLE_SIZE as f32 - 1.001;
        let (registration_integral, registration_fractional) =
            make_integral_fractional(registration);

        let row_a = at(&REGISTRATIONS, registration_integral);
        let row_b = at(&REGISTRATIONS, registration_integral.wrapping_add(1));
        for i in 0..K_CHORD_NUM_HARMONICS * 2 {
            let a = row_a[i];
            let b = row_b[i];
            amplitudes[i] = a + (b - a) * registration_fractional;
        }
    }
}

impl Engine for StringMachineEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        for i in 0..K_CHORD_NUM_NOTES {
            self.divide_down_voice_[i].init();
        }
        self.chords_.init(allocator);
        self.morph_lp_ = 0.0;
        self.timbre_lp_ = 0.0;
        self.svf_[0].init();
        self.svf_[1].init();
        self.ensemble_.init(
            allocator
                .allocate::<f32>(1024)
                .expect("the arena holds the ensemble's delay memory"),
        );
    }

    fn reset(&mut self, arena: &mut Arena) {
        self.chords_.reset(arena);
        self.ensemble_.reset(arena);
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
        one_pole(&mut self.morph_lp_, parameters.morph, 0.1);
        one_pole(&mut self.timbre_lp_, parameters.timbre, 0.1);

        self.chords_.set_chord(parameters.harmonics);

        // The last element is never written, nor read (the oscillators read seven).
        let mut harmonics = [0.0f32; K_CHORD_NUM_HARMONICS * 2 + 2];
        let registration = max(self.morph_lp_, 0.0);
        Self::compute_registration(registration, &mut harmonics);
        harmonics[K_CHORD_NUM_HARMONICS * 2] = 0.0;

        // Render string/organ sound.
        out[..size].fill(0.0);
        aux[..size].fill(0.0);
        let f0 = note_to_frequency(parameters.note) * 0.998;
        for note in 0..K_CHORD_NUM_NOTES {
            let note_f0 = f0 * self.chords_.ratio(arena, note);
            let mut divide_down_gain = 4.0 - note_f0 * 32.0;
            constrain(&mut divide_down_gain, 0.0, 1.0);
            self.divide_down_voice_[note].render(
                note_f0,
                &harmonics,
                0.25 * divide_down_gain,
                if note & 1 != 0 { &mut *aux } else { &mut *out },
                size,
            );
        }

        // Pass through VCF.
        let cutoff = 2.2 * f0 * semitones_to_ratio(120.0 * parameters.timbre);
        self.svf_[0].set_f_q::<FREQUENCY_DIRTY>(cutoff, 1.0);
        self.svf_[1].set_f_q::<FREQUENCY_DIRTY>(cutoff * 1.5, 1.0);

        // Mixdown.
        for i in 0..size {
            let l = self.svf_[0].process::<FILTER_MODE_LOW_PASS>(out[i]);
            let r = self.svf_[1].process::<FILTER_MODE_LOW_PASS>(aux[i]);
            out[i] = 0.66 * l + 0.33 * r;
            aux[i] = 0.66 * r + 0.33 * l;
        }

        // Ensemble FX.
        let amount = (parameters.timbre - 0.5).abs() * 2.0;
        let depth = 0.35 + 0.65 * parameters.timbre;
        self.ensemble_.set_amount(amount);
        self.ensemble_.set_depth(depth);
        self.ensemble_.process(out, aux, size, arena);
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
