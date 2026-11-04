//! Ported from plaits/dsp/fx/fx_engine.h: base class for building reverbs.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! How the C++'s templates become Rust:
//!
//! - `Format` (`FORMAT_12_BIT`, …) and `DataType<format>` are marker types implementing
//!   [`DataType`]: [`Format12Bit`], [`Format16Bit`], [`Format32Bit`].
//! - The delay memory, `Reserve<l1, Reserve<l2, …>>`, is a list of lengths, and
//!   `DelayLine<Memory, index>` the [`DelayLine`] that [`delay_line`] computes from it at compile
//!   time, with the same base and length (each line's base is the sum of the lengths before it,
//!   plus one each). The C++'s `STATIC_ASSERT(D::base + D::length <= size)` is a `const`
//!   assertion where the lines are declared.
//! - `TAIL` (`, -1`) is the offset [`TAIL`].
//! - The buffer is a region of the arena: the engine keeps its offset (`init`), and `clear` and
//!   `start` are handed the region. `Start(&c)` sets every field of the context, so `start`
//!   returns a new one, borrowing the region.
//! - The overloads of `Context::Read`, `Write`, `WriteAllPass` and `Interpolate` are named for
//!   their arguments: `read_value` and `read_value_scaled` (a float), `read` and `read_offset`
//!   (a delay line), and so on.

use std::marker::PhantomData;

use bytemuck::Pod;

use crate::stmlib::dsp::cosine_oscillator::{COSINE_OSCILLATOR_APPROXIMATE, CosineOscillator};
use crate::stmlib::dsp::dsp::{cast_i32, clip16, make_integral_fractional};

/// `TAIL`: read or write a delay line at its far end.
pub const TAIL: i32 = -1;

pub type LFOIndex = usize;
pub const LFO_1: LFOIndex = 0;
pub const LFO_2: LFOIndex = 1;

/// `DataType<format>`: how a sample is stored in the delay memory.
pub trait DataType {
    type T: Pod + Default;
    fn decompress(value: Self::T) -> f32;
    fn compress(value: f32) -> Self::T;
}

/// `FORMAT_12_BIT`.
pub struct Format12Bit;
/// `FORMAT_16_BIT`.
pub struct Format16Bit;
/// `FORMAT_32_BIT`.
pub struct Format32Bit;

impl DataType for Format12Bit {
    type T = u16;

    #[inline]
    fn decompress(value: u16) -> f32 {
        (value as i16) as f32 / 4096.0
    }

    #[inline]
    fn compress(value: f32) -> u16 {
        clip16(cast_i32(value * 4096.0)) as u16
    }
}

impl DataType for Format16Bit {
    type T = u16;

    #[inline]
    fn decompress(value: u16) -> f32 {
        (value as i16) as f32 / 32768.0
    }

    #[inline]
    fn compress(value: f32) -> u16 {
        clip16(cast_i32(value * 32768.0)) as u16
    }
}

impl DataType for Format32Bit {
    type T = f32;

    #[inline]
    fn decompress(value: f32) -> f32 {
        value
    }

    #[inline]
    fn compress(value: f32) -> f32 {
        value
    }
}

/// `DelayLine<Memory, index>`: where a line starts in the delay memory, and its length.
#[derive(Clone, Copy, Debug)]
pub struct DelayLine {
    pub base: i32,
    pub length: i32,
}

/// `DelayLine<Memory, index>` of the memory `Reserve<lengths[0], Reserve<lengths[1], …>>`.
pub const fn delay_line(lengths: &[i32], index: usize) -> DelayLine {
    let mut base = 0;
    let mut i = 0;
    while i < index {
        base += lengths[i] + 1;
        i += 1;
    }
    DelayLine {
        base,
        length: lengths[index],
    }
}

/// `STATIC_ASSERT(D::base + D::length <= size, delay_memory_full)`.
pub const fn fits(d: DelayLine, size: usize) -> bool {
    (d.base + d.length) as usize <= size
}

pub struct FxEngine<const SIZE: usize, F: DataType> {
    write_ptr_: i32,
    /// Arena offset of the `SIZE` samples of delay memory.
    buffer_: usize,
    lfo_: [CosineOscillator; 2],
    format_: PhantomData<F>,
}

impl<const SIZE: usize, F: DataType> Default for FxEngine<SIZE, F> {
    fn default() -> Self {
        Self {
            write_ptr_: 0,
            buffer_: 0,
            lfo_: Default::default(),
            format_: PhantomData,
        }
    }
}

impl<const SIZE: usize, F: DataType> FxEngine<SIZE, F> {
    /// `Init(T* buffer)`: the arena offset of the delay memory.
    pub fn init(&mut self, buffer: usize) {
        self.buffer_ = buffer;
    }

    /// The arena offset of the delay memory, whose `SIZE` samples `clear` and `start` take.
    #[inline]
    pub fn buffer(&self) -> usize {
        self.buffer_
    }

    pub fn clear(&mut self, buffer: &mut [F::T]) {
        buffer[..SIZE].fill(F::T::default());
        self.write_ptr_ = 0;
    }

    #[inline]
    pub fn set_lfo_frequency(&mut self, index: LFOIndex, frequency: f32) {
        self.lfo_[index].init::<COSINE_OSCILLATOR_APPROXIMATE>(frequency * 32.0);
    }

