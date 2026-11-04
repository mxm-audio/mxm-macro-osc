//! Ported from plaits/dsp/oscillator/string_synth_oscillator.h: a mixture of 7 sawtooth and
//! square waveforms in the style of divide-down organs:
//!
//! ```text
//! [0]     [1]        [2]     [3]        [4]     [5]        [6]
//! Saw 8', Square 8', Saw 4', Square 4', Saw 2', Square 2', Saw 1'
//! ```
//!
//! Internally, this renders 4 band-limited sawtooths, from 8' to 1' from a single phase counter.
//! The square waveforms are obtained by algebraic manipulations on the sawtooths, using the
//! identity: Square 16' = 2 Sawtooth 16' - Sawtooth 8'
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).

use crate::stmlib::dsp::dsp::cast_i32;
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::polyblep::{next_blep_sample, this_blep_sample};

#[derive(Default)]
pub struct StringSynthOscillator {
    // Oscillator state.
    phase_: f32,
    next_sample_: f32,
    segment_: i32,

    // For interpolation of parameters.
    frequency_: f32,
    saw_8_gain_: f32,
    saw_4_gain_: f32,
    saw_2_gain_: f32,
    saw_1_gain_: f32,
}

impl StringSynthOscillator {
    #[inline]
    pub fn init(&mut self) {
        self.phase_ = 0.0;
        self.next_sample_ = 0.0;
        self.segment_ = 0;

        self.frequency_ = 0.001;
        self.saw_8_gain_ = 0.0;
        self.saw_4_gain_ = 0.0;
        self.saw_2_gain_ = 0.0;
        self.saw_1_gain_ = 0.0;
    }

    /// `Render(frequency, unshifted_registration, gain, out, size)`: adds to `out`;
    /// `unshifted_registration` holds at least 7 amplitudes.
    #[inline]
    pub fn render(
        &mut self,
        mut frequency: f32,
        unshifted_registration: &[f32],
        gain: f32,
        out: &mut [f32],
        size: usize,
    ) {
        frequency *= 8.0;

        // Deal with very high frequencies by shifting everything 1 or 2 octave
        // down: Instead of playing the 1st harmonic of a 8kHz wave, we play the
        // second harmonic of a 4kHz wave.
        let mut shift: usize = 0;
        while frequency > 0.5 {
            shift += 2;
            frequency *= 0.5;
        }
        // Frequency is just too high.
        if shift >= 8 {
            return;
        }

        let mut registration = [0.0f32; 7];
        registration[..shift].fill(0.0);
        registration[shift..].copy_from_slice(&unshifted_registration[..7 - shift]);

        let mut fm = ParameterInterpolator::new(&mut self.frequency_, frequency, size);
        let mut saw_8_gain_modulation = ParameterInterpolator::new(
            &mut self.saw_8_gain_,
            (registration[0] + 2.0 * registration[1]) * gain,
            size,
        );
        let mut saw_4_gain_modulation = ParameterInterpolator::new(
            &mut self.saw_4_gain_,
            (registration[2] - registration[1] + 2.0 * registration[3]) * gain,
            size,
        );
        let mut saw_2_gain_modulation = ParameterInterpolator::new(
            &mut self.saw_2_gain_,
            (registration[4] - registration[3] + 2.0 * registration[5]) * gain,
            size,
        );
        let mut saw_1_gain_modulation = ParameterInterpolator::new(
            &mut self.saw_1_gain_,
            (registration[6] - registration[5]) * gain,
            size,
        );

        let mut phase = self.phase_;
        let mut next_sample = self.next_sample_;
        let mut segment = self.segment_;
        for out in &mut out[..size] {
            let mut this_sample = next_sample;
            next_sample = 0.0;

            let frequency = fm.next();
            let saw_8_gain = saw_8_gain_modulation.next();
            let saw_4_gain = saw_4_gain_modulation.next();
            let saw_2_gain = saw_2_gain_modulation.next();
            let saw_1_gain = saw_1_gain_modulation.next();

            phase += frequency;
            let mut next_segment = cast_i32(phase);
            if next_segment != segment {
                let mut discontinuity = 0.0f32;
                if next_segment == 8 {
                    phase -= 8.0;
                    next_segment -= 8;
                    discontinuity -= saw_8_gain;
                }
                if (next_segment & 3) == 0 {
                    discontinuity -= saw_4_gain;
                }
                if (next_segment & 1) == 0 {
                    discontinuity -= saw_2_gain;
                }
                discontinuity -= saw_1_gain;
                if discontinuity != 0.0 {
                    let fraction = phase - next_segment as f32;
                    let t = fraction / frequency;
                    this_sample += this_blep_sample(t) * discontinuity;
                    next_sample += next_blep_sample(t) * discontinuity;
                }
            }
            segment = next_segment;

            next_sample += (phase - 4.0) * saw_8_gain * 0.125;
            next_sample += (phase - (segment & 4) as f32 - 2.0) * saw_4_gain * 0.25;
            next_sample += (phase - (segment & 6) as f32 - 1.0) * saw_2_gain * 0.5;
            next_sample += (phase - (segment & 7) as f32 - 0.5) * saw_1_gain;
            *out += 2.0 * this_sample;
        }
        self.next_sample_ = next_sample;
        self.phase_ = phase;
        self.segment_ = segment;
    }
}
