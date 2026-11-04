//! Ported from plaits/dsp/engine2/arpeggiator.h: arpeggiator.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `Clock` draws from stmlib's `Random` in random mode, so it takes the voice's.

use crate::stmlib::utils::random::Random;

pub type ArpeggiatorMode = i32;
pub const ARPEGGIATOR_MODE_UP: ArpeggiatorMode = 0;
pub const ARPEGGIATOR_MODE_DOWN: ArpeggiatorMode = 1;
pub const ARPEGGIATOR_MODE_UP_DOWN: ArpeggiatorMode = 2;
pub const ARPEGGIATOR_MODE_RANDOM: ArpeggiatorMode = 3;
pub const ARPEGGIATOR_MODE_LAST: ArpeggiatorMode = 4;

/// `range_` is not set by `Init`; zeroed by `Default`, as the static voice's members start.
#[derive(Default)]
pub struct Arpeggiator {
    mode_: ArpeggiatorMode,
    range_: i32,

    note_: i32,
    octave_: i32,
    direction_: i32,
}

impl Arpeggiator {
    pub fn init(&mut self) {
        self.mode_ = ARPEGGIATOR_MODE_UP;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.note_ = 0;
        self.octave_ = 0;
        self.direction_ = 1;
    }

    pub fn set_mode(&mut self, mode: ArpeggiatorMode) {
        self.mode_ = mode;
    }

    pub fn set_range(&mut self, range: i32) {
        self.range_ = range;
    }

    #[inline]
    pub fn note(&self) -> i32 {
        self.note_
    }

    #[inline]
    pub fn octave(&self) -> i32 {
        self.octave_
    }

    pub fn clock(&mut self, num_notes: i32, random: &mut Random) {
        if num_notes == 1 && self.range_ == 1 {
            self.octave_ = 0;
            self.note_ = 0;
            return;
        }

        if self.mode_ == ARPEGGIATOR_MODE_RANDOM {
            loop {
                let w = random.get_word();
                // `uint32_t % int`: the int converts to unsigned. Neither divisor is zero: the
                // engine sets the range to 1, 2 or 4 before each clock, and every chord counts
                // at least its root.
                let octave = ((w >> 4) % self.range_ as u32) as i32;
                let note = ((w >> 20) % num_notes as u32) as i32;
                if octave != self.octave_ || note != self.note_ {
                    self.octave_ = octave;
                    self.note_ = note;
                    break;
                }
            }
            return;
        }

        if self.mode_ == ARPEGGIATOR_MODE_UP {
            self.direction_ = 1;
        }

        if self.mode_ == ARPEGGIATOR_MODE_DOWN {
            self.direction_ = -1;
        }

        self.note_ += self.direction_;

        let mut done = false;
        while !done {
            done = true;
            if self.note_ >= num_notes || self.note_ < 0 {
                self.octave_ += self.direction_;
                self.note_ = if self.direction_ > 0 {
                    0
                } else {
                    num_notes - 1
                };
            }
            if self.octave_ >= self.range_ || self.octave_ < 0 {
                self.octave_ = if self.direction_ > 0 {
                    0
                } else {
                    self.range_ - 1
                };
                if self.mode_ == ARPEGGIATOR_MODE_UP_DOWN {
                    self.direction_ = -self.direction_;
                    self.note_ = if self.direction_ > 0 {
                        1
                    } else {
                        num_notes - 2
                    };
                    self.octave_ = if self.direction_ > 0 {
                        0
                    } else {
                        self.range_ - 1
                    };
                    done = false;
                }
            }
        }
    }
}
