//! Plaits' voice and its twenty-four engines, ported to Rust from Emilie Gillet's MIT code
//! (`pichenettes/eurorack` `plaits/dsp/` and the parts of `pichenettes/stmlib` it uses), held
//! bit for bit to that C++: see this crate's `AGENTS.md`.
//!
//! One module per C++ file, at the same path: `voice` is `plaits/dsp/voice.{h,cc}`,
//! `engine::virtual_analog_engine` is `plaits/dsp/engine/virtual_analog_engine.{h,cc}`,
//! `stmlib::dsp::units` is `stmlib/dsp/units.{h,cc}`. `cpp` holds what the port takes from C++
//! itself (`std::max`, `std::min`, an array read).
//!
//! The interface is the voice's: [`Patch`], [`Modulations`], [`Frame`], [`Voice::new`] and
//! [`Voice::render`], once per block of [`dsp::K_BLOCK_SIZE`] frames. The README's example:
//!
//! ```
//! use mxm_macro_osc::dsp::K_BLOCK_SIZE;
//! use mxm_macro_osc::{Frame, Modulations, Patch, Voice};
//!
//! let mut voice = Voice::new(); // its 16 KB engine memory, allocated once
//! let patch = Patch {
//!     engine: 8, // 0 to 23, as the module numbers them
//!     note: 48.0,
//!     harmonics: 0.5,
//!     timbre: 0.5,
//!     morph: 0.5,
//!     decay: 0.5,
//!     lpg_colour: 0.5,
//!     ..Patch::default()
//! };
//! let modulations = Modulations::default(); // nothing patched: the voice drones
//! let mut frames = [Frame::default(); K_BLOCK_SIZE];
//! voice.render(&patch, &modulations, &mut frames); // 12 frames of OUT and AUX at 48 kHz
//! ```

#![forbid(unsafe_code)]
// Lints whose advice would have the port read unlike the C++ it is held to.
#![allow(
    // The C++'s float literals, digit for digit; Rust rounds each to f32 as MSVC does.
    clippy::excessive_precision,
    // `CONSTRAIN` and `Clip16` compare as the C++ does; `clamp` treats a NaN differently.
    clippy::manual_clamp,
    // One module per C++ file: `oscillator/oscillator.h` is `oscillator::oscillator`.
    clippy::module_inception,
    // `Next()` is `next`, a method of the C++ class, not an iterator.
    clippy::should_implement_trait
)]

pub mod cpp;
pub mod stmlib;

pub mod chords;
pub mod downsampler;
pub mod drums;
pub mod dsp;
pub mod engine;
pub mod engine2;
pub mod envelope;
pub mod fm;
pub mod fx;
pub mod noise;
pub mod oscillator;
pub mod physical_modelling;
pub mod resources;
pub mod speech;
pub mod voice;

pub use voice::{Frame, Modulations, Patch, Voice};
