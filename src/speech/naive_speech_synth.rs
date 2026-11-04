//! Ported from plaits/dsp/speech/naive_speech_synth.{h,cc}: naive speech synth - made from
//! "synthesizer" building blocks (pulse oscillator and zero-delay SVF).
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The speech engine passes `click` as `trigger == TRIGGER_RISING_EDGE`, which the voice never
//! sends (a rising edge comes with `TRIGGER_HIGH`), so the click is never heard through the
//! voice; it is ported as it is.

use crate::cpp::{at, min};
use crate::dsp::{A0, K_SAMPLE_RATE};
use crate::oscillator::oscillator::{OSCILLATOR_SHAPE_IMPULSE_TRAIN, Oscillator};
use crate::stmlib::dsp::dsp::{cast_usize, make_integral_fractional};
use crate::stmlib::dsp::filter::{FILTER_MODE_BAND_PASS, FREQUENCY_DIRTY, Svf};
use crate::stmlib::dsp::units::semitones_to_ratio;

pub const K_NAIVE_SPEECH_NUM_FORMANTS: usize = 5;
pub const K_NAIVE_SPEECH_NUM_PHONEMES: i32 = 5;
pub const K_NAIVE_SPEECH_NUM_REGISTERS: i32 = 5;

#[derive(Clone, Copy, Default, Debug)]
struct Formant {
    frequency: u8,
    amplitude: u8,
}

#[derive(Clone, Copy, Default, Debug)]
struct Phoneme {
    formant: [Formant; K_NAIVE_SPEECH_NUM_FORMANTS],
}

#[derive(Default)]
pub struct NaiveSpeechSynth {
    pulse_: Oscillator,
    /// Set by `init` and never read, as in the C++.
    #[allow(dead_code)]
    frequency_: f32,
    click_duration_: usize,

    filter_: [Svf; K_NAIVE_SPEECH_NUM_FORMANTS],
    pulse_coloration_: Svf,
}

/// `{ { { frequency, amplitude }, … } }`, five formants.
const fn p(formants: [(u8, u8); K_NAIVE_SPEECH_NUM_FORMANTS]) -> Phoneme {
    let mut formant = [Formant {
        frequency: 0,
        amplitude: 0,
    }; K_NAIVE_SPEECH_NUM_FORMANTS];
    let mut i = 0;
    while i < K_NAIVE_SPEECH_NUM_FORMANTS {
        formant[i] = Formant {
            frequency: formants[i].0,
            amplitude: formants[i].1,
        };
        i += 1;
    }
    Phoneme { formant }
}

/// `NaiveSpeechSynth::phonemes_[kNaiveSpeechNumPhonemes][kNaiveSpeechNumRegisters]`.
static PHONEMES_: [[Phoneme; K_NAIVE_SPEECH_NUM_REGISTERS as usize];
    K_NAIVE_SPEECH_NUM_PHONEMES as usize] = [
    [
        p([(74, 255), (83, 114), (97, 90), (98, 90), (100, 25)]),
        p([(75, 255), (84, 128), (100, 114), (101, 101), (103, 20)]),
        p([(76, 255), (85, 128), (100, 18), (102, 16), (104, 3)]),
        p([(79, 255), (85, 161), (101, 25), (104, 4), (110, 0)]),
        p([(79, 255), (85, 128), (101, 6), (106, 25), (110, 0)]),
    ],
    [
        p([(67, 255), (91, 64), (98, 90), (101, 64), (102, 32)]),
        p([(67, 255), (92, 51), (99, 64), (103, 51), (105, 25)]),
        p([(69, 255), (93, 51), (100, 32), (102, 25), (103, 25)]),
        p([(67, 255), (91, 16), (100, 8), (103, 4), (110, 0)]),
        p([(65, 255), (95, 25), (101, 45), (105, 2), (110, 0)]),
    ],
    [
        p([(59, 255), (92, 8), (99, 40), (102, 20), (104, 10)]),
        p([(61, 255), (94, 45), (101, 32), (103, 25), (105, 8)]),
        p([(60, 255), (93, 16), (101, 16), (104, 4), (105, 4)]),
        p([(65, 255), (92, 25), (100, 8), (105, 4), (110, 0)]),
        p([(60, 255), (96, 64), (101, 12), (106, 12), (110, 1)]),
    ],
    [
        p([(67, 255), (78, 72), (98, 22), (99, 25), (101, 2)]),
        p([(67, 255), (79, 80), (99, 64), (101, 64), (102, 12)]),
        p([(68, 255), (79, 80), (100, 12), (102, 20), (103, 5)]),
        p([(69, 255), (79, 90), (101, 40), (104, 10), (110, 0)]),
        p([(69, 255), (79, 72), (101, 20), (106, 20), (110, 0)]),
    ],
    [
        p([(65, 255), (74, 25), (98, 6), (100, 10), (101, 4)]),
        p([(65, 255), (74, 25), (100, 36), (101, 51), (103, 12)]),
        p([(66, 255), (75, 25), (100, 18), (102, 8), (104, 5)]),
        p([(63, 255), (77, 64), (99, 8), (104, 2), (110, 0)]),
        p([(63, 255), (77, 40), (100, 4), (106, 2), (110, 0)]),
    ],
];

