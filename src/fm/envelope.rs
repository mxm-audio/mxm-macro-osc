//! Ported from plaits/dsp/fm/envelope.h: multi-segment envelope generator.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The classic constant-time design (as found in many other MI products) might
//! cause differences in behavior from the original DX-series envelopes, in
//! particular when jumping to the last segment before having reached the sustain
//! phase.
//!
//! The unusual RenderAtSample() method allows the evaluation of the envelope at
//! an arbitrary point in time, used in Plaits' "envelope scrubbing" feature.
//!
//! A couple of quirks from the DX-series' operator envelopes are implemented,
//! namely:
//! - vaguely logarithmic shape for ascending segments.
//! - direct jump above a threshold for ascending segments.
//! - specific logic and rates for plateaus.
//!
//! `Envelope<num_stages, reshape_ascending_segments>` is generic over both. The two subclasses
//! add only a `Set`, so they are the template's two instantiations with that `set` of their own;
//! the base class's `Set(increment, level)`, which hides behind theirs in the C++, is
//! `set_increments_and_levels`. The overloads `Init()` and `Render(gate)` are `init_default`
//! and `render_gate`, and `value(stage, phase, start_level)` is `value_at`.

use super::dx_units::{
    operator_envelope_increment, operator_level, pitch_envelope_increment, pitch_envelope_level,
};
use crate::cpp::max;

pub struct Envelope<const NUM_STAGES: usize, const RESHAPE_ASCENDING_SEGMENTS: bool> {
    stage_: i32,
    phase_: f32,
    start_: f32,

    increment_: [f32; NUM_STAGES],
    level_: [f32; NUM_STAGES],
    scale_: f32,
}

impl<const NUM_STAGES: usize, const RESHAPE_ASCENDING_SEGMENTS: bool> Default
    for Envelope<NUM_STAGES, RESHAPE_ASCENDING_SEGMENTS>
{
    fn default() -> Self {
        Self {
            stage_: 0,
            phase_: 0.0,
            start_: 0.0,
            increment_: [0.0; NUM_STAGES],
            level_: [0.0; NUM_STAGES],
            scale_: 0.0,
        }
    }
}

impl<const NUM_STAGES: usize, const RESHAPE_ASCENDING_SEGMENTS: bool>
    Envelope<NUM_STAGES, RESHAPE_ASCENDING_SEGMENTS>
{
    // enum
    pub const NUM_STAGES: usize = NUM_STAGES;
    pub const PREVIOUS_LEVEL: i32 = -100;

    /// `PREVIOUS_LEVEL` where the C++ compares it with, or stores it in, a float.
    const PREVIOUS_LEVEL_F: f32 = Self::PREVIOUS_LEVEL as f32;

    /// The last stage, `num_stages - 1`.
    const LAST: i32 = NUM_STAGES as i32 - 1;

    #[inline]
    pub fn init_default(&mut self) {
        self.init(1.0);
    }

    #[inline]
    pub fn init(&mut self, scale: f32) {
        self.scale_ = scale;
        self.stage_ = Self::LAST;
        self.phase_ = 1.0;
        self.start_ = 0.0;
        for i in 0..NUM_STAGES {
            self.increment_[i] = 0.001;
            self.level_[i] = 1.0 / (1i32 << i) as f32;
        }
        self.level_[NUM_STAGES - 1] = 0.0;
    }

    /// Directly copy the variables.
    pub fn set_increments_and_levels(
        &mut self,
        increment: &[f32; NUM_STAGES],
        level: &[f32; NUM_STAGES],
    ) {
        self.increment_.copy_from_slice(increment);
        self.level_.copy_from_slice(level);
    }

    #[inline]
    pub fn render_at_sample(&self, mut t: f32, gate_duration: f32) -> f32 {
        if t > gate_duration {
            // Check how far we are into the release phase.
            let phase = (t - gate_duration) * self.increment_[NUM_STAGES - 1];
            return if phase >= 1.0 {
                self.level_[NUM_STAGES - 1]
            } else {
                self.value_at(
                    Self::LAST,
                    phase,
                    self.render_at_sample(gate_duration, gate_duration),
                )
            };
        }

        let mut stage = 0i32;
        while stage < Self::LAST {
            let stage_duration = 1.0 / self.increment_[stage as usize];
            if t < stage_duration {
                break;
            }
            t -= stage_duration;
            stage += 1;
        }

        if stage == Self::LAST {
            t -= gate_duration;
            if t <= 0.0 {
                // TODO(pichenettes): this should always be true.
                return self.level_[NUM_STAGES - 2];
            } else if t * self.increment_[NUM_STAGES - 1] > 1.0 {
                return self.level_[NUM_STAGES - 1];
            }
        }
        self.value_at(
            stage,
            t * self.increment_[stage as usize],
            Self::PREVIOUS_LEVEL_F,
        )
    }

    #[inline]
    pub fn render_gate(&mut self, gate: bool) -> f32 {
        self.render(gate, 1.0, 1.0, 1.0)
    }

    #[inline]
    pub fn render(&mut self, gate: bool, rate: f32, ad_scale: f32, release_scale: f32) -> f32 {
        if gate {
            if self.stage_ == Self::LAST {
                self.start_ = self.value();
                self.stage_ = 0;
                self.phase_ = 0.0;
            }
        } else if self.stage_ != Self::LAST {
            self.start_ = self.value();
            self.stage_ = Self::LAST;
            self.phase_ = 0.0;
        }
        self.phase_ += self.increment_[self.stage_ as usize]
            * rate
            * (if self.stage_ == Self::LAST {
                release_scale
            } else {
                ad_scale
            });
        if self.phase_ >= 1.0 {
            if self.stage_ >= Self::LAST - 1 {
                self.phase_ = 1.0;
            } else {
                self.phase_ = 0.0;
                self.stage_ += 1;
            }
            self.start_ = Self::PREVIOUS_LEVEL_F;
        }

        self.value()
    }

    #[inline]
    fn value(&self) -> f32 {
        self.value_at(self.stage_, self.phase_, self.start_)
    }

    #[inline]
    fn value_at(&self, stage: i32, mut phase: f32, start_level: f32) -> f32 {
        let num_stages = NUM_STAGES as i32;
        let mut from = if start_level == Self::PREVIOUS_LEVEL_F {
            self.level_[((stage - 1 + num_stages) % num_stages) as usize]
        } else {
            start_level
        };
        let mut to = self.level_[stage as usize];

        if RESHAPE_ASCENDING_SEGMENTS && from < to {
            from = max(6.7, from);
            to = max(6.7, to);
            phase *= (2.5 - phase) * 0.666667;
        }

        phase * (to - from) + from
    }
}

