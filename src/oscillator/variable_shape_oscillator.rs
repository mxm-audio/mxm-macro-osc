//! Ported from plaits/dsp/oscillator/variable_shape_oscillator.h: continuously variable
//! waveform: triangle > saw > square. Both square and triangle have variable slope /
//! pulse-width. Additionally, the phase resets can be locked to a master frequency.
//!
//! A template parameter allows the generation of a master + slave signal,
//! which can be used as the phase for phase distortion or modulation synthesis.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The C++ `Render` overloads are `render` (free-running) and `render_sync` (locked to a
//! master), and the template `render_with::<ENABLE_SYNC, OUTPUT_PHASE>`. `ComputeNaiveSample`
//! reads no member, so it is an associated function: the parameter interpolators borrow the
//! oscillator's fields while it runs.

use crate::cpp::max;
use crate::oscillator::oscillator::K_MAX_FREQUENCY;
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::polyblep::{
    next_blep_sample, next_integrated_blep_sample, this_blep_sample, this_integrated_blep_sample,
};
use crate::stmlib::stmlib::constrain;

#[derive(Default)]
pub struct VariableShapeOscillator {
    // Oscillator state.
    master_phase_: f32,
    slave_phase_: f32,
    next_sample_: f32,
    previous_pw_: f32,
    high_: bool,

    // For interpolation of parameters.
    master_frequency_: f32,
    slave_frequency_: f32,
    pw_: f32,
    waveshape_: f32,
    phase_modulation_: f32,
}

impl VariableShapeOscillator {
    pub fn init(&mut self) {
        self.master_phase_ = 0.0;
        self.slave_phase_ = 0.0;
        self.next_sample_ = 0.0;
        self.previous_pw_ = 0.5;
        self.high_ = false;

        self.master_frequency_ = 0.0;
        self.slave_frequency_ = 0.01;
        self.pw_ = 0.5;
        self.waveshape_ = 0.0;
        self.phase_modulation_ = 0.0;
    }

    pub fn render(
        &mut self,
        frequency: f32,
        pw: f32,
        waveshape: f32,
        out: &mut [f32],
        size: usize,
    ) {
        self.render_with::<false, false>(0.0, frequency, pw, waveshape, 0.0, out, size);
    }

