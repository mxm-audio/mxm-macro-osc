//! Ported from plaits/dsp/speech/lpc_speech_synth.{h,cc}: LPC10 speech synth.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The two C++ overloads of `PlayFrame` are [`LPCSpeechSynth::play_frame`] (a frame array and a
//! position in it) and the private `play_frame_blend` (two frames and a blend). The frame
//! array is a slice: Plaits' phonemes, or the word bank's frames viewed in the arena. `Render`
//! draws one random sample per sample, from the voice's [`Random`].

use bytemuck::{Pod, Zeroable};

use crate::cpp::at;
use crate::resources::LUT_LPC_EXCITATION_PULSE;
use crate::stmlib::dsp::dsp::{cast_i32, make_integral_fractional};
use crate::stmlib::dsp::polyblep::{next_blep_sample, this_blep_sample};
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::random::Random;

pub const K_LPC_ORDER: usize = 10;

pub const K_LPC_SPEECH_SYNTH_DEFAULT_F0: f32 = 100.0;

/// `LUT_LPC_EXCITATION_PULSE_SIZE` of `plaits/resources.h`, 640.
const LUT_LPC_EXCITATION_PULSE_SIZE: i32 = LUT_LPC_EXCITATION_PULSE.len() as i32;

/// `LPCSpeechSynth::Frame`, 14 bytes, laid out as on x64 (the word bank keeps 1 024 of them in
/// the arena).
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct Frame {
    // 14 bytes.
    pub energy: u8,
    pub period: u8,
    pub k0: i16,
    pub k1: i16,
    pub k2: i8,
    pub k3: i8,
    pub k4: i8,
    pub k5: i8,
    pub k6: i8,
    pub k7: i8,
    pub k8: i8,
    pub k9: i8,
}

const _: () = assert!(size_of::<Frame>() == 14);

#[derive(Default)]
pub struct LPCSpeechSynth {
    phase_: f32,
    frequency_: f32,
    noise_energy_: f32,
    pulse_energy_: f32,

    next_sample_: f32,
    excitation_pulse_sample_index_: i32,

    k_: [f32; K_LPC_ORDER],
    s_: [f32; K_LPC_ORDER + 1],
}

/// `BlendCoefficient<scale>(a, b, blend)`.
#[inline]
fn blend_coefficient<const SCALE: i32>(a: f32, b: f32, blend: f32) -> f32 {
    let a_f = a / SCALE as f32;
    let b_f = b / SCALE as f32;
    a_f + (b_f - a_f) * blend
}

impl LPCSpeechSynth {
    pub fn init(&mut self) {
        self.phase_ = 0.0;
        self.frequency_ = 0.0125;
        self.noise_energy_ = 0.0;
        self.pulse_energy_ = 0.0;

        self.next_sample_ = 0.0;
        self.excitation_pulse_sample_index_ = 0;

        self.k_.fill(0.0);
        self.s_.fill(0.0);
    }

    pub fn render(
        &mut self,
        prosody_amount: f32,
        pitch_shift: f32,
        excitation: &mut [f32],
        output: &mut [f32],
        size: usize,
        random: &mut Random,
    ) {
        let base_f0 = K_LPC_SPEECH_SYNTH_DEFAULT_F0 / 8000.0;
        let d = self.frequency_ - base_f0;
        let mut f = (base_f0 + d * prosody_amount) * pitch_shift;
        constrain(&mut f, 0.0, 0.5);

        let mut next_sample = self.next_sample_;
        for (excitation, output) in excitation[..size].iter_mut().zip(&mut output[..size]) {
            self.phase_ += f;

            let mut this_sample = next_sample;
            next_sample = 0.0;

            if self.phase_ >= 1.0 {
                self.phase_ -= 1.0;
                let reset_time = self.phase_ / f;
                let reset_sample = cast_i32(32.0 * reset_time);

                let mut discontinuity = 0.0f32;
                if self.excitation_pulse_sample_index_ < LUT_LPC_EXCITATION_PULSE_SIZE {
                    self.excitation_pulse_sample_index_ = self
                        .excitation_pulse_sample_index_
                        .wrapping_sub(reset_sample);
                    let s = at(
                        &LUT_LPC_EXCITATION_PULSE,
                        self.excitation_pulse_sample_index_,
                    );
                    discontinuity = s as f32 / 128.0 * self.pulse_energy_;
                }

                this_sample += -discontinuity * this_blep_sample(reset_time);
                next_sample += -discontinuity * next_blep_sample(reset_time);

                self.excitation_pulse_sample_index_ = reset_sample;
            }

            let mut e = [0.0f32; 11];
            e[10] = if random.get_sample() > 0 {
                self.noise_energy_
            } else {
                -self.noise_energy_
            };
            if self.excitation_pulse_sample_index_ < LUT_LPC_EXCITATION_PULSE_SIZE {
                let s = at(
                    &LUT_LPC_EXCITATION_PULSE,
                    self.excitation_pulse_sample_index_,
                );
                next_sample += s as f32 / 128.0 * self.pulse_energy_;
                self.excitation_pulse_sample_index_ =
                    self.excitation_pulse_sample_index_.wrapping_add(32);
            }
            e[10] += this_sample;
            e[10] *= 1.5;

            let k = &self.k_;
            let s = &mut self.s_;
            e[9] = e[10] - k[9] * s[9];
            e[8] = e[9] - k[8] * s[8];
            e[7] = e[8] - k[7] * s[7];
            e[6] = e[7] - k[6] * s[6];
            e[5] = e[6] - k[5] * s[5];
            e[4] = e[5] - k[4] * s[4];
            e[3] = e[4] - k[3] * s[3];
            e[2] = e[3] - k[2] * s[2];
            e[1] = e[2] - k[1] * s[1];
            e[0] = e[1] - k[0] * s[0];

            constrain(&mut e[0], -2.0, 2.0);

            s[9] = s[8] + k[8] * e[8];
            s[8] = s[7] + k[7] * e[7];
            s[7] = s[6] + k[6] * e[6];
            s[6] = s[5] + k[5] * e[5];
            s[5] = s[4] + k[4] * e[4];
            s[4] = s[3] + k[3] * e[3];
            s[3] = s[2] + k[2] * e[2];
            s[2] = s[1] + k[1] * e[1];
            s[1] = s[0] + k[0] * e[0];
            s[0] = e[0];

            *excitation = e[10];
            *output = e[0];
        }
        self.next_sample_ = next_sample;
    }