pub type OperatorEnvelope = Envelope<4, true>;

impl OperatorEnvelope {
    pub fn set(&mut self, rate: &[u8; 4], level: &[u8; 4], global_level: u8) {
        // Configure levels.
        for (i, &stage_level) in level.iter().enumerate() {
            let mut level_scaled = operator_level(i32::from(stage_level));
            level_scaled = (level_scaled & !1) + i32::from(global_level) - 133; // 125 ?
            self.level_[i] = 0.125
                * (if level_scaled < 1 {
                    0.5
                } else {
                    level_scaled as f32
                });
        }

        // Configure increments.
        for i in 0..Self::NUM_STAGES {
            let mut increment = operator_envelope_increment(i32::from(rate[i]));
            let mut from = self.level_[(i + Self::NUM_STAGES - 1) % Self::NUM_STAGES];
            let mut to = self.level_[i];

            if from == to {
                // Quirk: for plateaux, the increment is scaled.
                increment *= 0.6;
                if i == 0 && level[i] == 0 {
                    // Quirk: the attack plateau is faster.
                    increment *= 20.0;
                }
            } else if from < to {
                from = max(6.7, from);
                to = max(6.7, to);
                if from == to {
                    // Quirk: because of the jump, the attack might disappear.
                    increment = 1.0;
                } else {
                    // Quirk: because of the weird shape, the rate is adjusted.
                    increment *= 7.2 / (to - from);
                }
            } else {
                increment *= 1.0 / (from - to);
            }
            self.increment_[i] = increment * self.scale_;
        }
    }
}

pub type PitchEnvelope = Envelope<4, false>;

impl PitchEnvelope {
    pub fn set(&mut self, rate: &[u8; 4], level: &[u8; 4]) {
        // Configure levels.
        for (level_, &stage_level) in self.level_.iter_mut().zip(level) {
            *level_ = pitch_envelope_level(i32::from(stage_level));
        }

        // Configure increments.
        for (i, &stage_rate) in rate.iter().enumerate() {
            let from = self.level_[(i + Self::NUM_STAGES - 1) % Self::NUM_STAGES];
            let to = self.level_[i];
            let mut increment = pitch_envelope_increment(i32::from(stage_rate));
            if from != to {
                increment *= 1.0 / (from - to).abs();
            } else if i != Self::NUM_STAGES - 1 {
                increment = 0.2;
            }
            self.increment_[i] = increment * self.scale_;
        }
    }
}
