//! Ported from stmlib/utils/dsp.h: basic operands for fixed-point arithmetic.
//!
//! Copyright 2012 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The C++ overloads are named for their table's type. The arithmetic is C++'s: `int16_t`,
//! `uint16_t` and `uint8_t` operands are promoted to `int`, whose overflow wraps here (the C++'s
//! is undefined, and wraps on every target it runs on); `uint32_t` arithmetic wraps, and its
//! right shift is logical. Where `utils/dsp.h` declares an overload and defines another (the
//! `uint8_t` `Crossfade1022` and `Crossfade115`, the `uint32_t` `Interpolate115`), the defined
//! one is ported, as the declared ones have no body to call.

/// `Mix(int16_t a, int16_t b, uint16_t balance)`.
#[inline]
pub fn mix_i16(a: i16, b: i16, balance: u16) -> i16 {
    let a = a as i32;
    let b = b as i32;
    let balance = balance as i32;
    (a.wrapping_mul(65535 - balance)
        .wrapping_add(b.wrapping_mul(balance))
        >> 16) as i16
}

/// `Mix(uint16_t a, uint16_t b, uint16_t balance)`.
#[inline]
pub fn mix_u16(a: u16, b: u16, balance: u16) -> u16 {
    let a = a as i32;
    let b = b as i32;
    let balance = balance as i32;
    (a.wrapping_mul(65535 - balance)
        .wrapping_add(b.wrapping_mul(balance))
        >> 16) as u16
}

/// `Interpolate824(const int16_t* table, uint32_t phase)`.
#[inline]
pub fn interpolate824_i16(table: &[i16], phase: u32) -> i16 {
    let a = table[(phase >> 24) as usize] as i32;
    let b = table[((phase >> 24) + 1) as usize] as i32;
    a.wrapping_add((b - a).wrapping_mul(((phase >> 8) & 0xffff) as i32) >> 16) as i16
}

/// `Interpolate824(const uint16_t* table, uint32_t phase)`.
#[inline]
pub fn interpolate824_u16(table: &[u16], phase: u32) -> u16 {
    let a = table[(phase >> 24) as usize] as u32;
    let b = table[((phase >> 24) + 1) as usize] as u32;
    a.wrapping_add(b.wrapping_sub(a).wrapping_mul((phase >> 8) & 0xffff) >> 16) as u16
}

/// `Interpolate824(const uint8_t* table, uint32_t phase)`.
#[inline]
pub fn interpolate824_u8(table: &[u8], phase: u32) -> i16 {
    let a = table[(phase >> 24) as usize] as i32;
    let b = table[((phase >> 24) + 1) as usize] as i32;
    ((a << 8)
        .wrapping_add((b - a).wrapping_mul((phase & 0xffffff) as i32) >> 16)
        .wrapping_sub(32768)) as i16
}

/// `Interpolate88(const uint16_t* table, uint16_t index)`.
#[inline]
pub fn interpolate88_u16(table: &[u16], index: u16) -> u16 {
    let a = table[(index >> 8) as usize] as i32;
    let b = table[((index >> 8) + 1) as usize] as i32;
    (a + (((b - a) * (index & 0xff) as i32) >> 8)) as u16
}

/// `Interpolate88(const int16_t* table, uint16_t index)`.
#[inline]
pub fn interpolate88_i16(table: &[i16], index: u16) -> i16 {
    let a = table[(index >> 8) as usize] as i32;
    let b = table[((index >> 8) + 1) as usize] as i32;
    (a + (((b - a) * (index & 0xff) as i32) >> 8)) as i16
}

/// `Interpolate1022(const int16_t* table, uint32_t phase)`.
#[inline]
pub fn interpolate1022(table: &[i16], phase: u32) -> i16 {
    let a = table[(phase >> 22) as usize] as i32;
    let b = table[((phase >> 22) + 1) as usize] as i32;
    a.wrapping_add((b - a).wrapping_mul(((phase >> 6) & 0xffff) as i32) >> 16) as i16
}

/// `Interpolate115(const int16_t* table, uint16_t phase)`.
#[inline]
pub fn interpolate115(table: &[i16], phase: u16) -> i16 {
    let a = table[(phase >> 5) as usize] as i32;
    let b = table[((phase >> 5) + 1) as usize] as i32;
    (a + (((b - a) * (phase & 0x1f) as i32) >> 5)) as i16
}

/// `Crossfade(const int16_t* table_a, const int16_t* table_b, uint32_t phase, uint16_t
/// balance)`.
#[inline]
pub fn crossfade_i16(table_a: &[i16], table_b: &[i16], phase: u32, balance: u16) -> i16 {
    let a = interpolate824_i16(table_a, phase) as i32;
    let b = interpolate824_i16(table_b, phase) as i32;
    a.wrapping_add((b - a).wrapping_mul(balance as i32) >> 16) as i16
}

/// `Crossfade(const uint8_t* table_a, const uint8_t* table_b, uint32_t phase, uint16_t
/// balance)`.
#[inline]
pub fn crossfade_u8(table_a: &[u8], table_b: &[u8], phase: u32, balance: u16) -> i16 {
    let a = interpolate824_u8(table_a, phase) as i32;
    let b = interpolate824_u8(table_b, phase) as i32;
    a.wrapping_add((b - a).wrapping_mul(balance as i32) >> 16) as i16
}

/// `Crossfade1022(const int16_t* table_a, const int16_t* table_b, uint32_t phase, uint16_t
/// balance)`.
#[inline]
pub fn crossfade1022(table_a: &[i16], table_b: &[i16], phase: u32, balance: u16) -> i16 {
    let a = interpolate1022(table_a, phase) as i32;
    let b = interpolate1022(table_b, phase) as i32;
    a.wrapping_add((b - a).wrapping_mul(balance as i32) >> 16) as i16
}

/// `Crossfade115(const int16_t* table_a, const int16_t* table_b, uint16_t phase, uint16_t
/// balance)`.
#[inline]
pub fn crossfade115(table_a: &[i16], table_b: &[i16], phase: u16, balance: u16) -> i16 {
    let a = interpolate115(table_a, phase) as i32;
    let b = interpolate115(table_b, phase) as i32;
    a.wrapping_add((b - a).wrapping_mul(balance as i32) >> 16) as i16
}
