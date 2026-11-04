//! Ported from plaits/dsp/fx/overdrive.h: distortion/overdrive.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).

use crate::stmlib::dsp::dsp::soft_clip;
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;

#[derive(Default)]
pub struct Overdrive {
    pre_gain_: f32,
    post_gain_: f32,
}

impl Overdrive {
    pub fn init(&mut self) {
        self.pre_gain_ = 0.0;
        self.post_gain_ = 0.0;
    }

    pub fn process(&mut self, drive: f32, in_out: &mut [f32], size: usize) {
        let drive_2 = drive * drive;
        let pre_gain_a = drive * 0.5;
        let pre_gain_b = drive_2 * drive_2 * drive * 24.0;
        let pre_gain = pre_gain_a + (pre_gain_b - pre_gain_a) * drive_2;
        let drive_squashed = drive * (2.0 - drive);
        let post_gain = 1.0 / soft_clip(0.33 + drive_squashed * (pre_gain - 0.33));

        let mut pre_gain_modulation =
            ParameterInterpolator::new(&mut self.pre_gain_, pre_gain, size);

        let mut post_gain_modulation =
            ParameterInterpolator::new(&mut self.post_gain_, post_gain, size);

        for sample in &mut in_out[..size] {
            let pre = pre_gain_modulation.next() * *sample;
            *sample = soft_clip(pre) * post_gain_modulation.next();
        }
    }
}
