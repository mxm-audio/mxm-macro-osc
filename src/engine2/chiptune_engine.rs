//! Ported from plaits/dsp/engine2/chiptune_engine.{h,cc}: chiptune waveforms with
//! arpeggiator.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `Init` initialises four of the five square voices (`kChordNumNotes` of `kChordNumVoices`),
//! as in the C++: the fifth keeps the zeroed state the static voice starts with, so its slave
//! frequency starts at 0, not 0.01.

use crate::chords::chord_bank::{ChordBank, K_CHORD_NUM_NOTES, K_CHORD_NUM_VOICES};
use crate::dsp::K_SAMPLE_RATE;
use crate::engine::engine::{
    Engine, EngineParameters, PostProcessingSettings, TRIGGER_RISING_EDGE, TRIGGER_UNPATCHED,
    note_to_frequency,
};
use crate::engine2::arpeggiator::Arpeggiator;
use crate::oscillator::nes_triangle_oscillator::NESTriangleOscillator;
use crate::oscillator::super_square_oscillator::SuperSquareOscillator;
use crate::stmlib::dsp::dsp::one_pole;
use crate::stmlib::dsp::hysteresis_quantizer::HysteresisQuantizer2;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct ChiptuneEngine {
    pub post_processing_settings: PostProcessingSettings,

    voice_: [SuperSquareOscillator; K_CHORD_NUM_VOICES],
    bass_: NESTriangleOscillator,

    chords_: ChordBank,
    arpeggiator_: Arpeggiator,
    arpeggiator_pattern_selector_: HysteresisQuantizer2,

    envelope_shape_: f32,
    envelope_state_: f32,
    aux_envelope_amount_: f32,
}

impl ChiptuneEngine {
    pub const NO_ENVELOPE: i32 = 2;

    #[inline]
    pub fn set_envelope_shape(&mut self, envelope_shape: f32) {
        self.envelope_shape_ = envelope_shape;
    }
}

impl Engine for ChiptuneEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.bass_.init();
        for i in 0..K_CHORD_NUM_NOTES {
            self.voice_[i].init();
        }

        self.chords_.init(allocator);

        self.arpeggiator_.init();

        self.arpeggiator_pattern_selector_.init(12, 0.075, false);

        self.envelope_shape_ = Self::NO_ENVELOPE as f32;
        self.envelope_state_ = 0.0;
        self.aux_envelope_amount_ = 0.0;
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
        already_enveloped: &mut bool,
        arena: &mut Arena,
        random: &mut Random,
    ) {
        let f0 = note_to_frequency(parameters.note);
        let shape = parameters.morph * 0.995;
        let clocked = (parameters.trigger & TRIGGER_UNPATCHED) == 0;
        let mut root_transposition = 1.0f32;

        *already_enveloped = clocked;

        if clocked {
            if parameters.trigger & TRIGGER_RISING_EDGE != 0 {
                self.chords_.set_chord(parameters.harmonics);
                self.chords_.sort(arena);

                let pattern = self
                    .arpeggiator_pattern_selector_
                    .process(parameters.timbre);
                self.arpeggiator_.set_mode(pattern / 3);
                self.arpeggiator_.set_range(1 << (pattern % 3));
                self.arpeggiator_
                    .clock(self.chords_.num_notes(arena), random);
                self.envelope_state_ = 1.0;
            }
            let octave = (1i32 << self.arpeggiator_.octave()) as f32;
            // The arpeggiator leaves its note in [0, num_notes), at most 4 notes.
            let note_f0 = f0
                * self
                    .chords_
                    .sorted_ratio(arena, self.arpeggiator_.note() as usize)
                * octave;
            root_transposition = octave;
            self.voice_[0].render(note_f0, shape, out, size);
        } else {
            let mut ratios = [0.0f32; K_CHORD_NUM_VOICES];
            let mut amplitudes = [0.0f32; K_CHORD_NUM_VOICES];

            self.chords_.set_chord(parameters.harmonics);
            self.chords_.compute_chord_inversion(
                arena,
                parameters.timbre,
                &mut ratios,
                &mut amplitudes,
            );
            for j in (1..K_CHORD_NUM_VOICES).step_by(2) {
                amplitudes[j] = -amplitudes[j];
            }

            out[..size].fill(0.0);
            for voice in 0..K_CHORD_NUM_VOICES {
                let voice_f0 = f0 * ratios[voice];
                self.voice_[voice].render(voice_f0, shape, aux, size);
                for j in 0..size {
                    out[j] += aux[j] * amplitudes[voice];
                }
            }
        }

        // Render bass note.
        self.bass_.render(f0 * 0.5 * root_transposition, aux, size);

        // Apply envelope if necessary.
        if self.envelope_shape_ != Self::NO_ENVELOPE as f32 {
            let shape = self.envelope_shape_.abs();
            let decay = 1.0 - 2.0 / K_SAMPLE_RATE * semitones_to_ratio(60.0 * shape) * shape;
            let mut aux_envelope_amount = self.envelope_shape_ * 20.0;
            constrain(&mut aux_envelope_amount, 0.0, 1.0);

            for i in 0..size {
                one_pole(&mut self.aux_envelope_amount_, aux_envelope_amount, 0.01);
                self.envelope_state_ *= decay;
                out[i] *= self.envelope_state_;
                aux[i] *= 1.0 + self.aux_envelope_amount_ * (self.envelope_state_ - 1.0);
            }
        }
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
