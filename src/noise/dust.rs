//! Ported from plaits/dsp/noise/dust.h: randomly clocked samples.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `GetFloat` can return exactly 1.0, which `u < frequency` then compares as the C++ does.

use crate::stmlib::utils::random::Random;

/// `Dust(frequency)`: one draw from the voice's `Random`.
#[inline]
pub fn dust(frequency: f32, random: &mut Random) -> f32 {
    let inv_frequency = 1.0 / frequency;
    let u = random.get_float();
    if u < frequency {
        u * inv_frequency
    } else {
        0.0
    }
}
