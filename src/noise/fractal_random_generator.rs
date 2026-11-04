//! Ported from plaits/dsp/noise/fractal_random_generator.h: stack octaves of a noise source to
//! generate a fractal noise.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The template's `typename T` is any [`FractalSource`]: what the C++ calls on it, `Init()` and
//! `Render(frequency)`, the latter handed the voice's `Random`. No engine of the voice uses it
//! (as in the C++).

use crate::stmlib::utils::random::Random;

/// What `FractalRandomGenerator<T, order>` calls on its `T`.
pub trait FractalSource {
    fn init(&mut self);
    fn render(&mut self, frequency: f32, random: &mut Random) -> f32;
}

pub struct FractalRandomGenerator<T: FractalSource, const ORDER: usize> {
    generator_: [T; ORDER],
}

impl<T: FractalSource + Default, const ORDER: usize> Default for FractalRandomGenerator<T, ORDER> {
    fn default() -> Self {
        Self {
            generator_: std::array::from_fn(|_| T::default()),
        }
    }
}

impl<T: FractalSource, const ORDER: usize> FractalRandomGenerator<T, ORDER> {
    pub fn init(&mut self) {
        for generator in &mut self.generator_ {
            generator.init();
        }
    }

    /// `Render(frequency)`: the decay at 0.5.
    pub fn render(&mut self, frequency: f32, random: &mut Random) -> f32 {
        self.render_decay(frequency, 0.5, random)
    }

    /// `Render(frequency, decay)`.
    pub fn render_decay(&mut self, mut frequency: f32, decay: f32, random: &mut Random) -> f32 {
        let mut gain = 0.5f32;
        let mut sum = 0.0f32;

        for generator in &mut self.generator_ {
            sum += generator.render(frequency, random) * gain;
            gain *= decay;
            frequency *= 2.0;
        }

        sum
    }
}