    /// `Start(Context* c)`.
    #[inline]
    pub fn start<'a>(&mut self, buffer: &'a mut [F::T]) -> Context<'a, SIZE, F> {
        self.write_ptr_ -= 1;
        if self.write_ptr_ < 0 {
            self.write_ptr_ += SIZE as i32;
        }
        let lfo_value_ = if (self.write_ptr_ & 31) == 0 {
            let lfo_0 = self.lfo_[0].next();
            let lfo_1 = self.lfo_[1].next();
            [lfo_0, lfo_1]
        } else {
            [self.lfo_[0].value(), self.lfo_[1].value()]
        };
        Context {
            accumulator_: 0.0,
            previous_read_: 0.0,
            lfo_value_,
            buffer_: buffer,
            write_ptr_: self.write_ptr_,
            format_: PhantomData,
        }
    }
}

pub struct Context<'a, const SIZE: usize, F: DataType> {
    accumulator_: f32,
    previous_read_: f32,
    lfo_value_: [f32; 2],
    buffer_: &'a mut [F::T],
    write_ptr_: i32,
    format_: PhantomData<F>,
}

impl<const SIZE: usize, F: DataType> Context<'_, SIZE, F> {
    /// The C++'s `MASK = size - 1`: `SIZE` is a power of two.
    const MASK: i32 = SIZE as i32 - 1;

    /// The buffer index `(write_ptr_ + offset) & MASK`, in `int32_t`.
    #[inline]
    fn index(&self, offset: i32) -> usize {
        (self.write_ptr_.wrapping_add(offset) & Self::MASK) as usize
    }

    #[inline]
    pub fn load(&mut self, value: f32) {
        self.accumulator_ = value;
    }

    /// `Read(float value, float scale)`.
    #[inline]
    pub fn read_value_scaled(&mut self, value: f32, scale: f32) {
        self.accumulator_ += value * scale;
    }

    /// `Read(float value)`.
    #[inline]
    pub fn read_value(&mut self, value: f32) {
        self.accumulator_ += value;
    }

    /// `Write(float& value)`.
    #[inline]
    pub fn write_value(&mut self, value: &mut f32) {
        *value = self.accumulator_;
    }

    /// `Write(float& value, float scale)`.
    #[inline]
    pub fn write_value_scaled(&mut self, value: &mut f32, scale: f32) {
        *value = self.accumulator_;
        self.accumulator_ *= scale;
    }

    /// `Write(D& d, int32_t offset, float scale)`.
    #[inline]
    pub fn write_offset(&mut self, d: DelayLine, offset: i32, scale: f32) {
        let w = F::compress(self.accumulator_);
        let index = if offset == -1 {
            self.index(d.base + d.length - 1)
        } else {
            self.index(d.base.wrapping_add(offset))
        };
        self.buffer_[index] = w;
        self.accumulator_ *= scale;
    }

    /// `Write(D& d, float scale)`.
    #[inline]
    pub fn write(&mut self, d: DelayLine, scale: f32) {
        self.write_offset(d, 0, scale);
    }

    /// `WriteAllPass(D& d, int32_t offset, float scale)`.
    #[inline]
    pub fn write_all_pass_offset(&mut self, d: DelayLine, offset: i32, scale: f32) {
        self.write_offset(d, offset, scale);
        self.accumulator_ += self.previous_read_;
    }

    /// `WriteAllPass(D& d, float scale)`.
    #[inline]
    pub fn write_all_pass(&mut self, d: DelayLine, scale: f32) {
        self.write_all_pass_offset(d, 0, scale);
    }

    /// `Read(D& d, int32_t offset, float scale)`.
    #[inline]
    pub fn read_offset(&mut self, d: DelayLine, offset: i32, scale: f32) {
        let index = if offset == -1 {
            self.index(d.base + d.length - 1)
        } else {
            self.index(d.base.wrapping_add(offset))
        };
        let r = self.buffer_[index];
        let r_f = F::decompress(r);
        self.previous_read_ = r_f;
        self.accumulator_ += r_f * scale;
    }

    /// `Read(D& d, float scale)`.
    #[inline]
    pub fn read(&mut self, d: DelayLine, scale: f32) {
        self.read_offset(d, 0, scale);
    }

    #[inline]
    pub fn lp(&mut self, state: &mut f32, coefficient: f32) {
        *state += coefficient * (self.accumulator_ - *state);
        self.accumulator_ = *state;
    }

    #[inline]
    pub fn hp(&mut self, state: &mut f32, coefficient: f32) {
        *state += coefficient * (self.accumulator_ - *state);
        self.accumulator_ -= *state;
    }

    /// `Interpolate(D& d, float offset, float scale)`.
    #[inline]
    pub fn interpolate(&mut self, d: DelayLine, offset: f32, scale: f32) {
        let (offset_integral, offset_fractional) = make_integral_fractional(offset);
        let a = F::decompress(self.buffer_[self.index(offset_integral.wrapping_add(d.base))]);
        let b = F::decompress(
            self.buffer_[self.index(offset_integral.wrapping_add(d.base).wrapping_add(1))],
        );
        let x = a + (b - a) * offset_fractional;
        self.previous_read_ = x;
        self.accumulator_ += x * scale;
    }

    /// `Interpolate(D& d, float offset, LFOIndex index, float amplitude, float scale)`.
    #[inline]
    pub fn interpolate_lfo(
        &mut self,
        d: DelayLine,
        mut offset: f32,
        index: LFOIndex,
        amplitude: f32,
        scale: f32,
    ) {
        offset += amplitude * self.lfo_value_[index];
        self.interpolate(d, offset, scale);
    }
}
