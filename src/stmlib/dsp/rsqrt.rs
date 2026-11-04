//! Ported from stmlib/dsp/rsqrt.h: fast reciprocal of square-root routines.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `unsafe_bit_cast` is `f32::to_bits` and `f32::from_bits`.

#[inline]
pub fn fast_rsqrt_carmack(x: f32) -> f32 {
    let threehalfs = 1.5f32;
    let mut y = x;
    let mut i = y.to_bits();
    i = 0x5f3759df_u32.wrapping_sub(i >> 1);
    y = f32::from_bits(i);
    let x2 = x * 0.5;
    y *= threehalfs - (x2 * y * y);
    y
}

/// `float _min = 1.0e-38;`: a `double` literal rounded to `float` when compiled, which gives a
/// subnormal float; a `const`, so it is the compiler's value too, whatever the run-time
/// flush-to-zero mode.
const MIN: f32 = 1.0e-38_f64 as f32;

#[inline]
pub fn fast_rsqrt_accurate(fp0: f32) -> f32 {
    let _1p5 = 1.5f32;

    let q = fp0.to_bits();
    let mut fp2 = f32::from_bits(0x5F3997BB_u32.wrapping_sub((q >> 1) & 0x3FFFFFFF));
    let fp1 = _1p5 * fp0 - fp0;
    let mut fp3 = fp2 * fp2;
    if fp0 < MIN {
        return if fp0 > 0.0 { fp2 } else { 1000.0 };
    }
    fp3 = _1p5 - fp1 * fp3;
    fp2 *= fp3;
    fp3 = fp2 * fp2;
    fp3 = _1p5 - fp1 * fp3;
    fp2 *= fp3;
    fp3 = fp2 * fp2;
    fp3 = _1p5 - fp1 * fp3;
    fp2 * fp3
}
