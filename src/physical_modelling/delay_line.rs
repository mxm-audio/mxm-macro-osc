//! Ported from plaits/dsp/physical_modelling/delay_line.h: delay line (same implementation as
//! from stmlib, but does not own its buffer).
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The C++ keeps a pointer to a buffer it does not own; here every method is handed the buffer,
//! `MAX_DELAY` samples, wherever it lives (a voice's member, or a region of the arena). The C++
//! template is over the sample type, but `Init` takes a `float*`, so only `float` compiles: the
//! port is over `f32`.
//!
//! The write pointer is a `size_t` and the integral part of a delay an `int32_t`, added as C++
//! adds them (the `int32_t` converted to `size_t`, modulo 2⁶⁴).

use crate::stmlib::dsp::dsp::make_integral_fractional;

#[derive(Default)]
pub struct DelayLine<const MAX_DELAY: usize> {
    write_ptr_: usize,
}

impl<const MAX_DELAY: usize> DelayLine<MAX_DELAY> {
    /// `Init(float* buffer)`.
    pub fn init(&mut self, line: &mut [f32]) {
        self.reset(line);
    }

    pub fn reset(&mut self, line: &mut [f32]) {
        line[..MAX_DELAY].fill(0.0);
        self.write_ptr_ = 0;
    }

    #[inline]
    pub fn write(&mut self, line: &mut [f32], sample: f32) {
        line[self.write_ptr_] = sample;
        self.write_ptr_ = self.write_ptr_.wrapping_sub(1).wrapping_add(MAX_DELAY) % MAX_DELAY;
    }

    #[inline]
    pub fn allpass(
        &mut self,
        line: &mut [f32],
        sample: f32,
        delay: usize,
        coefficient: f32,
    ) -> f32 {
        let read = line[self.write_ptr_.wrapping_add(delay) % MAX_DELAY];
        let write = sample + coefficient * read;
        self.write(line, write);
        -write * coefficient + read
    }

    #[inline]
    pub fn write_read(&mut self, line: &mut [f32], sample: f32, delay: f32) -> f32 {
        self.write(line, sample);
        self.read(line, delay)
    }

    #[inline]
    pub fn read(&self, line: &[f32], delay: f32) -> f32 {
        let (delay_integral, delay_fractional) = make_integral_fractional(delay);
        let base = self.write_ptr_.wrapping_add(delay_integral as usize);
        let a = line[base % MAX_DELAY];
        let b = line[base.wrapping_add(1) % MAX_DELAY];
        a + (b - a) * delay_fractional
    }

    #[inline]
    pub fn read_hermite(&self, line: &[f32], delay: f32) -> f32 {
        let (delay_integral, delay_fractional) = make_integral_fractional(delay);
        // `int32_t t = (write_ptr_ + delay_integral + max_delay);`: computed in size_t, then
        // narrowed to int32_t.
        let t = self
            .write_ptr_
            .wrapping_add(delay_integral as usize)
            .wrapping_add(MAX_DELAY) as i32;
        let xm1 = line[(t.wrapping_sub(1) as usize) % MAX_DELAY];
        let x0 = line[(t as usize) % MAX_DELAY];
        let x1 = line[(t.wrapping_add(1) as usize) % MAX_DELAY];
        let x2 = line[(t.wrapping_add(2) as usize) % MAX_DELAY];
        let c = (x1 - xm1) * 0.5;
        let v = x0 - x1;
        let w = c + v;
        let a = w + v + (x2 - x0) * 0.5;
        let b_neg = w + a;
        let f = delay_fractional;
        (((a * f) - b_neg) * f + c) * f + x0
    }
}
