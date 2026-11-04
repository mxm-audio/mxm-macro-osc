//! What the port takes from C++ itself, with the C++'s exact semantics: `std::max`, `std::min`
//! (`<algorithm>`), an array read whose index comes from a float, and the reads before a table
//! that a case makes, of what the reference build keeps there.
//!
//! Not a ported file: these stand in for the language and its library where Rust's own
//! functions differ (`f32::max` and `f32::min` treat a NaN differently from `std::max` and
//! `std::min`, and indexing out of range panics), and for the build's memory where the C++
//! reads past a table.

use crate::resources::{LUT_LPC_EXCITATION_PULSE, LUT_SINE, SYX_BANK_2, WAV_INTEGRATED_WAVES};

/// `std::max(a, b)`: `(a < b) ? b : a`. With a NaN, the result is `a` unless `a < b`, which
/// `f32::max` does not do.
#[inline]
pub fn max<T: PartialOrd>(a: T, b: T) -> T {
    if a < b { b } else { a }
}

/// `std::min(a, b)`: `(b < a) ? b : a`.
#[inline]
pub fn min<T: PartialOrd>(a: T, b: T) -> T {
    if b < a { b } else { a }
}

/// `table[index]`, where the C++ indexes a table with an integer computed from a float.
///
/// In range it is the element. Out of range (a NaN, or a value beyond what the code was written
/// for) the C++ reads outside the array, which is undefined; the port returns `T::default()`
/// rather than panic, since `render` must not panic on any input. No case reaches it, but for
/// the reads before a table below.
#[inline]
pub fn at<T: Copy + Default>(table: &[T], index: i32) -> T {
    // A negative index becomes a huge usize, out of range like any other.
    table.get(index as usize).copied().unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// Before a table, in the reference build
//
// The reference (MSVC x64, `/O2`) keeps `resources.cc`'s tables in `.rdata` one after another,
// in the order the file defines them, and its tables of pointers (`fm_patches_table`,
// `lookup_table_table`, ...) elsewhere: read off the harness's `harness.exe` (2026-11-04), where
// the 8 192 bytes before `lut_sine` are `syx_bank_1` and `syx_bank_2`, and the 640 before
// `wav_integrated_waves` are `lut_lpc_excitation_pulse`. No relocation lies in them, so what a
// read before either table finds is the same in every run. A read further out, or after the
// end, returns 0, as [`at`].

/// `lut_sine[index]` as the reference build reads it: before the table, `syx_bank_2`'s last
/// bytes as little-endian floats. A negative phase gets there through `Sine`'s
/// `InterpolateWrap` (`e05-edges`: the wave terrain's AUX, once its path's y passes -1).
#[inline]
pub fn lut_sine_at(index: i32) -> f32 {
    if index >= 0 {
        return at(&LUT_SINE, index);
    }
    let start = SYX_BANK_2.len() as i64 + 4 * index as i64;
    match usize::try_from(start) {
        Ok(start) => f32::from_le_bytes([
            SYX_BANK_2[start],
            SYX_BANK_2[start + 1],
            SYX_BANK_2[start + 2],
            SYX_BANK_2[start + 3],
        ]),
        Err(_) => 0.0,
    }
}

/// `wav_integrated_waves[index]` as the reference build reads it: before the table,
/// `lut_lpc_excitation_pulse`'s last bytes as little-endian shorts. Once its path's y passes -1,
/// the wave terrain reads a sample before a wave's start: in the table, the end of the wave
/// before; before it only for the first bank's first wave, which no case reaches.
#[inline]
pub fn wav_integrated_waves_at(index: i32) -> i16 {
    if index >= 0 {
        return at(&WAV_INTEGRATED_WAVES, index);
    }
    let start = LUT_LPC_EXCITATION_PULSE.len() as i64 + 2 * index as i64;
    match usize::try_from(start) {
        Ok(start) => i16::from_le_bytes([
            LUT_LPC_EXCITATION_PULSE[start] as u8,
            LUT_LPC_EXCITATION_PULSE[start + 1] as u8,
        ]),
        Err(_) => 0,
    }
}
