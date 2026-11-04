//! Ported from plaits/dsp/dsp.h: the rates and block sizes.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).

pub const K_SAMPLE_RATE: f32 = 48000.0;

// There is no proper PLL for I2S, only a divider on the system clock to derive
// the bit clock.
// The division ratio is set to 47 (23 EVEN, 1 ODD) by the ST libraries.
//
// Bit clock = 72000000 / 47 = 1531.91 kHz
// Frame clock = Bit clock / 32 = 47872.34 Hz
//
// That's only 4.6 cts of error, but we care!

/// **The one change from the C++**: 48 000 where the module takes its DAC's 47 872.34 Hz, since
/// the port runs at exactly 48 kHz (the crate's `AGENTS.md`).
pub const K_CORRECTED_SAMPLE_RATE: f32 = 48000.0;
pub const A0: f32 = (440.0 / 8.0) / K_CORRECTED_SAMPLE_RATE;

pub const K_MAX_BLOCK_SIZE: usize = 24;
pub const K_BLOCK_SIZE: usize = 12;
