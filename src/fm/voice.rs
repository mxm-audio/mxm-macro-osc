//! Ported from plaits/dsp/fm/voice.h: DX7 voice.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `Voice<num_operators>` for the six operators Plaits instantiates (see `algorithms`). The C++
//! keeps two pointers: to the engine's `Algorithms`, which `render` takes as a parameter, and to
//! the patch, one of the six-op engine's 32 in the arena, which is `patch_`, its index among
//! them; `render` takes those patches. Of the three `Render` overloads the six-op engine calls
//! `Render(parameters, temp, size)`, which is `render`; the one they all call,
//! `Render(parameters, buffers[4], size)`, is `render_buffers`, the four buffers being offsets
//! into one slice. `Render(parameters, temp, out, aux, size)`, which nothing calls, is not
//! ported.
//!
//! When enabled, the amplitude modulation LFO linearly modulates the amplitude
//! of an operator. Otherwise, a more complex formula involving an exponential
//! is used, to match Dexed's output. (`FAST_OP_LEVEL_MODULATION`, which is not defined.)

use super::algorithms::{Algorithms, NUM_OPERATORS};
use super::dx_units::{
    amp_mod_sensitivity, frequency_ratio, keyboard_scaling, normalize_velocity, operator_level,
    pow2_fast, rate_scaling,
};
use super::envelope::{OperatorEnvelope, PitchEnvelope};
use super::operator::Operator;
use super::patch::Patch;
use crate::cpp::min;
use crate::stmlib::dsp::units::semitones_to_ratio_safe;

#[derive(Clone, Copy, Default, Debug)]
pub struct Parameters {
    pub sustain: bool,
    pub gate: bool,
    pub note: f32,
    pub velocity: f32,
    pub brightness: f32,
    pub envelope_control: f32,
    pub pitch_mod: f32,
    pub amp_mod: f32,
}

#[derive(Default)]
pub struct Voice {
    sample_rate_: f32,
    one_hz_: f32,
    a0_: f32,

    gate_: bool,

    operator_: [Operator; NUM_OPERATORS],
    operator_envelope_: [OperatorEnvelope; NUM_OPERATORS],
    pitch_envelope_: PitchEnvelope,

    normalized_velocity_: f32,
    note_: f32,

    ratios_: [f32; NUM_OPERATORS],
    level_headroom_: [f32; NUM_OPERATORS],
    level_: [f32; NUM_OPERATORS],

    feedback_state_: [f32; 2],

    /// The patch, as its index among the engine's patches; `None` for `NULL`.
    patch_: Option<usize>,

    dirty_: bool,
}

impl Voice {
    /// `Init(algorithms, sample_rate)`: the algorithms are `render`'s parameter.
    #[inline]
    pub fn init(&mut self, sample_rate: f32) {
        self.sample_rate_ = sample_rate;
        self.one_hz_ = 1.0 / sample_rate;
        self.a0_ = 55.0 / sample_rate;

        let native_sr = 44100.0f32; // Legacy sample rate.
        let envelope_scale = native_sr * self.one_hz_;

        for i in 0..NUM_OPERATORS {
            self.operator_[i].reset();
            self.operator_envelope_[i].init(envelope_scale);
        }
        self.pitch_envelope_.init(envelope_scale);

        self.feedback_state_ = [0.0, 0.0];

        self.patch_ = None;
        self.gate_ = false;
        self.note_ = 48.0;
        self.normalized_velocity_ = 10.0;

        self.dirty_ = true;
    }

    #[inline]
    pub fn set_patch(&mut self, patch: Option<usize>) {
        self.patch_ = patch;
        self.dirty_ = true;
    }

    /// Pre-compute everything that can be pre-computed once a patch is loaded:
    /// - envelope constants
    /// - frequency ratios
    #[inline]
    pub fn setup(&mut self, patch: &Patch) -> bool {
        if !self.dirty_ {
            return false;
        }

        self.pitch_envelope_
            .set(&patch.pitch_envelope.rate, &patch.pitch_envelope.level);
        for i in 0..NUM_OPERATORS {
            let op = &patch.op[i];

            let level = operator_level(i32::from(op.level));
            // `level` passed as the `uint8_t global_level`.
            self.operator_envelope_[i].set(&op.envelope.rate, &op.envelope.level, level as u8);

            // The level increase caused by keyboard scaling plus velocity
            // scaling should not exceed this number - otherwise it would be
            // equivalent to have an operator with a level above 99.
            self.level_headroom_[i] = (127 - level) as f32;

            // Pre-compute frequency ratios. Encode the base frequency
            // (1Hz or the root note) as the sign of the ratio.
            let sign = if op.mode == 0 { 1.0 } else { -1.0 };
            self.ratios_[i] = sign * frequency_ratio(op);
        }
        self.dirty_ = false;
        true
    }

    #[inline]
    pub fn op_level(&self, i: usize) -> f32 {
        self.level_[i]
    }