    /// `PlayFrame(const Frame* frames, float frame, bool interpolate)`: `frames[frame_integral]`
    /// and the frame after it. Past the end of `frames` the C++ reads what follows the array; the
    /// port reads a zeroed frame. Only the triggered consonant 14, the last of the phonemes, has
    /// a frame after it out of the array, and it plays with a blend of 0, where the second frame
    /// changes nothing.
    pub fn play_frame(&mut self, frames: &[Frame], frame: f32, interpolate: bool) {
        let (frame_integral, mut frame_fractional) = make_integral_fractional(frame);

        if !interpolate {
            frame_fractional = 0.0;
        }
        self.play_frame_blend(
            &at(frames, frame_integral),
            &at(frames, frame_integral.wrapping_add(1)),
            frame_fractional,
        );
    }

    /// `PlayFrame(const Frame& f1, const Frame& f2, float blend)`.
    fn play_frame_blend(&mut self, f1: &Frame, f2: &Frame, blend: f32) {
        let frequency_1 = if f1.period == 0 {
            self.frequency_
        } else {
            1.0 / f1.period as f32
        };
        let frequency_2 = if f2.period == 0 {
            self.frequency_
        } else {
            1.0 / f2.period as f32
        };
        self.frequency_ = frequency_1 + (frequency_2 - frequency_1) * blend;

        let energy_1 = f1.energy as f32 / 256.0;
        let energy_2 = f2.energy as f32 / 256.0;
        let noise_energy_1 = if f1.period == 0 { energy_1 } else { 0.0 };
        let noise_energy_2 = if f2.period == 0 { energy_2 } else { 0.0 };
        self.noise_energy_ = noise_energy_1 + (noise_energy_2 - noise_energy_1) * blend;

        let pulse_energy_1 = if f1.period != 0 { energy_1 } else { 0.0 };
        let pulse_energy_2 = if f2.period != 0 { energy_2 } else { 0.0 };
        self.pulse_energy_ = pulse_energy_1 + (pulse_energy_2 - pulse_energy_1) * blend;

        self.k_[0] = blend_coefficient::<32768>(f1.k0 as f32, f2.k0 as f32, blend);
        self.k_[1] = blend_coefficient::<32768>(f1.k1 as f32, f2.k1 as f32, blend);
        self.k_[2] = blend_coefficient::<128>(f1.k2 as f32, f2.k2 as f32, blend);
        self.k_[3] = blend_coefficient::<128>(f1.k3 as f32, f2.k3 as f32, blend);
        self.k_[4] = blend_coefficient::<128>(f1.k4 as f32, f2.k4 as f32, blend);
        self.k_[5] = blend_coefficient::<128>(f1.k5 as f32, f2.k5 as f32, blend);
        self.k_[6] = blend_coefficient::<128>(f1.k6 as f32, f2.k6 as f32, blend);
        self.k_[7] = blend_coefficient::<128>(f1.k7 as f32, f2.k7 as f32, blend);
        self.k_[8] = blend_coefficient::<128>(f1.k8 as f32, f2.k8 as f32, blend);
        self.k_[9] = blend_coefficient::<128>(f1.k9 as f32, f2.k9 as f32, blend);
    }
}
