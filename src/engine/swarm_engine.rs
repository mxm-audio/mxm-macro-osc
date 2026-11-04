//! Ported from plaits/dsp/engine/swarm_engine.{h,cc}: swarm of sawtooths and sines.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The eight voices are in the arena, as in the C++ (`Allocate<SwarmVoice>(kNumSwarmVoices)`):
//! `SwarmVoice` and what it holds are `#[repr(C)]` and `Pod`, all floats, 64 bytes a voice as
//! on x64, so `init` keeps their offset and `reset` and `render` view them there. Each grain
//! draws two floats from the voice's `Random`, voice by voice, in the C++'s order.

use bytemuck::{Pod, Zeroable};

use super::engine::{
    Engine, EngineParameters, PostProcessingSettings, TRIGGER_RISING_EDGE, TRIGGER_UNPATCHED,
    note_to_frequency,
};
use crate::oscillator::oscillator::K_MAX_FREQUENCY;
use crate::oscillator::sine_oscillator::{FastSineOscillator, sine};
use crate::stmlib::dsp::dsp::{cast_i32, one_pole};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::polyblep::{next_blep_sample, this_blep_sample};
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

pub const K_NUM_SWARM_VOICES: i32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub struct GrainEnvelope {
    from_: f32,
    interval_: f32,
    phase_: f32,
    fm_: f32,
    amplitude_: f32,
    previous_size_ratio_: f32,
    filter_coefficient_: f32,
}

impl GrainEnvelope {
    pub fn init(&mut self) {
        self.from_ = 0.0;
        self.interval_ = 1.0;
        self.phase_ = 1.0;
        self.fm_ = 0.0;
        self.amplitude_ = 0.5;
        self.previous_size_ratio_ = 0.0;
        self.filter_coefficient_ = 0.0;
    }

    #[inline]
    pub fn step(&mut self, rate: f32, burst_mode: bool, start_burst: bool, random: &mut Random) {
        let mut randomize = false;
        if start_burst {
            self.phase_ = 0.5;
            self.fm_ = 16.0;
            randomize = true;
        } else {
            self.phase_ += rate * self.fm_;
            if self.phase_ >= 1.0 {
                self.phase_ -= cast_i32(self.phase_) as f32;
                randomize = true;
            }
        }

        if randomize {
            self.from_ += self.interval_;
            self.interval_ = random.get_float() - self.from_;
            // Randomize the duration of the grain.
            if burst_mode {
                self.fm_ *= 0.8 + 0.2 * random.get_float();
            } else {
                self.fm_ = 0.5 + 1.5 * random.get_float();
            }
        }
    }

    #[inline]
    pub fn frequency(&self, size_ratio: f32) -> f32 {
        // We approximate two overlapping grains of frequencies f1 and f2
        // By a continuous tone ramping from f1 to f2. This allows a continuous
        // transition between the "grain cloud" and "swarm of glissandi" textures.
        if size_ratio < 1.0 {
            2.0 * (self.from_ + self.interval_ * self.phase_) - 1.0
        } else {
            self.from_
        }
    }

