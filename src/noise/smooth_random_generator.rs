//! Ported from plaits/dsp/noise/smooth_random_generator.h: smooth random generator for the
//! internal modulations.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! No engine of the voice uses it (as in the C++); it draws from the `Random` it is passed.

use super::fractal_random_generator::FractalSource;
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct SmoothRandomGenerator {
    phase_: f32,
    from_: f32,
    interval_: f32,
}

impl SmoothRandomGenerator {
    pub fn init(&mut self) {
        self.phase_ = 0.0;
        self.from_ = 0.0;
        self.interval_ = 0.0;
    }

    pub fn render(&mut self, frequency: f32, random: &mut Random) -> f32 {
        self.phase_ += frequency;
        if self.phase_ >= 1.0 {
            self.phase_ -= 1.0;
            self.from_ += self.interval_;
            self.interval_ = random.get_float() * 2.0 - 1.0 - self.from_;
        }
        let t = self.phase_ * self.phase_ * (3.0 - 2.0 * self.phase_);
        self.from_ + self.interval_ * t
    }
}

impl FractalSource for SmoothRandomGenerator {
    fn init(&mut self) {
        SmoothRandomGenerator::init(self);
    }

    fn render(&mut self, frequency: f32, random: &mut Random) -> f32 {
        SmoothRandomGenerator::render(self, frequency, random)
    }
}
