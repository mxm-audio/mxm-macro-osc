//! The generated tables against values read from the C++ sources by hand, and against what the
//! sources say generated them (`gen/tables.py` makes the files; this checks it independently).

use mxm_thanks_eg_engines::resources::*;
use mxm_thanks_eg_engines::speech::lpc_speech_synth_words::{
    BANK_0, BANK_4, LPC_SPEECH_SYNTH_NUM_WORD_BANKS, WORD_BANKS_,
};
use mxm_thanks_eg_engines::stmlib::dsp::units::{LUT_PITCH_RATIO_HIGH, LUT_PITCH_RATIO_LOW};

/// What the C++ compiler makes of an unsuffixed literal in a `float` table: the decimal as a
/// `double`, rounded to `float`.
fn c(literal: &str) -> f32 {
    literal.parse::<f64>().expect("a literal") as f32
}

#[test]
fn sizes_are_resources_h() {
    assert_eq!(SYX_BANK_0.len(), 4096);
    assert_eq!(SYX_BANK_1.len(), 4096);
    assert_eq!(SYX_BANK_2.len(), 4096);
    assert_eq!(LUT_SINE.len(), 641);
    assert_eq!(LUT_FM_FREQUENCY_QUANTIZER.len(), 130);
    assert_eq!(LUT_FOLD.len(), 516);
    assert_eq!(LUT_FOLD_2.len(), 516);
    assert_eq!(LUT_STIFFNESS.len(), 65);
    assert_eq!(LUT_SVF_SHIFT.len(), 257);
    assert_eq!(LUT_4X_DOWNSAMPLER_FIR.len(), 4);
    for table in LOOKUP_TABLE_I16_TABLE {
        assert_eq!(table.len(), 257);
    }
    assert_eq!(LUT_LPC_EXCITATION_PULSE.len(), 640);
    assert_eq!(WAV_INTEGRATED_WAVES.len(), 25344);
    assert_eq!(LOOKUP_TABLE_TABLE.len(), 7);
    assert_eq!(LOOKUP_TABLE_I32_TABLE.len(), 0);
}

#[test]
fn values_read_from_resources_cc() {
    assert_eq!(SYX_BANK_0[..4], [99, 58, 99, 60]);
    assert_eq!(LUT_SINE[0], 0.0);
    assert_eq!(LUT_SINE[1], c("1.227153829e-02"));
    assert_eq!(LUT_SINE[128], 1.0);
    assert_eq!(LUT_SINE[640], 1.0);
    assert_eq!(LUT_FM_FREQUENCY_QUANTIZER[..3], [-12.0, -12.0, -12.0]);
    assert_eq!(LUT_FM_FREQUENCY_QUANTIZER[3], c("-1.184000000e+01"));
    assert_eq!(
        LUT_4X_DOWNSAMPLER_FIR,
        [
            c("2.442415000e-02"),
            c("9.297315000e-02"),
            c("1.671293800e-01"),
            c("2.154733200e-01")
        ]
    );
    assert_eq!(LUT_WS_INVERSE_TAN[..4], [-32767, -26872, -24921, -23661]);
    assert_eq!(LUT_LPC_EXCITATION_PULSE[..8], [0, 0, 0, 0, 1, 1, 1, 2]);
    assert_eq!(WAV_INTEGRATED_WAVES[..4], [-20556, -20706, -20806, -20857]);
    assert_eq!(WAV_INTEGRATED_WAVES[25340..], [3044, 2181, 1253, 261]);
}

#[test]
fn pointer_tables_point_where_the_cc_does() {
    assert!(std::ptr::eq(FM_PATCHES_TABLE[0], &SYX_BANK_0[..]));
    assert!(std::ptr::eq(FM_PATCHES_TABLE[2], &SYX_BANK_2[..]));
    assert!(std::ptr::eq(LOOKUP_TABLE_TABLE[0], &LUT_SINE[..]));
    assert!(std::ptr::eq(
        LOOKUP_TABLE_TABLE[6],
        &LUT_4X_DOWNSAMPLER_FIR[..]
    ));
    // The sixth i16 table is the fifth again: `lut_ws_double_bump` twice.
    assert!(std::ptr::eq(
        LOOKUP_TABLE_I16_TABLE[4],
        &LUT_WS_DOUBLE_BUMP[..]
    ));
    assert!(std::ptr::eq(
        LOOKUP_TABLE_I16_TABLE[5],
        &LUT_WS_DOUBLE_BUMP[..]
    ));
    assert!(std::ptr::eq(WAVETABLES_TABLE[0], &WAV_INTEGRATED_WAVES[..]));
}

#[test]
fn units_tables() {
    assert_eq!(LUT_PITCH_RATIO_HIGH.len(), 257);
    assert_eq!(LUT_PITCH_RATIO_LOW.len(), 257);
    assert_eq!(LUT_PITCH_RATIO_HIGH[0], c("6.151958251e-04"));
    assert_eq!(LUT_PITCH_RATIO_HIGH[128], 1.0);
    assert_eq!(LUT_PITCH_RATIO_HIGH[255], c("1.534266447e+03"));
    assert_eq!(LUT_PITCH_RATIO_LOW[1], c("1.000225659e+00"));
    assert_eq!(LUT_PITCH_RATIO_LOW[255], c("1.059224071e+00"));
    // `units.h` declares both `[257]` and `units.cc` gives 256 values: the last is zero.
    assert_eq!(LUT_PITCH_RATIO_HIGH[256], 0.0);
    assert_eq!(LUT_PITCH_RATIO_LOW[256], 0.0);
    // And they are what units.cc's "Original python code" says: 2^((i - 128) / 12) and
    // 2^(i / 256 / 12), to the ten digits the file keeps.
    for i in 0..256 {
        let high = 2f64.powf((i as f64 - 128.0) / 12.0);
        let low = 2f64.powf(i as f64 / 256.0 / 12.0);
        assert!(
            (LUT_PITCH_RATIO_HIGH[i] as f64 / high - 1.0).abs() < 1e-7,
            "high {i}"
        );
        assert!(
            (LUT_PITCH_RATIO_LOW[i] as f64 / low - 1.0).abs() < 1e-7,
            "low {i}"
        );
    }
}

#[test]
fn the_sine_is_a_sine() {
    // lookup_tables.py: 512 samples a cycle, plus a quarter and one guard sample.
    for (i, &value) in LUT_SINE.iter().enumerate() {
        let sine = (2.0 * std::f64::consts::PI * i as f64 / 512.0).sin();
        // Ten digits, then rounded to float: within half a float step of 1.
        assert!((value as f64 - sine).abs() < 6e-8, "{i}");
    }
}

#[test]
fn words() {
    assert_eq!(LPC_SPEECH_SYNTH_NUM_WORD_BANKS, 5);
    assert_eq!(
        BANK_0[..8],
        [0x30, 0x4a, 0x57, 0xd8, 0x23, 0x2c, 0xc9, 0xa9]
    );
    assert_eq!(BANK_4[BANK_4.len() - 2..], [0xe0, 0xff]);
    let sizes: Vec<usize> = WORD_BANKS_.iter().map(|b| b.size).collect();
    assert_eq!(sizes, [1233, 900, 1552, 2524, 4802]);
    for bank in &WORD_BANKS_ {
        assert_eq!(bank.data.len(), bank.size);
    }
}
