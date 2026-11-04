//! Ported from plaits/dsp/chords/chord_bank.{h,cc}: chord bank shared by several engines.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The ratios, note counts and sorted ratios live in the arena, as in the C++: `init` allocates
//! them and `reset` recomputes them, so the methods that read them take the arena. `JON_CHORDS`
//! is not defined in the C++ (`chord_bank.h`), so its alternative table is left out.

use crate::cpp::at;
use crate::stmlib::dsp::dsp::make_integral_fractional;
use crate::stmlib::dsp::hysteresis_quantizer::HysteresisQuantizer2;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};

pub const K_CHORD_NUM_NOTES: usize = 4;
pub const K_CHORD_NUM_VOICES: usize = K_CHORD_NUM_NOTES + 1;

pub const K_CHORD_NUM_CHORDS: usize = 11;

#[derive(Default)]
pub struct ChordBank {
    chord_index_quantizer_: HysteresisQuantizer2,

    /// Arena offset of `kChordNumChords × kChordNumNotes` floats.
    ratios_: usize,
    /// Arena offset of `kChordNumNotes` floats.
    sorted_ratios_: usize,
    /// Arena offset of `kChordNumChords` ints.
    note_count_: usize,
}

impl ChordBank {
    const CHORDS_: [[f32; K_CHORD_NUM_NOTES]; K_CHORD_NUM_CHORDS] = [
        [0.00, 0.01, 11.99, 12.00], // OCT
        [0.00, 7.00, 7.01, 12.00],  // 5
        [0.00, 5.00, 7.00, 12.00],  // sus4
        [0.00, 3.00, 7.00, 12.00],  // m
        [0.00, 3.00, 7.00, 10.00],  // m7
        [0.00, 3.00, 10.00, 14.00], // m9
        [0.00, 3.00, 10.00, 17.00], // m11
        [0.00, 2.00, 9.00, 16.00],  // 69
        [0.00, 4.00, 11.00, 14.00], // M9
        [0.00, 4.00, 7.00, 11.00],  // M7
        [0.00, 4.00, 7.00, 12.00],  // M
    ];

    pub fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.ratios_ = allocator
            .allocate::<f32>(K_CHORD_NUM_CHORDS * K_CHORD_NUM_NOTES)
            .expect("the arena holds the chord ratios");
        self.note_count_ = allocator
            .allocate::<i32>(K_CHORD_NUM_CHORDS)
            .expect("the arena holds the chord note counts");
        self.sorted_ratios_ = allocator
            .allocate::<f32>(K_CHORD_NUM_NOTES)
            .expect("the arena holds the sorted ratios");