    /// `Render(parameters, temp, size)`: the buffers are `temp`, `temp + size` and twice
    /// `temp + 2 * size`.
    #[inline]
    pub fn render(
        &mut self,
        parameters: &Parameters,
        algorithms: &Algorithms,
        patches: &[Patch],
        temp: &mut [f32],
        size: usize,
    ) {
        let buffers = [0, size, 2 * size, 2 * size];
        self.render_buffers(parameters, algorithms, patches, temp, &buffers, size);
    }

    /// `Render(parameters, buffers, size)`: `buffers` are offsets into `buffer`.
    #[inline]
    pub fn render_buffers(
        &mut self,
        parameters: &Parameters,
        algorithms: &Algorithms,
        patches: &[Patch],
        buffer: &mut [f32],
        buffers: &[usize; 4],
        size: usize,
    ) {
        // The C++ dereferences `patch_`, which the six-op engine has always set by now.
        let Some(patch) = self.patch_.and_then(|index| patches.get(index)) else {
            return;
        };

        if self.setup(patch) {
            // This prevents a CPU overrun, since there is not enough CPU to perform
            // both a patch setup and a full render in the time alloted for
            // a render. As a drawback, this causes a 0.5ms blank before a new
            // patch starts playing. But this is a clean blank, as opposed to a
            // glitchy overrun.
            return;
        }

        let envelope_rate = size as f32;
        let ad_scale = pow2_fast::<1>((0.5 - parameters.envelope_control) * 8.0);
        let r_scale = pow2_fast::<1>(-(parameters.envelope_control - 0.3).abs() * 8.0);
        let gate_duration = 1.5 * self.sample_rate_;
        let envelope_sample = gate_duration * parameters.envelope_control;

        // Apply LFO and pitch envelope modulations.
        let pitch_envelope = if parameters.sustain {
            self.pitch_envelope_
                .render_at_sample(envelope_sample, gate_duration)
        } else {
            self.pitch_envelope_
                .render(parameters.gate, envelope_rate, ad_scale, r_scale)
        };
        let pitch_mod = pitch_envelope + parameters.pitch_mod;
        let f0 =
            self.a0_ * 0.25 * semitones_to_ratio_safe(parameters.note - 9.0 + pitch_mod * 12.0);

        // Sample the note and velocity (used for scaling) only when a trigger
        // is received, or constantly when we are in free-running mode.
        let note_on = parameters.gate && !self.gate_;
        self.gate_ = parameters.gate;
        if note_on || parameters.sustain {
            self.normalized_velocity_ = normalize_velocity(parameters.velocity);
            self.note_ = parameters.note;
        }

        // Reset operator phase if a note on is detected & if the patch requires it.
        if note_on && patch.reset_phase != 0 {
            for i in 0..NUM_OPERATORS {
                self.operator_[i].phase = 0;
            }
        }

        // Compute frequencies and amplitudes.
        let mut f = [0.0f32; NUM_OPERATORS];
        let mut a = [0.0f32; NUM_OPERATORS];
        let algorithm = patch.algorithm as usize;
        for i in 0..NUM_OPERATORS {
            let op = &patch.op[i];

            f[i] = self.ratios_[i]
                * (if self.ratios_[i] < 0.0 {
                    -self.one_hz_
                } else {
                    f0
                });

            let rate_scaling = rate_scaling(self.note_, i32::from(op.rate_scaling));
            let mut level = if parameters.sustain {
                self.operator_envelope_[i].render_at_sample(envelope_sample, gate_duration)
            } else {
                self.operator_envelope_[i].render(
                    parameters.gate,
                    envelope_rate * rate_scaling,
                    ad_scale,
                    r_scale,
                )
            };
            let kb_scaling = keyboard_scaling(self.note_, &op.keyboard_scaling);
            let velocity_scaling = self.normalized_velocity_ * op.velocity_sensitivity as f32;
            let brightness = if algorithms.is_modulator(algorithm, i) {
                (parameters.brightness - 0.5) * 32.0
            } else {
                0.0
            };

            level += 0.125
                * min(
                    kb_scaling + velocity_scaling + brightness,
                    self.level_headroom_[i],
                );

            self.level_[i] = level;

            let sensitivity = amp_mod_sensitivity(i32::from(op.amp_mod_sensitivity));
            let log_level_mod = sensitivity * parameters.amp_mod - 1.0;
            let level_mod = 1.0 - pow2_fast::<2>(6.4 * log_level_mod);
            a[i] = pow2_fast::<2>(-14.0 + level * level_mod);
        }

        let mut i = 0usize;
        while i < NUM_OPERATORS {
            let call = algorithms.render_call(algorithm, i);
            // Every operator a chain starts at has its renderer (`Compile`); the C++ would call
            // `NULL` otherwise.
            let Some(render_fn) = call.render_fn else {
                break;
            };
            render_fn(
                &mut self.operator_[i..],
                &f[i..],
                &a[i..],
                &mut self.feedback_state_,
                i32::from(patch.feedback),
                buffer,
                buffers[call.input_index as usize],
                buffers[call.output_index as usize],
                size,
            );
            i += call.n as usize;
        }
    }
}
