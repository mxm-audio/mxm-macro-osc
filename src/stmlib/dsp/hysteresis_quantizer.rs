//! Ported from stmlib/dsp/hysteresis_quantizer.h: quantize a float in [0, 1] to an integer in
//! [0, num_steps[, with hysteresis to prevent jumps near the decision boundary.
//!
//! Copyright 2015 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The C++ overloads of `Process` are `process`, `process_hysteresis` and `process_base`.

use super::dsp::cast_i32;
use crate::stmlib::stmlib::constrain;

#[derive(Default)]
pub struct HysteresisQuantizer {
    quantized_value_: i32,
}

impl HysteresisQuantizer {
    pub fn init(&mut self) {
        self.quantized_value_ = 0;
    }

    #[inline]
    pub fn process(&mut self, value: f32, num_steps: i32) -> i32 {
        self.process_hysteresis(value, num_steps, 0.25)
    }

    #[inline]
    pub fn process_hysteresis(&mut self, value: f32, num_steps: i32, hysteresis: f32) -> i32 {
        self.process_base(0, value, num_steps, hysteresis)
    }

    #[inline]
    pub fn process_base(
        &mut self,
        base: i32,
        mut value: f32,
        num_steps: i32,
        hysteresis: f32,
    ) -> i32 {
        value *= num_steps.wrapping_sub(1) as f32;
        value += base as f32;
        let hysteresis_feedback = if value > self.quantized_value_ as f32 {
            -hysteresis
        } else {
            hysteresis
        };
        let mut q = cast_i32(value + hysteresis_feedback + 0.5);
        constrain(&mut q, 0, num_steps.wrapping_sub(1));
        self.quantized_value_ = q;
        q
    }

    #[inline]
    pub fn lookup<'t, T>(&mut self, array: &'t [T], value: f32, num_steps: i32) -> &'t T {
        &array[self.process(value, num_steps) as usize]
    }
}

#[derive(Default)]
pub struct HysteresisQuantizer2 {
    num_steps_: i32,
    hysteresis_: f32,

    scale_: f32,
    offset_: f32,

    quantized_value_: i32,
}

impl HysteresisQuantizer2 {
    pub fn init(&mut self, num_steps: i32, hysteresis: f32, symmetric: bool) {
        self.num_steps_ = num_steps;
        self.hysteresis_ = hysteresis;

        self.scale_ = (if symmetric {
            num_steps.wrapping_sub(1)
        } else {
            num_steps
        }) as f32;
        self.offset_ = if symmetric { 0.0 } else { -0.5 };

        self.quantized_value_ = 0;
    }

    #[inline]
    pub fn process(&mut self, value: f32) -> i32 {
        self.process_base(0, value)
    }

    #[inline]
    pub fn process_base(&mut self, base: i32, mut value: f32) -> i32 {
        value *= self.scale_;
        value += self.offset_;
        value += base as f32;

        let hysteresis_sign = if value > self.quantized_value_ as f32 {
            -1.0
        } else {
            1.0
        };
        let mut q = cast_i32(value + hysteresis_sign * self.hysteresis_ + 0.5);
        constrain(&mut q, 0, self.num_steps_.wrapping_sub(1));
        self.quantized_value_ = q;
        q
    }

    #[inline]
    pub fn lookup<'t, T>(&mut self, array: &'t [T], value: f32) -> &'t T {
        &array[self.process(value) as usize]
    }

    #[inline]
    pub fn num_steps(&self) -> i32 {
        self.num_steps_
    }

    #[inline]
    pub fn quantized_value(&self) -> i32 {
        self.quantized_value_
    }
}
