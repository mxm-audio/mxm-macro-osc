//! Ported from plaits/dsp/oscillator/super_square_oscillator.h: two hard-sync'ed square waves
//! with a meta-parameter, also faking PWM. Based on VariableShapeOscillator, with hard-coded
//! pulse width (0.5), waveshape (only square), and sync enabled by default.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).

use crate::oscillator::oscillator::K_MAX_FREQUENCY;
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::polyblep::{next_blep_sample, this_blep_sample};

/// Zeroed by `Default`, as the static voice's members start: the chiptune engine never calls
/// `Init` on its fifth one.
#[derive(Default)]
pub struct SuperSquareOscillator {
    // Oscillator state.
    master_phase_: f32,
    slave_phase_: f32,
    next_sample_: f32,
    high_: bool,

    // For interpolation of parameters.
    master_frequency_: f32,
    slave_frequency_: f32,
}

impl SuperSquareOscillator {
    pub fn init(&mut self) {
        self.master_phase_ = 0.0;
        self.slave_phase_ = 0.0;
        self.next_sample_ = 0.0;
        self.high_ = false;

        self.master_frequency_ = 0.0;
        self.slave_frequency_ = 0.01;
    }

    pub fn render(&mut self, mut frequency: f32, shape: f32, out: &mut [f32], size: usize) {
        let mut master_frequency = frequency;
        frequency *= if shape < 0.5 {
            0.51 + 0.98 * shape
        } else {
            1.0 + 16.0 * (shape - 0.5) * (shape - 0.5)
        };

        if master_frequency >= K_MAX_FREQUENCY {
            master_frequency = K_MAX_FREQUENCY;
        }

        if frequency >= K_MAX_FREQUENCY {
            frequency = K_MAX_FREQUENCY;
        }

        let mut master_fm =
            ParameterInterpolator::new(&mut self.master_frequency_, master_frequency, size);
        let mut fm = ParameterInterpolator::new(&mut self.slave_frequency_, frequency, size);

        let mut next_sample = self.next_sample_;

        for out_sample in &mut out[..size] {
            let mut reset = false;
            let mut transition_during_reset = false;
            let mut reset_time = 0.0f32;

            let mut this_sample = next_sample;
            next_sample = 0.0;

            let master_frequency = master_fm.next();
            let slave_frequency = fm.next();

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
                if !self.high_ && slave_phase_at_reset >= 0.5 {
                    transition_during_reset = true;
                }
                let value = if slave_phase_at_reset < 0.5 { 0.0 } else { 1.0 };
                this_sample -= value * this_blep_sample(reset_time);
                next_sample -= value * next_blep_sample(reset_time);
            }

            self.slave_phase_ += slave_frequency;
            // `while (transition_during_reset || !reset)`: neither changes in the loop, which
            // only `break` leaves.
            if transition_during_reset || !reset {
                loop {
                    if !self.high_ {
                        if self.slave_phase_ < 0.5 {
                            break;
                        }
                        let t = (self.slave_phase_ - 0.5) / slave_frequency;
                        this_sample += this_blep_sample(t);
                        next_sample += next_blep_sample(t);
                        self.high_ = true;
                    }

                    if self.high_ {
                        if self.slave_phase_ < 1.0 {
                            break;
                        }
                        self.slave_phase_ -= 1.0;
                        let t = self.slave_phase_ / slave_frequency;
                        this_sample -= this_blep_sample(t);
                        next_sample -= next_blep_sample(t);
                        self.high_ = false;
                    }
                }
            }

            if reset {
                self.slave_phase_ = reset_time * slave_frequency;
                self.high_ = false;
            }

            next_sample += if self.slave_phase_ < 0.5 { 0.0 } else { 1.0 };
            *out_sample = 2.0 * this_sample - 1.0;
        }

        self.next_sample_ = next_sample;
    }
}
