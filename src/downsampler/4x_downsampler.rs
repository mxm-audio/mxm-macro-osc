//! Ported from plaits/dsp/downsampler/4x_downsampler.h: FIR downsampler.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The C++ keeps a pointer to its state and writes `head_` back in its destructor; here it
//! borrows the state and writes it back when dropped, at the end of its scope.

use crate::resources::LUT_4X_DOWNSAMPLER_FIR;

pub const K_OVERSAMPLING: usize = 4;

pub struct Downsampler<'a> {
    head_: f32,
    tail_: f32,
    state_: &'a mut f32,
}

impl<'a> Downsampler<'a> {
    pub fn new(state: &'a mut f32) -> Self {
        Self {
            head_: *state,
            tail_: 0.0,
            state_: state,
        }
    }

    #[inline]
    pub fn accumulate(&mut self, i: i32, sample: f32) {
        self.head_ += sample * LUT_4X_DOWNSAMPLER_FIR[(3 - (i & 3)) as usize];
        self.tail_ += sample * LUT_4X_DOWNSAMPLER_FIR[(i & 3) as usize];
    }

    #[inline]
    pub fn read(&mut self) -> f32 {
        let value = self.head_;
        self.head_ = self.tail_;
        self.tail_ = 0.0;
        value
    }
}

impl Drop for Downsampler<'_> {
    /// `~Downsampler() { *state_ = head_; }`.
    fn drop(&mut self) {
        *self.state_ = self.head_;
    }
}
