//! Ported from plaits/dsp/speech/sam_speech_synth.{h,cc}: SAM-inspired speech synth (as used
//! in Shruthi/Ambika/Braids).
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The speech engine passes `consonant` as `trigger == TRIGGER_RISING_EDGE`, which the voice
//! never sends (a rising edge comes with `TRIGGER_HIGH`), so the consonant is never heard
//! through the voice; it is ported as it is.

use crate::cpp::{at, min};
use crate::dsp::K_SAMPLE_RATE;
use crate::oscillator::sine_oscillator::sine_raw;
use crate::stmlib::dsp::dsp::{cast_i32, cast_u32, cast_usize, make_integral_fractional};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::polyblep::{next_blep_sample, this_blep_sample};

pub const K_SAM_NUM_FORMANTS: usize = 3;
pub const K_SAM_NUM_VOWELS: i32 = 9;
pub const K_SAM_NUM_CONSONANTS: i32 = 8;
pub const K_SAM_NUM_PHONEMES: i32 = K_SAM_NUM_VOWELS + K_SAM_NUM_CONSONANTS;

#[derive(Clone, Copy, Default, Debug)]
struct Formant {
    frequency: u8,
    amplitude: u8,
}

#[derive(Clone, Copy, Default, Debug)]
struct Phoneme {
    formant: [Formant; K_SAM_NUM_FORMANTS],
}

#[derive(Default)]
pub struct SAMSpeechSynth {
    phase_: f32,
    frequency_: f32,

    pulse_next_sample_: f32,
    pulse_lp_: f32,

    formant_phase_: [u32; 3],
    consonant_samples_: usize,
    consonant_index_: f32,
}

/// `{ { { frequency, amplitude }, … } }`, three formants.
const fn p(formants: [(u8, u8); K_SAM_NUM_FORMANTS]) -> Phoneme {
    Phoneme {
        formant: [
            Formant {
                frequency: formants[0].0,
                amplitude: formants[0].1,
            },
            Formant {
                frequency: formants[1].0,
                amplitude: formants[1].1,
            },
            Formant {
                frequency: formants[2].0,
                amplitude: formants[2].1,
            },
        ],
    }
}

// Phoneme data

/// `SAMSpeechSynth::phonemes_[kSAMNumPhonemes + 1]`.
static PHONEMES_: [Phoneme; K_SAM_NUM_PHONEMES as usize + 1] = [
    p([(60, 15), (90, 13), (200, 1)]),
    p([(40, 13), (114, 12), (139, 6)]),
    p([(33, 14), (155, 12), (209, 7)]),
    p([(22, 13), (189, 10), (247, 8)]),
    p([(51, 15), (99, 12), (195, 1)]),
    p([(29, 13), (65, 8), (180, 0)]),
    p([(13, 12), (103, 3), (182, 0)]),
    p([(20, 15), (114, 3), (213, 0)]),
    p([(13, 7), (164, 3), (222, 14)]),
    p([(13, 9), (121, 9), (254, 0)]),
    p([(40, 12), (112, 10), (114, 5)]),
    p([(24, 13), (54, 8), (157, 0)]),
    p([(33, 14), (155, 12), (166, 7)]),
    p([(36, 14), (83, 8), (249, 1)]),
    p([(40, 14), (114, 12), (139, 6)]),
    p([(13, 5), (58, 5), (182, 5)]),
    p([(13, 7), (164, 10), (222, 14)]),
    p([(13, 7), (164, 10), (222, 14)]), // GUARD
];

/// `SAMSpeechSynth::formant_amplitude_lut`.
static FORMANT_AMPLITUDE_LUT: [f32; 16] = [
    0.03125000, 0.03756299, 0.04515131, 0.05427259, 0.06523652, 0.07841532, 0.09425646, 0.11329776,
    0.13618570, 0.16369736, 0.19676682, 0.23651683, 0.28429697, 0.34172946, 0.41076422, 0.49374509,
];

impl SAMSpeechSynth {
    pub fn init(&mut self) {
        self.phase_ = 0.0;
        self.frequency_ = 0.0;
        self.pulse_next_sample_ = 0.0;
        self.pulse_lp_ = 0.0;

        self.formant_phase_.fill(0);
        self.consonant_samples_ = 0;
        self.consonant_index_ = 0.0;
    }