    #[inline]
    pub fn amplitude(&mut self, size_ratio: f32) -> f32 {
        let mut target_amplitude = 1.0f32;
        if size_ratio >= 1.0 {
            let mut phase = (self.phase_ - 0.5) * size_ratio;
            constrain(&mut phase, -1.0, 1.0);
            let e = sine(0.5 * phase + 1.25);
            target_amplitude = 0.5 * (e + 1.0);
        }

        if (size_ratio >= 1.0) ^ (self.previous_size_ratio_ >= 1.0) {
            self.filter_coefficient_ = 0.5;
        }
        self.filter_coefficient_ *= 0.95;

        self.previous_size_ratio_ = size_ratio;
        one_pole(
            &mut self.amplitude_,
            target_amplitude,
            0.5 - self.filter_coefficient_,
        );
        self.amplitude_
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub struct AdditiveSawOscillator {
    // Oscillator state.
    phase_: f32,
    next_sample_: f32,

    // For interpolation of parameters.
    frequency_: f32,
    gain_: f32,
}

impl AdditiveSawOscillator {
    #[inline]
    pub fn init(&mut self) {
        self.phase_ = 0.0;
        self.next_sample_ = 0.0;
        self.frequency_ = 0.01;
        self.gain_ = 0.0;
    }

    #[inline]
    pub fn render(&mut self, mut frequency: f32, level: f32, out: &mut [f32], size: usize) {
        if frequency >= K_MAX_FREQUENCY {
            frequency = K_MAX_FREQUENCY;
        }
        let mut fm = ParameterInterpolator::new(&mut self.frequency_, frequency, size);
        let mut gain = ParameterInterpolator::new(&mut self.gain_, level, size);

        let mut next_sample = self.next_sample_;
        let mut phase = self.phase_;

        for out in &mut out[..size] {
            let mut this_sample = next_sample;
            next_sample = 0.0;

            let frequency = fm.next();

            phase += frequency;

            if phase >= 1.0 {
                phase -= 1.0;
                let t = phase / frequency;
                this_sample -= this_blep_sample(t);
                next_sample -= next_blep_sample(t);
            }

            next_sample += phase;
            *out += (2.0 * this_sample - 1.0) * gain.next();
        }
        self.phase_ = phase;
        self.next_sample_ = next_sample;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub struct SwarmVoice {
    rank_: f32,

    envelope_: GrainEnvelope,
    saw_: AdditiveSawOscillator,
    sine_: FastSineOscillator,
}

const _: () = assert!(size_of::<SwarmVoice>() == 64, "SwarmVoice's x64 layout");

impl SwarmVoice {
    pub fn init(&mut self, rank: f32) {
        self.rank_ = rank;
        self.envelope_.init();
        self.saw_.init();
        self.sine_.init();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        mut f0: f32,
        density: f32,
        burst_mode: bool,
        start_burst: bool,
        spread: f32,
        size_ratio: f32,
        saw: &mut [f32],
        sine: &mut [f32],
        size: usize,
        random: &mut Random,
    ) {
        self.envelope_
            .step(density, burst_mode, start_burst, random);

        let scale = 1.0 / K_NUM_SWARM_VOICES as f32;
        let amplitude = self.envelope_.amplitude(size_ratio) * scale;

        let expo_amount = self.envelope_.frequency(size_ratio);
        f0 *= semitones_to_ratio(48.0 * expo_amount * spread * self.rank_);

        let linear_amount = self.rank_ * (self.rank_ + 0.01) * spread * 0.25;
        f0 *= 1.0 + linear_amount;

        self.saw_.render(f0, amplitude, saw, size);
        self.sine_.render_additive(f0, amplitude, sine, size);
    }
}

#[derive(Default)]
pub struct SwarmEngine {
    pub post_processing_settings: PostProcessingSettings,

    /// Arena offset of `kNumSwarmVoices` `SwarmVoice`s.
    swarm_voice_: usize,
}

impl Engine for SwarmEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.swarm_voice_ = allocator
            .allocate::<SwarmVoice>(K_NUM_SWARM_VOICES as usize)
            .expect("the arena holds the swarm's voices");
    }

    fn reset(&mut self, arena: &mut Arena) {
        let swarm_voice =
            arena.view_mut::<SwarmVoice>(self.swarm_voice_, K_NUM_SWARM_VOICES as usize);
        // Integer division, as in the C++: 3, not 3.5, so the ranks run from -1 to +1.33.
        let n = ((K_NUM_SWARM_VOICES - 1) / 2) as f32;
        for (i, voice) in swarm_voice.iter_mut().enumerate() {
            let rank = (i as f32 - n) / n;
            voice.init(rank);
        }
    }

    fn load_user_data(&mut self, _user_data: Option<&'static [u8]>, _arena: &mut Arena) {}

    fn render(
        &mut self,
        parameters: &EngineParameters,
        out: &mut [f32],
        aux: &mut [f32],
        size: usize,
        _already_enveloped: &mut bool,
        arena: &mut Arena,
        random: &mut Random,
    ) {
        let swarm_voice =
            arena.view_mut::<SwarmVoice>(self.swarm_voice_, K_NUM_SWARM_VOICES as usize);

        let f0 = note_to_frequency(parameters.note);
        let control_rate = size as f32;
        let density = note_to_frequency(parameters.timbre * 120.0) * 0.025 * control_rate;
        let spread = parameters.harmonics * parameters.harmonics * parameters.harmonics;
        let mut size_ratio = 0.25 * semitones_to_ratio((1.0 - parameters.morph) * 84.0);

        let burst_mode = parameters.trigger & TRIGGER_UNPATCHED == 0;
        let start_burst = parameters.trigger & TRIGGER_RISING_EDGE != 0;

        out[..size].fill(0.0);
        aux[..size].fill(0.0);

        for voice in swarm_voice.iter_mut() {
            voice.render(
                f0,
                density,
                burst_mode,
                start_burst,
                spread,
                size_ratio,
                out,
                aux,
                size,
                random,
            );
            size_ratio *= 0.97;
        }
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