        self.chord_index_quantizer_
            .init(K_CHORD_NUM_CHORDS as i32, 0.075, false);
    }

    pub fn reset(&mut self, arena: &mut Arena) {
        for i in 0..K_CHORD_NUM_CHORDS {
            let mut count = 0;
            for j in 0..K_CHORD_NUM_NOTES {
                arena.view_mut::<f32>(self.ratios_, K_CHORD_NUM_CHORDS * K_CHORD_NUM_NOTES)
                    [i * K_CHORD_NUM_NOTES + j] = semitones_to_ratio(Self::CHORDS_[i][j]);
                let c = Self::CHORDS_[i][j];
                if c != 0.01 && c != 7.01 && c != 11.99 && c != 12.00 {
                    count += 1;
                }
            }
            arena.view_mut::<i32>(self.note_count_, K_CHORD_NUM_CHORDS)[i] = count;
        }
        self.sort(arena);
    }

    /// `ComputeChordInversion(inversion, ratios, amplitudes)`: `ratios` and `amplitudes` hold
    /// `kChordNumVoices` values.
    pub fn compute_chord_inversion(
        &self,
        arena: &Arena,
        mut inversion: f32,
        ratios: &mut [f32; K_CHORD_NUM_VOICES],
        amplitudes: &mut [f32; K_CHORD_NUM_VOICES],
    ) -> i32 {
        let base_ratio = self.ratios(arena);
        inversion *= (K_CHORD_NUM_NOTES * K_CHORD_NUM_VOICES) as f32;

        let (inversion_integral, inversion_fractional) = make_integral_fractional(inversion);

        let num_notes = K_CHORD_NUM_NOTES as i32;
        let num_voices = K_CHORD_NUM_VOICES as i32;
        let num_rotations = inversion_integral / num_notes;
        let rotated_note = inversion_integral % num_notes;

        const K_BASE_GAIN: f32 = 0.25;

        let mut mask = 0i32;

        // An index the C++ computes from the inversion: in range for every inversion in [0, 1].
        // Out of range (only a NaN gets there) the C++ writes outside the array; the port drops
        // the write rather than panic.
        fn voice(index: i32) -> Option<usize> {
            usize::try_from(index)
                .ok()
                .filter(|&v| v < K_CHORD_NUM_VOICES)
        }

        for i in 0..num_notes {
            // A shift by the low five bits of the count, as x64's `shl` does (only a NaN gets
            // the count out of range, where the C++ is undefined).
            let transposition = 0.25
                * 1i32.wrapping_shl(
                    ((num_notes - 1)
                        .wrapping_add(inversion_integral)
                        .wrapping_sub(i)
                        / num_notes) as u32,
                ) as f32;
            let target_voice =
                (i.wrapping_sub(num_rotations).wrapping_add(num_voices)) % num_voices;
            let previous_voice = (target_voice - 1 + num_voices) % num_voices;
            let target = voice(target_voice);
            let previous = voice(previous_voice);

            if i == rotated_note {
                if let (Some(t), Some(p)) = (target, previous) {
                    ratios[t] = base_ratio[i as usize] * transposition;
                    ratios[p] = ratios[t] * 2.0;
                    amplitudes[p] = K_BASE_GAIN * inversion_fractional;
                    amplitudes[t] = K_BASE_GAIN * (1.0 - inversion_fractional);
                }
            } else if i < rotated_note {
                if let Some(p) = previous {
                    ratios[p] = base_ratio[i as usize] * transposition;
                    amplitudes[p] = K_BASE_GAIN;
                }
            } else if let Some(t) = target {
                ratios[t] = base_ratio[i as usize] * transposition;
                amplitudes[t] = K_BASE_GAIN;
            }

            if i == 0 {
                if i >= rotated_note {
                    mask |= 1i32.wrapping_shl(target_voice as u32);
                }
                if i <= rotated_note {
                    mask |= 1i32.wrapping_shl(previous_voice as u32);
                }
            }
        }
        mask
    }

    #[inline]
    pub fn sort(&mut self, arena: &mut Arena) {
        for i in 0..K_CHORD_NUM_NOTES {
            let mut r = self.ratio(arena, i);
            while r > 2.0 {
                r *= 0.5;
            }
            arena.view_mut::<f32>(self.sorted_ratios_, K_CHORD_NUM_NOTES)[i] = r;
        }
        // `std::sort` of four floats: MSVC's insertion sort below 32 elements. None is a NaN
        // (they are the table's ratios), so any sort gives these bits.
        let sorted = arena.view_mut::<f32>(self.sorted_ratios_, K_CHORD_NUM_NOTES);
        for i in 1..K_CHORD_NUM_NOTES {
            let value = sorted[i];
            let mut j = i;
            while j > 0 && value < sorted[j - 1] {
                sorted[j] = sorted[j - 1];
                j -= 1;
            }
            sorted[j] = value;
        }
    }

    #[inline]
    pub fn set_chord(&mut self, parameter: f32) {
        self.chord_index_quantizer_.process(parameter * 1.02);
    }

    #[inline]
    pub fn chord_index(&self) -> i32 {
        self.chord_index_quantizer_.quantized_value()
    }

    /// `ratios()`: the current chord's `kChordNumNotes` ratios.
    #[inline]
    pub fn ratios<'a>(&self, arena: &'a Arena) -> &'a [f32] {
        let start = self.chord_index() as usize * K_CHORD_NUM_NOTES;
        &arena.view::<f32>(self.ratios_, K_CHORD_NUM_CHORDS * K_CHORD_NUM_NOTES)
            [start..start + K_CHORD_NUM_NOTES]
    }

    #[inline]
    pub fn ratio(&self, arena: &Arena, note: usize) -> f32 {
        self.ratios(arena)[note]
    }

    #[inline]
    pub fn sorted_ratio(&self, arena: &Arena, note: usize) -> f32 {
        arena.view::<f32>(self.sorted_ratios_, K_CHORD_NUM_NOTES)[note]
    }

    #[inline]
    pub fn num_notes(&self, arena: &Arena) -> i32 {
        at(
            arena.view::<i32>(self.note_count_, K_CHORD_NUM_CHORDS),
            self.chord_index(),
        )
    }
}