impl NaiveSpeechSynth {
    pub fn init(&mut self) {
        self.pulse_.init();
        self.frequency_ = 0.0;
        self.click_duration_ = 0;

        for filter in &mut self.filter_ {
            filter.init();
        }
        self.pulse_coloration_.init();
        self.pulse_coloration_
            .set_f_q::<FREQUENCY_DIRTY>(800.0 / K_SAMPLE_RATE, 0.5);
    }

    /// `temp` is unused, as in the C++.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        click: bool,
        mut frequency: f32,
        phoneme: f32,
        vocal_register: f32,
        _temp: &mut [f32],
        excitation: &mut [f32],
        output: &mut [f32],
        size: usize,
    ) {
        if click {
            self.click_duration_ = cast_usize(K_SAMPLE_RATE * 0.05);
        }
        self.click_duration_ -= min(self.click_duration_, size);

        if self.click_duration_ != 0 {
            frequency *= 0.5;
        }

        // Generate excitation signal (glottal pulse).
        self.pulse_
            .render::<OSCILLATOR_SHAPE_IMPULSE_TRAIN>(frequency, 0.5, excitation, size);
        self.pulse_coloration_
            .process_buffer_in_place::<FILTER_MODE_BAND_PASS>(excitation, size);
        for sample in &mut excitation[..size] {
            *sample *= 4.0;
        }

        let p = phoneme * (K_NAIVE_SPEECH_NUM_PHONEMES as f32 - 1.001);
        let r = vocal_register * (K_NAIVE_SPEECH_NUM_REGISTERS as f32 - 1.001);

        let (p_integral, p_fractional) = make_integral_fractional(p);
        let (r_integral, r_fractional) = make_integral_fractional(r);

        output[..size].fill(0.0);
        let p0 = at(&PHONEMES_, p_integral);
        let p1 = at(&PHONEMES_, p_integral.wrapping_add(1));
        for i in 0..K_NAIVE_SPEECH_NUM_FORMANTS {
            let p0r0 = at(&p0, r_integral).formant[i];
            let p0r1 = at(&p0, r_integral.wrapping_add(1)).formant[i];
            let p1r0 = at(&p1, r_integral).formant[i];
            let p1r1 = at(&p1, r_integral.wrapping_add(1)).formant[i];

            let p0r_f = p0r0.frequency as f32
                + (p0r1.frequency as i32 - p0r0.frequency as i32) as f32 * r_fractional;
            let p1r_f = p1r0.frequency as f32
                + (p1r1.frequency as i32 - p1r0.frequency as i32) as f32 * r_fractional;
            let mut f = p0r_f + (p1r_f - p0r_f) * p_fractional;

            let p0r_a = p0r0.amplitude as f32
                + (p0r1.amplitude as i32 - p0r0.amplitude as i32) as f32 * r_fractional;
            let p1r_a = p1r0.amplitude as f32
                + (p1r1.amplitude as i32 - p1r0.amplitude as i32) as f32 * r_fractional;
            let a = (p0r_a + (p1r_a - p0r_a) * p_fractional) / 256.0;

            if f >= 160.0 {
                f = 160.0;
            }
            f = A0 * semitones_to_ratio(f - 33.0);
            if self.click_duration_ != 0 && i == 0 {
                f *= 0.5;
            }
            self.filter_[i].set_f_q::<FREQUENCY_DIRTY>(f, 20.0);
            self.filter_[i].process_add::<FILTER_MODE_BAND_PASS>(excitation, output, size, a);
        }
    }
}
