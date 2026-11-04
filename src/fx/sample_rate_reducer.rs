//! Ported from plaits/dsp/fx/sample_rate_reducer.h: sample rate reducer.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).

use crate::stmlib::dsp::polyblep::{next_blep_sample, this_blep_sample};
use crate::stmlib::stmlib::constrain;

#[derive(Default)]
pub struct SampleRateReducer {
    phase_: f32,
    sample_: f32,
    previous_sample_: f32,
    next_sample_: f32,
}

impl SampleRateReducer {
    pub fn init(&mut self) {
        self.phase_ = 0.0;
        self.sample_ = 0.0;
        self.next_sample_ = 0.0;
        self.previous_sample_ = 0.0;
    }

    pub fn process<const OPTIMIZED_HANDLING_OF_SPECIAL_CASES: bool>(
        &mut self,
        mut frequency: f32,
        in_out: &mut [f32],
        size: usize,
    ) {
        if OPTIMIZED_HANDLING_OF_SPECIAL_CASES {
            // Use fast specialized implementations for target rates close to the
            // original rates. Caveats:
            // - The size argument must be a multiple of 4.
            // - There will be a transition glitch between the "optimized" and the
            //   "common case" code, so don't use this when frequency is modulated!
            // - The optimized code is not a truly variable reclocking, instead,
            //   this is a crossfade between reclocking at SR / 2N and SR / N.
            if frequency >= 1.0 {
                return;
            } else if frequency >= 0.5 {
                self.process_half(2.0 - 2.0 * frequency, in_out, size);
                return;
            } else if frequency >= 0.25 {
                self.process_quarter(2.0 - 4.0 * frequency, in_out, size);
                return;
            }
        } else {
            constrain(&mut frequency, 0.0, 1.0);
        }
        let mut previous_sample = self.previous_sample_;
        let mut next_sample = self.next_sample_;
        let mut sample = self.sample_;
        let mut phase = self.phase_;
        for x in &mut in_out[..size] {
            let mut this_sample = next_sample;
            next_sample = 0.0;
            phase += frequency;
            if phase >= 1.0 {
                phase -= 1.0;
                let t = phase / frequency;
                // t = 0: the transition occurred right at this sample.
                // t = 1: the transition occurred at the previous sample.
                // Use linear interpolation to recover the fractional sample.
                let new_sample = previous_sample + (*x - previous_sample) * (1.0 - t);
                let discontinuity = new_sample - sample;
                this_sample += discontinuity * this_blep_sample(t);
                next_sample += discontinuity * next_blep_sample(t);
                sample = new_sample;
            }
            next_sample += sample;
            previous_sample = *x;
            *x = this_sample;
        }
        self.phase_ = phase;
        self.next_sample_ = next_sample;
        self.sample_ = sample;
        self.previous_sample_ = previous_sample;
    }

    /// `size` must be a multiple of 2, as the C++ asserts in a comment.
    fn process_half(&mut self, amount: f32, in_out: &mut [f32], size: usize) {
        for pair in in_out[..size].chunks_exact_mut(2) {
            pair[1] += (pair[0] - pair[1]) * amount;
        }
        if let Some(&last) = in_out[..size].last() {
            self.sample_ = last;
            self.next_sample_ = last;
            self.previous_sample_ = last;
        }
    }

    /// `size` must be a multiple of 4, as the C++ asserts in a comment.
    fn process_quarter(&mut self, amount: f32, in_out: &mut [f32], size: usize) {
        for quad in in_out[..size].chunks_exact_mut(4) {
            quad[1] = quad[0];
            quad[2] += (quad[0] - quad[2]) * amount;
            quad[3] = quad[2];
        }
        if let Some(&last) = in_out[..size].last() {
            self.sample_ = last;
            self.next_sample_ = last;
            self.previous_sample_ = last;
        }
    }
}
