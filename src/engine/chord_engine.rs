//! Ported from plaits/dsp/engine/chord_engine.{h,cc}: chords: wavetable and divide-down
//! organ/string machine.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `ComputeRegistration` reads no member, so it is an associated function; `ComputeChordInversion`
//! is declared in the C++ class and never defined (the chord bank's is called).

use super::engine::{Engine, EngineParameters, PostProcessingSettings, note_to_frequency};
use crate::chords::chord_bank::{ChordBank, K_CHORD_NUM_VOICES};
use crate::cpp::{at, max};
use crate::oscillator::string_synth_oscillator::StringSynthOscillator;
use crate::oscillator::wavetable_oscillator::WavetableOscillator;
use crate::resources::WAV_INTEGRATED_WAVES;
use crate::stmlib::dsp::dsp::{make_integral_fractional, one_pole};
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

pub const K_CHORD_NUM_HARMONICS: usize = 3;

#[derive(Default)]
pub struct ChordEngine {
    pub post_processing_settings: PostProcessingSettings,

    divide_down_voice_: [StringSynthOscillator; K_CHORD_NUM_VOICES],
    wavetable_voice_: [WavetableOscillator<128, 15>; K_CHORD_NUM_VOICES],
    chords_: ChordBank,

    morph_lp_: f32,
    timbre_lp_: f32,
}

const FADE_POINT: [f32; K_CHORD_NUM_VOICES] = [0.55, 0.47, 0.49, 0.51, 0.53];

const K_REGISTRATION_TABLE_SIZE: i32 = 8;
const REGISTRATIONS: [[f32; K_CHORD_NUM_HARMONICS * 2]; K_REGISTRATION_TABLE_SIZE as usize] = [
    [0.0, 1.0, 0.0, 0.0, 0.0, 0.0],    // Square
    [1.0, 0.0, 0.0, 0.0, 0.0, 0.0],    // Saw
    [0.5, 0.0, 0.5, 0.0, 0.0, 0.0],    // Saw + saw
    [0.33, 0.0, 0.33, 0.0, 0.33, 0.0], // Full saw
    [0.33, 0.0, 0.0, 0.33, 0.0, 0.33], // Full saw + square hybrid
    [0.5, 0.0, 0.0, 0.0, 0.0, 0.5],    // Saw + high square harmo
    [0.0, 0.5, 0.0, 0.0, 0.0, 0.5],    // Square + high square harmo
    [0.0, 0.1, 0.1, 0.0, 0.2, 0.6],    // // Saw+square + high harmo
];

/// `WAVE(bank, row, column)`: `&wav_integrated_waves[(bank * 64 + row * 8 + column) * 132]`.
macro_rules! wave {
    ($bank:expr, $row:expr, $column:expr) => {
        WAV_INTEGRATED_WAVES
            .split_at(($bank * 64 + $row * 8 + $column) * 132)
            .1
    };
}

static WAVETABLE: [&[i16]; 15] = [
    wave!(2, 6, 1),
    wave!(2, 6, 6),
    wave!(2, 6, 4),
    wave!(0, 6, 0),
    wave!(0, 6, 1),
    wave!(0, 6, 2),
    wave!(0, 6, 7),
    wave!(2, 4, 7),
    wave!(2, 4, 6),
    wave!(2, 4, 5),
    wave!(2, 4, 4),
    wave!(2, 4, 3),
    wave!(2, 4, 2),
    wave!(2, 4, 1),
    wave!(2, 4, 0),
];

impl ChordEngine {
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

impl Engine for ChordEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        for i in 0..K_CHORD_NUM_VOICES {
            self.divide_down_voice_[i].init();
            self.wavetable_voice_[i].init();
        }
        self.chords_.init(allocator);

        self.morph_lp_ = 0.0;
        self.timbre_lp_ = 0.0;
    }

    fn reset(&mut self, arena: &mut Arena) {
        self.chords_.reset(arena);
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

        let mut harmonics = [0.0f32; K_CHORD_NUM_HARMONICS * 2 + 2];
        let mut note_amplitudes = [0.0f32; K_CHORD_NUM_VOICES];
        let registration = max(1.0 - self.morph_lp_ * 2.15, 0.0);

        Self::compute_registration(registration, &mut harmonics);
        harmonics[K_CHORD_NUM_HARMONICS * 2] = 0.0;

        let mut ratios = [0.0f32; K_CHORD_NUM_VOICES];
        let aux_note_mask = self.chords_.compute_chord_inversion(
            arena,
            self.timbre_lp_,
            &mut ratios,
            &mut note_amplitudes,
        );

        out[..size].fill(0.0);
        aux[..size].fill(0.0);

        let f0 = note_to_frequency(parameters.note) * 0.998;
        let waveform = max((self.morph_lp_ - 0.535) * 2.15, 0.0);

        for note in 0..K_CHORD_NUM_VOICES {
            let mut wavetable_amount = 50.0 * (self.morph_lp_ - FADE_POINT[note]);
            constrain(&mut wavetable_amount, 0.0, 1.0);

            let mut divide_down_amount = 1.0 - wavetable_amount;
            let destination: &mut [f32] = if (1 << note) & aux_note_mask != 0 {
                &mut *aux
            } else {
                &mut *out
            };

            let note_f0 = f0 * ratios[note];
            let mut divide_down_gain = 4.0 - note_f0 * 32.0;
            constrain(&mut divide_down_gain, 0.0, 1.0);
            divide_down_amount *= divide_down_gain;

            // `if (wavetable_amount)`: true unless zero, a NaN included.
            if wavetable_amount != 0.0 {
                self.wavetable_voice_[note].render(
                    note_f0 * 1.004,
                    note_amplitudes[note] * wavetable_amount,
                    waveform,
                    &WAVETABLE,
                    destination,
                    size,
                );
            }

            if divide_down_amount != 0.0 {
                self.divide_down_voice_[note].render(
                    note_f0,
                    &harmonics,
                    note_amplitudes[note] * divide_down_amount,
                    destination,
                    size,
                );
            }
        }

        for i in 0..size {
            out[i] += aux[i];
            aux[i] *= 3.0;
        }
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
