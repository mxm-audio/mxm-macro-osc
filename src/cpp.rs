//! What the port takes from C++ itself, with the C++'s exact semantics: `std::max`, `std::min`
//! (`<algorithm>`), and an array read whose index comes from a float.
//!
//! Not a ported file: these stand in for the language and its library where Rust's own
//! functions differ (`f32::max` and `f32::min` treat a NaN differently from `std::max` and
//! `std::min`, and indexing out of range panics).

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
/// rather than panic, since `render` must not panic on any input. No case reaches it.
#[inline]
pub fn at<T: Copy + Default>(table: &[T], index: i32) -> T {
    // A negative index becomes a huge usize, out of range like any other.
    table.get(index as usize).copied().unwrap_or_default()
}
