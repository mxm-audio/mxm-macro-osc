//! Ported from plaits/dsp/envelope.h: envelope for the internal LPG.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).

#[derive(Default)]
pub struct LPGEnvelope {
    vactrol_state_: f32,
    gain_: f32,
    frequency_: f32,
    hf_bleed_: f32,
    ramp_up_: bool,
}

impl LPGEnvelope {
    #[inline]
    pub fn init(&mut self) {
        self.vactrol_state_ = 0.0;
        self.gain_ = 1.0;
        self.frequency_ = 0.5;
        self.hf_bleed_ = 0.0;
        self.ramp_up_ = false;
    }

    #[inline]
    pub fn trigger(&mut self) {
        self.ramp_up_ = true;
    }

    #[inline]
    pub fn process_ping(&mut self, attack: f32, short_decay: f32, decay_tail: f32, hf: f32) {
        if self.ramp_up_ {
            self.vactrol_state_ += attack;
            if self.vactrol_state_ >= 1.0 {
                self.vactrol_state_ = 1.0;
                self.ramp_up_ = false;
            }
        }
        self.process_lp(
            if self.ramp_up_ {
                self.vactrol_state_
            } else {
                0.0
            },
            short_decay,
            decay_tail,
            hf,
        );
    }

    #[inline]
    pub fn process_lp(&mut self, level: f32, short_decay: f32, decay_tail: f32, hf: f32) {
        let vactrol_input = level;
        let vactrol_error = vactrol_input - self.vactrol_state_;
        let vactrol_state_2 = self.vactrol_state_ * self.vactrol_state_;
        let vactrol_state_4 = vactrol_state_2 * vactrol_state_2;
        let tail = 1.0 - self.vactrol_state_;
        let tail_2 = tail * tail;
        let vactrol_coefficient = if vactrol_error > 0.0 {
            0.6
        } else {
            short_decay + (1.0 - vactrol_state_4) * decay_tail
        };
        self.vactrol_state_ += vactrol_coefficient * vactrol_error;

        self.gain_ = self.vactrol_state_;
        self.frequency_ = 0.003 + 0.3 * vactrol_state_4 + hf * 0.04;
        self.hf_bleed_ = (tail_2 + (1.0 - tail_2) * hf) * hf * hf;
    }

    #[inline]
    pub fn gain(&self) -> f32 {
        self.gain_
    }

    #[inline]
    pub fn frequency(&self) -> f32 {
        self.frequency_
    }

    #[inline]
    pub fn hf_bleed(&self) -> f32 {
        self.hf_bleed_
    }
}

#[derive(Default)]
pub struct DecayEnvelope {
    value_: f32,
}

impl DecayEnvelope {
    #[inline]
    pub fn init(&mut self) {
        self.value_ = 0.0;
    }

    #[inline]
    pub fn trigger(&mut self) {
        self.value_ = 1.0;
    }

    #[inline]
    pub fn process(&mut self, decay: f32) {
        self.value_ *= 1.0 - decay;
    }

    #[inline]
    pub fn value(&self) -> f32 {
        self.value_
    }
}