    pub fn render_sync(
        &mut self,
        master_frequency: f32,
        frequency: f32,
        pw: f32,
        waveshape: f32,
        out: &mut [f32],
        size: usize,
    ) {
        self.render_with::<true, false>(master_frequency, frequency, pw, waveshape, 0.0, out, size);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_with<const ENABLE_SYNC: bool, const OUTPUT_PHASE: bool>(
        &mut self,
        mut master_frequency: f32,
        mut frequency: f32,
        mut pw: f32,
        waveshape: f32,
        phase_modulation_amount: f32,
        out: &mut [f32],
        size: usize,
    ) {
        if master_frequency >= K_MAX_FREQUENCY {
            master_frequency = K_MAX_FREQUENCY;
        }
        if frequency >= K_MAX_FREQUENCY {
            frequency = K_MAX_FREQUENCY;
        }

        if frequency >= 0.25 {
            pw = 0.5;
        } else {
            constrain(&mut pw, frequency * 2.0, 1.0 - 2.0 * frequency);
        }

        let mut master_fm =
            ParameterInterpolator::new(&mut self.master_frequency_, master_frequency, size);
        let mut fm = ParameterInterpolator::new(&mut self.slave_frequency_, frequency, size);
        let mut pwm = ParameterInterpolator::new(&mut self.pw_, pw, size);
        let mut waveshape_modulation =
            ParameterInterpolator::new(&mut self.waveshape_, waveshape, size);
        let mut phase_modulation =
            ParameterInterpolator::new(&mut self.phase_modulation_, phase_modulation_amount, size);

        let mut next_sample = self.next_sample_;

        for out_sample in &mut out[..size] {
            let mut reset = false;
            let mut transition_during_reset = false;
            let mut reset_time = 0.0f32;

            let mut this_sample = next_sample;
            next_sample = 0.0;

            let master_frequency = master_fm.next();
            let slave_frequency = fm.next();
            let pw = pwm.next();
            let waveshape = waveshape_modulation.next();

            let square_amount = max(waveshape - 0.5, 0.0) * 2.0;
            let triangle_amount = max(1.0 - waveshape * 2.0, 0.0);

            let slope_up = 1.0 / pw;
            let slope_down = 1.0 / (1.0 - pw);

            if ENABLE_SYNC {
                self.master_phase_ += master_frequency;
                if self.master_phase_ >= 1.0 {
                    self.master_phase_ -= 1.0;
                    reset_time = self.master_phase_ / master_frequency;

                    let mut slave_phase_at_reset =
                        self.slave_phase_ + (1.0 - reset_time) * slave_frequency;
                    reset = true;
                    if slave_phase_at_reset >= 1.0 {
                        slave_phase_at_reset -= 1.0;
                        transition_during_reset = true;
                    }
                    if !self.high_ && slave_phase_at_reset >= pw {
                        transition_during_reset = true;
                    }
                    let value = Self::compute_naive_sample(
                        slave_phase_at_reset,
                        pw,
                        slope_up,
                        slope_down,
                        triangle_amount,
                        square_amount,
                    );
                    this_sample -= value * this_blep_sample(reset_time);
                    next_sample -= value * next_blep_sample(reset_time);
                }
            } else if OUTPUT_PHASE {
                self.master_phase_ += master_frequency;
                if self.master_phase_ >= 1.0 {
                    self.master_phase_ -= 1.0;
                }
            }

            self.slave_phase_ += slave_frequency;
            // `while (transition_during_reset || !reset)`: neither changes in the loop, which
            // only `break` leaves.
            if transition_during_reset || !reset {
                loop {
                    if !self.high_ {
                        if self.slave_phase_ < pw {
                            break;
                        }
                        let t =
                            (self.slave_phase_ - pw) / (self.previous_pw_ - pw + slave_frequency);
                        let mut triangle_step = (slope_up + slope_down) * slave_frequency;
                        triangle_step *= triangle_amount;

                        this_sample += square_amount * this_blep_sample(t);
                        next_sample += square_amount * next_blep_sample(t);
                        this_sample -= triangle_step * this_integrated_blep_sample(t);
                        next_sample -= triangle_step * next_integrated_blep_sample(t);
                        self.high_ = true;
                    }

                    if self.high_ {
                        if self.slave_phase_ < 1.0 {
                            break;
                        }
                        self.slave_phase_ -= 1.0;
                        let t = self.slave_phase_ / slave_frequency;
                        let mut triangle_step = (slope_up + slope_down) * slave_frequency;
                        triangle_step *= triangle_amount;

                        this_sample -= (1.0 - triangle_amount) * this_blep_sample(t);
                        next_sample -= (1.0 - triangle_amount) * next_blep_sample(t);
                        this_sample += triangle_step * this_integrated_blep_sample(t);
                        next_sample += triangle_step * next_integrated_blep_sample(t);
                        self.high_ = false;
                    }
                }
            }

            if ENABLE_SYNC && reset {
                self.slave_phase_ = reset_time * slave_frequency;
                self.high_ = false;
            }

            next_sample += Self::compute_naive_sample(
                self.slave_phase_,
                pw,
                slope_up,
                slope_down,
                triangle_amount,
                square_amount,
            );
            self.previous_pw_ = pw;

            if OUTPUT_PHASE {
                let mut phasor = self.master_phase_;
                if ENABLE_SYNC {
                    // A trick to prevent discontinuities when the phase wraps around.
                    let w = 4.0 * (1.0 - self.master_phase_) * self.master_phase_;
                    this_sample *= w * (2.0 - w);

                    // Apply some asymmetry on the main phasor too.
                    let p2 = phasor * phasor;
                    phasor += (p2 * p2 - phasor) * (pw - 0.5).abs() * 2.0;
                }
                *out_sample = phasor + phase_modulation.next() * this_sample;
            } else {
                *out_sample = 2.0 * this_sample - 1.0;
            }
        }

        self.next_sample_ = next_sample;
    }

    #[inline]
    pub fn set_master_phase(&mut self, master_phase: f32) {
        self.master_phase_ = master_phase;
    }

    #[inline]
    fn compute_naive_sample(
        phase: f32,
        pw: f32,
        slope_up: f32,
        slope_down: f32,
        triangle_amount: f32,
        square_amount: f32,
    ) -> f32 {
        let mut saw = phase;
        let square = if phase < pw { 0.0 } else { 1.0 };
        let triangle = if phase < pw {
            phase * slope_up
        } else {
            1.0 - (phase - pw) * slope_down
        };
        saw += (square - saw) * square_amount;
        saw += (triangle - saw) * triangle_amount;
        saw
    }
}
