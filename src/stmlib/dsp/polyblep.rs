//! Ported from stmlib/dsp/polyblep.h: polynomial approximation of band-limited step for
//! band-limited waveform synthesis.
//!
//! Copyright 2017 Emilie Gillet. MIT licence (`NOTICE.md`).

#[inline]
pub fn this_blep_sample(t: f32) -> f32 {
    0.5 * t * t
}

#[inline]
pub fn next_blep_sample(mut t: f32) -> f32 {
    t = 1.0 - t;
    -0.5 * t * t
}

#[inline]
pub fn next_integrated_blep_sample(t: f32) -> f32 {
    let t1 = 0.5 * t;
    let t2 = t1 * t1;
    let t4 = t2 * t2;
    0.1875 - t1 + 1.5 * t2 - t4
}

#[inline]
pub fn this_integrated_blep_sample(t: f32) -> f32 {
    next_integrated_blep_sample(1.0 - t)
}