    /// `InterpolatePhonemeData(phoneme, formant_shift, formant_frequency, formant_amplitude)`;
    /// it reads no member, so it is an associated function.
    fn interpolate_phoneme_data(
        phoneme: f32,
        mut formant_shift: f32,
        formant_frequency: &mut [u32; K_SAM_NUM_FORMANTS],
        formant_amplitude: &mut [f32; K_SAM_NUM_FORMANTS],
    ) {
        let (phoneme_integral, phoneme_fractional) = make_integral_fractional(phoneme);

        let p_1 = at(&PHONEMES_, phoneme_integral);
        let p_2 = at(&PHONEMES_, phoneme_integral.wrapping_add(1));

        formant_shift = 1.0 + formant_shift * 2.5;
        for i in 0..K_SAM_NUM_FORMANTS {
            let f_1 = p_1.formant[i].frequency as f32;
            let f_2 = p_2.formant[i].frequency as f32;
            let mut f = f_1 + (f_2 - f_1) * phoneme_fractional;
            f *= 8.0 * formant_shift * 4294967296.0 / K_SAMPLE_RATE;
            formant_frequency[i] = cast_u32(f);

            let a_1 = FORMANT_AMPLITUDE_LUT[p_1.formant[i].amplitude as usize];
            let a_2 = FORMANT_AMPLITUDE_LUT[p_2.formant[i].amplitude as usize];
            formant_amplitude[i] = a_1 + (a_2 - a_1) * phoneme_fractional;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        consonant: bool,
        mut frequency: f32,
        vowel: f32,
        formant_shift: f32,
        excitation: &mut [f32],
        output: &mut [f32],
        size: usize,
    ) {
        if frequency >= 0.0625 {
            frequency = 0.0625;
        }

        if consonant {
            self.consonant_samples_ = cast_usize(K_SAMPLE_RATE * 0.05);
            let r = cast_i32((vowel + 3.0 * frequency + 7.0 * formant_shift) * 8.0);
            self.consonant_index_ = r.wrapping_rem(K_SAM_NUM_CONSONANTS) as f32;
        }
        self.consonant_samples_ -= min(self.consonant_samples_, size);

        let phoneme = if self.consonant_samples_ != 0 {
            self.consonant_index_ + K_SAM_NUM_VOWELS as f32
        } else {
            vowel * (K_SAM_NUM_VOWELS as f32 - 1.0001)
        };

        let mut formant_frequency = [0u32; K_SAM_NUM_FORMANTS];
        let mut formant_amplitude = [0.0f32; K_SAM_NUM_FORMANTS];

        Self::interpolate_phoneme_data(
            phoneme,
            formant_shift,
            &mut formant_frequency,
            &mut formant_amplitude,
        );

        let mut fm = ParameterInterpolator::new(&mut self.frequency_, frequency, size);
        let mut pulse_next_sample = self.pulse_next_sample_;

        for (excitation, output) in excitation[..size].iter_mut().zip(&mut output[..size]) {
            let mut pulse_this_sample = pulse_next_sample;
            pulse_next_sample = 0.0;
            let frequency = fm.next();
            self.phase_ += frequency;

            if self.phase_ >= 1.0 {
                self.phase_ -= 1.0;
                let t = self.phase_ / frequency;
                self.formant_phase_[0] = cast_u32(t * formant_frequency[0] as f32);
                self.formant_phase_[1] = cast_u32(t * formant_frequency[1] as f32);
                self.formant_phase_[2] = cast_u32(t * formant_frequency[2] as f32);
                pulse_this_sample -= this_blep_sample(t);
                pulse_next_sample -= next_blep_sample(t);
            } else {
                self.formant_phase_[0] = self.formant_phase_[0].wrapping_add(formant_frequency[0]);
                self.formant_phase_[1] = self.formant_phase_[1].wrapping_add(formant_frequency[1]);
                self.formant_phase_[2] = self.formant_phase_[2].wrapping_add(formant_frequency[2]);
            }
            pulse_next_sample += self.phase_;

            let d = pulse_this_sample - 0.5 - self.pulse_lp_;
            self.pulse_lp_ += min(16.0 * frequency, 1.0) * d;
            *excitation = d;

            let mut s = 0.0f32;
            s += sine_raw(self.formant_phase_[0]) * formant_amplitude[0];
            s += sine_raw(self.formant_phase_[1]) * formant_amplitude[1];
            s += sine_raw(self.formant_phase_[2]) * formant_amplitude[2];
            s *= 1.0 - self.phase_;
            *output = s;
        }
        self.pulse_next_sample_ = pulse_next_sample;
    }
}
