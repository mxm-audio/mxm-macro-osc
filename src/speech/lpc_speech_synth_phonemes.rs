//! Ported from plaits/dsp/speech/lpc_speech_synth_phonemes.cc: "a table with vowels (a, e, i,
//! o, u) and a random selection of consonnants for the LPC10 speech synth", the C++'s
//! `LPCSpeechSynthController::phonemes_`.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).

use super::lpc_speech_synth::Frame;
use super::lpc_speech_synth_controller::K_LPC_SPEECH_SYNTH_NUM_PHONEMES;

/// `{ energy, period, k0, k1, k2, k3, k4, k5, k6, k7, k8, k9 }`, as the C++ initialises them.
const fn f(energy: u8, period: u8, k: (i16, i16, i8, i8, i8, i8, i8, i8, i8, i8)) -> Frame {
    Frame {
        energy,
        period,
        k0: k.0,
        k1: k.1,
        k2: k.2,
        k3: k.3,
        k4: k.4,
        k5: k.5,
        k6: k.6,
        k7: k.7,
        k8: k.8,
        k9: k.9,
    }
}

/// `LPCSpeechSynthController::phonemes_`.
pub static PHONEMES_: [Frame; K_LPC_SPEECH_SYNTH_NUM_PHONEMES] = [
    f(192, 80, (-18368, 11584, 52, 29, 23, 14, -17, 79, 37, 4)),
    f(192, 80, (-14528, 1536, 38, 29, 11, 14, -41, 79, 57, 4)),
    f(192, 80, (14528, 9216, 25, -54, -70, 36, 19, 79, 57, 22)),
    f(192, 80, (-14528, -13440, 38, 57, 57, 14, -53, 7, 37, 77)),
    f(192, 80, (-26368, 4160, 11, 15, -1, 36, -41, 31, 77, 22)),
    f(15, 0, (5184, 9216, -29, -12, 0, 0, 0, 0, 0, 0)),
    f(10, 0, (27968, 17856, 25, 43, -24, -20, -53, 55, -4, -51)),
    f(
        128,
        160,
        (14528, -3712, -43, -26, -24, -20, -53, 55, -4, -51),
    ),
    f(128, 160, (10048, 11584, -16, 15, 0, 0, 0, 0, 0, 0)),
    f(
        224,
        100,
        (18368, -13440, -97, -26, -12, -53, -41, 7, 57, 32),
    ),
    f(192, 80, (-10048, 9216, -70, 15, 34, -20, -17, 31, -24, 22)),
    f(96, 160, (-18368, 17856, -29, -12, -35, 3, -5, 7, 37, 22)),
    f(64, 80, (-21632, -6272, -83, 29, 57, 3, -5, 7, 16, 32)),
    f(192, 80, (0, -1088, 11, -26, -24, -9, -5, 55, 37, 22)),
    f(64, 80, (21632, -17536, -97, 85, 57, -20, -17, 31, -4, 59)),
];
