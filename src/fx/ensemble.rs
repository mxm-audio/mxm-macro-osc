//! Ported from plaits/dsp/fx/ensemble.h: ensemble FX.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! Its delay memory is 1 024 floats of the arena: `init` takes the offset the engine allocated,
//! and `reset` and `process` the arena. `amount_` and `depth_` start at zero, as the C++'s do
//! in its static voice, until they are set.

use super::fx_engine::{DelayLine, Format32Bit, FxEngine, delay_line, fits};
use crate::oscillator::sine_oscillator::sine_raw;
use crate::stmlib::utils::buffer_allocator::Arena;

pub type E = FxEngine<1024, Format32Bit>;

/// `E::Reserve<511, E::Reserve<511> >`.
const MEMORY: [i32; 2] = [511, 511];
const LINE_L: DelayLine = delay_line(&MEMORY, 0);
const LINE_R: DelayLine = delay_line(&MEMORY, 1);
const _: () = assert!(fits(LINE_R, 1024), "delay_memory_full");

#[derive(Default)]
pub struct Ensemble {
    engine_: E,

    amount_: f32,
    depth_: f32,

    phase_1_: u32,
    phase_2_: u32,
}

impl Ensemble {
    /// `Init(E::T* buffer)`: the arena offset of 1 024 floats.
    pub fn init(&mut self, buffer: usize) {
        self.engine_.init(buffer);
        self.phase_1_ = 0;
        self.phase_2_ = 0;
    }

    pub fn reset(&mut self, arena: &mut Arena) {
        let buffer = arena.view_mut::<f32>(self.engine_.buffer(), 1024);
        self.engine_.clear(buffer);
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32], size: usize, arena: &mut Arena) {
        let buffer = arena.view_mut::<f32>(self.engine_.buffer(), 1024);

        for i in 0..size {
            let mut c = self.engine_.start(buffer);
            let dry_amount = 1.0 - self.amount_ * 0.5;

            // Update LFO.
            let one_third: u32 = 1417339207;
            let two_third: u32 = 2834678415;

            self.phase_1_ = self.phase_1_.wrapping_add(67289); // 0.75 Hz
            self.phase_2_ = self.phase_2_.wrapping_add(589980); // 6.57 Hz
            let slow_0 = sine_raw(self.phase_1_);
            let slow_120 = sine_raw(self.phase_1_.wrapping_add(one_third));
            let slow_240 = sine_raw(self.phase_1_.wrapping_add(two_third));
            let fast_0 = sine_raw(self.phase_2_);
            let fast_120 = sine_raw(self.phase_2_.wrapping_add(one_third));
            let fast_240 = sine_raw(self.phase_2_.wrapping_add(two_third));

            // Max deviation: 176
            let a = self.depth_ * 160.0;
            let b = self.depth_ * 16.0;

            let mod_1 = slow_0 * a + fast_0 * b;
            let mod_2 = slow_120 * a + fast_120 * b;
            let mod_3 = slow_240 * a + fast_240 * b;

            let mut wet = 0.0f32;

            // Sum L & R channel to send to chorus line.
            c.read_value_scaled(left[i], 1.0);
            c.write(LINE_L, 0.0);
            c.read_value_scaled(right[i], 1.0);
            c.write(LINE_R, 0.0);

            c.interpolate(LINE_L, mod_1 + 192.0, 0.33);
            c.interpolate(LINE_L, mod_2 + 192.0, 0.33);
            c.interpolate(LINE_R, mod_3 + 192.0, 0.33);
            c.write_value_scaled(&mut wet, 0.0);
            left[i] = wet * self.amount_ + left[i] * dry_amount;

            c.interpolate(LINE_R, mod_1 + 192.0, 0.33);
            c.interpolate(LINE_R, mod_2 + 192.0, 0.33);
            c.interpolate(LINE_L, mod_3 + 192.0, 0.33);
            c.write_value_scaled(&mut wet, 0.0);
            right[i] = wet * self.amount_ + right[i] * dry_amount;
        }
    }

    #[inline]
    pub fn set_amount(&mut self, amount: f32) {
        self.amount_ = amount;
    }

    #[inline]
    pub fn set_depth(&mut self, depth: f32) {
        self.depth_ = depth;
    }
}
