//! Ported from plaits/dsp/voice.{h,cc}: main synthesis voice.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! What the C++ keeps outside the voice, the voice owns here: plaits.cc's 16 KB
//! `shared_buffer` (the [`Arena`] the engines share) and stmlib's one static [`Random`]. The
//! registry's pointers to the engine members are [`EngineInstance`] handles, which the
//! `engine!` macro turns back into the member.
//!
//! Not ported: user data (`UserData::ptr` returns `NULL` in the desktop build the reference is,
//! so only the built-in six-op banks are ever loaded).

use bytemuck::{Pod, Zeroable};

use crate::cpp::max;
use crate::dsp::{K_BLOCK_SIZE, K_MAX_BLOCK_SIZE, K_SAMPLE_RATE};
use crate::engine::chord_engine::ChordEngine;
use crate::engine::engine::{
    Engine, EngineParameters, EngineRegistry, TRIGGER_HIGH, TRIGGER_LOW, TRIGGER_RISING_EDGE,
    TRIGGER_UNPATCHED, note_to_frequency,
};
use crate::engine::placeholder::PlaceholderEngine;
use crate::engine::speech_engine::SpeechEngine;
use crate::engine::virtual_analog_engine::VirtualAnalogEngine;
use crate::engine::wavetable_engine::WavetableEngine;
use crate::engine::{
    additive_engine::AdditiveEngine, fm_engine::FMEngine, grain_engine::GrainEngine,
    waveshaping_engine::WaveshapingEngine,
};
use crate::envelope::{DecayEnvelope, LPGEnvelope};
use crate::fx::low_pass_gate::LowPassGate;
use crate::physical_modelling::delay_line::DelayLine;
use crate::resources::FM_PATCHES_TABLE;
use crate::stmlib::dsp::dsp::{cast_i32, clip16};
use crate::stmlib::dsp::hysteresis_quantizer::HysteresisQuantizer2;
use crate::stmlib::dsp::limiter::Limiter;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

// The engines not yet ported: each a placeholder until it is (the crate's AGENTS.md).
type VirtualAnalogVCFEngine = PlaceholderEngine;
type PhaseDistortionEngine = PlaceholderEngine;
type SixOpEngine = PlaceholderEngine;
type WaveTerrainEngine = PlaceholderEngine;
type StringMachineEngine = PlaceholderEngine;
type ChiptuneEngine = PlaceholderEngine;
type SwarmEngine = PlaceholderEngine;
type NoiseEngine = PlaceholderEngine;
type ParticleEngine = PlaceholderEngine;
type StringEngine = PlaceholderEngine;
type ModalEngine = PlaceholderEngine;
type BassDrumEngine = PlaceholderEngine;
type SnareDrumEngine = PlaceholderEngine;
type HiHatEngine = PlaceholderEngine;

pub const K_MAX_ENGINES: usize = 24;
pub const K_MAX_TRIGGER_DELAY: usize = 8;
pub const K_TRIGGER_DELAY: i32 = 5;

#[derive(Default)]
pub struct ChannelPostProcessor {
    limiter_: Limiter,
    lpg_: LowPassGate,
}

impl ChannelPostProcessor {
    pub fn init(&mut self) {
        self.lpg_.init();
        self.reset();
    }

    pub fn reset(&mut self) {
        self.limiter_.init();
    }

    /// `out[0]`, `out[stride]`, … take the channel's 16-bit samples.
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        gain: f32,
        bypass_lpg: bool,
        low_pass_gate_gain: f32,
        low_pass_gate_frequency: f32,
        low_pass_gate_hf_bleed: f32,
        in_: &mut [f32],
        out: &mut [i16],
        size: usize,
        stride: usize,
    ) {
        if gain < 0.0 {
            self.limiter_.process(-gain, in_, size);
        }
        let post_gain = (if gain < 0.0 { 1.0 } else { gain }) * -32767.0;
        if !bypass_lpg {
            self.lpg_.process_to_shorts(
                post_gain * low_pass_gate_gain,
                low_pass_gate_frequency,
                low_pass_gate_hf_bleed,
                in_,
                out,
                size,
                stride,
            );
        } else {
            for (i, x) in in_[..size].iter().enumerate() {
                out[i * stride] = clip16(1i32.wrapping_add(cast_i32(*x * post_gain))) as i16;
            }
        }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Patch {
    pub note: f32,
    pub harmonics: f32,
    pub timbre: f32,
    pub morph: f32,
    pub frequency_modulation_amount: f32,
    pub timbre_modulation_amount: f32,
    pub morph_modulation_amount: f32,

    pub engine: i32,
    pub decay: f32,
    pub lpg_colour: f32,
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Modulations {
    pub engine: f32,
    pub note: f32,
    pub frequency: f32,
    pub harmonics: f32,
    pub timbre: f32,
    pub morph: f32,
    pub trigger: f32,
    pub level: f32,

    pub frequency_patched: bool,
    pub timbre_patched: bool,
    pub morph_patched: bool,
    pub trigger_patched: bool,
    pub level_patched: bool,
}

/// `Voice::Frame`: one sample of OUT and AUX, as the module hands its DAC.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct Frame {
    pub out: i16,
    pub aux: i16,
}

/// The voice's engine members, which the registry's handles name. The six-op engine is one
/// member registered three times.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum EngineInstance {
    #[default]
    VirtualAnalogVcf,
    PhaseDistortion,
    SixOp,
    WaveTerrain,
    StringMachine,
    Chiptune,
    VirtualAnalog,
    Waveshaping,
    Fm,
    Grain,
    Additive,
    Wavetable,
    Chord,
    Speech,
    Swarm,
    Noise,
    Particle,
    String,
    Modal,
    BassDrum,
    SnareDrum,
    HiHat,
}

/// The engine member an [`EngineInstance`] names, as `&mut dyn Engine`, borrowing only that
/// field of the voice (a macro, so the voice's other fields stay free to borrow).
macro_rules! engine {
    ($voice:ident, $instance:expr) => {
        match $instance {
            EngineInstance::VirtualAnalogVcf => {
                &mut $voice.virtual_analog_vcf_engine_ as &mut dyn Engine
            }
            EngineInstance::PhaseDistortion => {
                &mut $voice.phase_distortion_engine_ as &mut dyn Engine
            }
            EngineInstance::SixOp => &mut $voice.six_op_engine_ as &mut dyn Engine,
            EngineInstance::WaveTerrain => &mut $voice.wave_terrain_engine_ as &mut dyn Engine,
            EngineInstance::StringMachine => &mut $voice.string_machine_engine_ as &mut dyn Engine,
            EngineInstance::Chiptune => &mut $voice.chiptune_engine_ as &mut dyn Engine,
            EngineInstance::VirtualAnalog => &mut $voice.virtual_analog_engine_ as &mut dyn Engine,
            EngineInstance::Waveshaping => &mut $voice.waveshaping_engine_ as &mut dyn Engine,
            EngineInstance::Fm => &mut $voice.fm_engine_ as &mut dyn Engine,
            EngineInstance::Grain => &mut $voice.grain_engine_ as &mut dyn Engine,
            EngineInstance::Additive => &mut $voice.additive_engine_ as &mut dyn Engine,
            EngineInstance::Wavetable => &mut $voice.wavetable_engine_ as &mut dyn Engine,
            EngineInstance::Chord => &mut $voice.chord_engine_ as &mut dyn Engine,
            EngineInstance::Speech => &mut $voice.speech_engine_ as &mut dyn Engine,
            EngineInstance::Swarm => &mut $voice.swarm_engine_ as &mut dyn Engine,
            EngineInstance::Noise => &mut $voice.noise_engine_ as &mut dyn Engine,
            EngineInstance::Particle => &mut $voice.particle_engine_ as &mut dyn Engine,
            EngineInstance::String => &mut $voice.string_engine_ as &mut dyn Engine,
            EngineInstance::Modal => &mut $voice.modal_engine_ as &mut dyn Engine,
            EngineInstance::BassDrum => &mut $voice.bass_drum_engine_ as &mut dyn Engine,
            EngineInstance::SnareDrum => &mut $voice.snare_drum_engine_ as &mut dyn Engine,
            EngineInstance::HiHat => &mut $voice.hi_hat_engine_ as &mut dyn Engine,
        }
    };
}

#[derive(Default)]
pub struct Voice {
    virtual_analog_engine_: VirtualAnalogEngine,
    waveshaping_engine_: WaveshapingEngine,
    fm_engine_: FMEngine,
    grain_engine_: GrainEngine,
    additive_engine_: AdditiveEngine,
    wavetable_engine_: WavetableEngine,
    chord_engine_: ChordEngine,
    speech_engine_: SpeechEngine,

    swarm_engine_: SwarmEngine,
    noise_engine_: NoiseEngine,
    particle_engine_: ParticleEngine,
    string_engine_: StringEngine,
    modal_engine_: ModalEngine,
    bass_drum_engine_: BassDrumEngine,
    snare_drum_engine_: SnareDrumEngine,
    hi_hat_engine_: HiHatEngine,

    virtual_analog_vcf_engine_: VirtualAnalogVCFEngine,
    phase_distortion_engine_: PhaseDistortionEngine,
    six_op_engine_: SixOpEngine,
    wave_terrain_engine_: WaveTerrainEngine,
    string_machine_engine_: StringMachineEngine,
    chiptune_engine_: ChiptuneEngine,

    engine_quantizer_: HysteresisQuantizer2,

    reload_user_data_: bool,
    previous_engine_index_: i32,
    engine_cv_: f32,

    previous_note_: f32,
    trigger_state_: bool,

    decay_envelope_: DecayEnvelope,
    lpg_envelope_: LPGEnvelope,

    trigger_delay_line_: [f32; K_MAX_TRIGGER_DELAY],
    trigger_delay_: DelayLine<K_MAX_TRIGGER_DELAY>,

    out_post_processor_: ChannelPostProcessor,
    aux_post_processor_: ChannelPostProcessor,

    engines_: EngineRegistry<EngineInstance, K_MAX_ENGINES>,

    out_buffer_: [f32; K_MAX_BLOCK_SIZE],
    aux_buffer_: [f32; K_MAX_BLOCK_SIZE],

    /// plaits.cc's `shared_buffer`, the engines' memory.
    shared_buffer_: Arena,
    /// stmlib's `Random`, this voice's own.
    random_: Random,
}

impl Voice {
    /// A voice as the module starts one: every member zero (the C++ voice and its buffer are
    /// static), the random state at `0x21`, then `Init`.
    pub fn new() -> Box<Voice> {
        let mut voice = Box::<Voice>::default();
        voice.init();
        voice
    }

    /// `Init(BufferAllocator* allocator)`, the allocator over the voice's own arena.
    fn init(&mut self) {
        self.engines_.init();

        let e = &mut self.engines_;
        e.register_instance(
            EngineInstance::VirtualAnalogVcf,
            &mut self.virtual_analog_vcf_engine_,
            false,
            1.0,
            1.0,
        );
        e.register_instance(
            EngineInstance::PhaseDistortion,
            &mut self.phase_distortion_engine_,
            false,
            0.7,
            0.7,
        );
        e.register_instance(
            EngineInstance::SixOp,
            &mut self.six_op_engine_,
            true,
            1.0,
            1.0,
        );
        e.register_instance(
            EngineInstance::SixOp,
            &mut self.six_op_engine_,
            true,
            1.0,
            1.0,
        );
        e.register_instance(
            EngineInstance::SixOp,
            &mut self.six_op_engine_,
            true,
            1.0,
            1.0,
        );
        e.register_instance(
            EngineInstance::WaveTerrain,
            &mut self.wave_terrain_engine_,
            false,
            0.7,
            0.7,
        );
        e.register_instance(
            EngineInstance::StringMachine,
            &mut self.string_machine_engine_,
            false,
            0.8,
            0.8,
        );
        e.register_instance(
            EngineInstance::Chiptune,
            &mut self.chiptune_engine_,
            false,
            0.5,
            0.5,
        );

        e.register_instance(
            EngineInstance::VirtualAnalog,
            &mut self.virtual_analog_engine_,
            false,
            0.8,
            0.8,
        );
        e.register_instance(
            EngineInstance::Waveshaping,
            &mut self.waveshaping_engine_,
            false,
            0.7,
            0.6,
        );
        e.register_instance(EngineInstance::Fm, &mut self.fm_engine_, false, 0.6, 0.6);
        e.register_instance(
            EngineInstance::Grain,
            &mut self.grain_engine_,
            false,
            0.7,
            0.6,
        );
        e.register_instance(
            EngineInstance::Additive,
            &mut self.additive_engine_,
            false,
            0.8,
            0.8,
        );
        e.register_instance(
            EngineInstance::Wavetable,
            &mut self.wavetable_engine_,
            false,
            0.6,
            0.6,
        );
        e.register_instance(
            EngineInstance::Chord,
            &mut self.chord_engine_,
            false,
            0.8,
            0.8,
        );
        e.register_instance(
            EngineInstance::Speech,
            &mut self.speech_engine_,
            false,
            -0.7,
            0.8,
        );

        e.register_instance(
            EngineInstance::Swarm,
            &mut self.swarm_engine_,
            false,
            -3.0,
            1.0,
        );
        e.register_instance(
            EngineInstance::Noise,
            &mut self.noise_engine_,
            false,
            -1.0,
            -1.0,
        );
        e.register_instance(
            EngineInstance::Particle,
            &mut self.particle_engine_,
            false,
            -2.0,
            1.0,
        );
        e.register_instance(
            EngineInstance::String,
            &mut self.string_engine_,
            true,
            -1.0,
            0.8,
        );
        e.register_instance(
            EngineInstance::Modal,
            &mut self.modal_engine_,
            true,
            -1.0,
            0.8,
        );
        e.register_instance(
            EngineInstance::BassDrum,
            &mut self.bass_drum_engine_,
            true,
            0.8,
            0.8,
        );
        e.register_instance(
            EngineInstance::SnareDrum,
            &mut self.snare_drum_engine_,
            true,
            0.8,
            0.8,
        );
        e.register_instance(
            EngineInstance::HiHat,
            &mut self.hi_hat_engine_,
            true,
            0.8,
            0.8,
        );

        let mut allocator = BufferAllocator::new(&mut self.shared_buffer_);
        for i in 0..self.engines_.size() {
            // All engines will share the same RAM space.
            allocator.free();
            let instance = self.engines_.get(i);
            engine!(self, instance).init(&mut allocator);
        }

        self.engine_quantizer_
            .init(self.engines_.size(), 0.05, true);
        self.previous_engine_index_ = -1;
        self.reload_user_data_ = false;
        self.engine_cv_ = 0.0;

        self.out_post_processor_.init();
        self.aux_post_processor_.init();

        self.decay_envelope_.init();
        self.lpg_envelope_.init();

        self.trigger_state_ = false;
        self.previous_note_ = 0.0;

        self.trigger_delay_.init(&mut self.trigger_delay_line_);
    }

    pub fn reload_user_data(&mut self) {
        self.reload_user_data_ = true;
    }

    #[inline]
    pub fn active_engine(&self) -> i32 {
        self.previous_engine_index_
    }

    /// `Render(patch, modulations, frames, size)`, `size` being `frames.len()`: once per block
    /// of `kBlockSize` frames, as plaits.cc calls it. Frames past `kMaxBlockSize` (the most the
    /// C++'s buffers hold) are left as they are.
    pub fn render(&mut self, patch: &Patch, modulations: &Modulations, frames: &mut [Frame]) {
        let size = frames.len().min(K_MAX_BLOCK_SIZE);

        // Trigger, LPG, internal envelope.

        // Delay trigger by 1ms to deal with sequencers or MIDI interfaces whose
        // CV out lags behind the GATE out.
        self.trigger_delay_
            .write(&mut self.trigger_delay_line_, modulations.trigger);
        let trigger_value = self
            .trigger_delay_
            .read(&self.trigger_delay_line_, K_TRIGGER_DELAY as f32);

        let previous_trigger_state = self.trigger_state_;
        if !previous_trigger_state {
            if trigger_value > 0.3 {
                self.trigger_state_ = true;
                if !modulations.level_patched {
                    self.lpg_envelope_.trigger();
                }
                self.decay_envelope_.trigger();
                self.engine_cv_ = modulations.engine;
            }
        } else if trigger_value < 0.1 {
            self.trigger_state_ = false;
        }
        if !modulations.trigger_patched {
            self.engine_cv_ = modulations.engine;
        }

        // Engine selection.
        let engine_index = self
            .engine_quantizer_
            .process_base(patch.engine, self.engine_cv_);

        let instance = self.engines_.get(engine_index);

        if engine_index != self.previous_engine_index_ || self.reload_user_data_ {
            // `UserData::ptr` is NULL in the desktop build: only the built-in banks load.
            let data = if (2..=4).contains(&engine_index) {
                Some(FM_PATCHES_TABLE[(engine_index - 2) as usize])
            } else {
                None
            };
            engine!(self, instance).load_user_data(data, &mut self.shared_buffer_);
            engine!(self, instance).reset(&mut self.shared_buffer_);

            self.out_post_processor_.reset();
            self.previous_engine_index_ = engine_index;
            self.reload_user_data_ = false;
        }
        let mut p = EngineParameters::default();

        let rising_edge = self.trigger_state_ && !previous_trigger_state;
        let note = (modulations.note + self.previous_note_) * 0.5;
        self.previous_note_ = modulations.note;
        let pp_s = *engine!(self, instance).post_processing_settings();

        if modulations.trigger_patched {
            p.trigger = (if rising_edge {
                TRIGGER_RISING_EDGE
            } else {
                TRIGGER_LOW
            }) | (if self.trigger_state_ {
                TRIGGER_HIGH
            } else {
                TRIGGER_LOW
            });
        } else {
            p.trigger = TRIGGER_UNPATCHED;
        }

        let short_decay =
            (200.0 * K_BLOCK_SIZE as f32) / K_SAMPLE_RATE * semitones_to_ratio(-96.0 * patch.decay);

        self.decay_envelope_.process(short_decay * 2.0);

        let mut compressed_level = 1.3 * modulations.level / (0.3 + modulations.level.abs());
        constrain(&mut compressed_level, 0.0, 1.0);
        p.accent = if modulations.level_patched {
            compressed_level
        } else {
            0.8
        };

        let use_internal_envelope = modulations.trigger_patched;

        // Actual synthesis parameters.

        p.harmonics = patch.harmonics + modulations.harmonics;
        constrain(&mut p.harmonics, 0.0, 1.0);

        let mut internal_envelope_amplitude = 1.0f32;
        let mut internal_envelope_amplitude_timbre = 1.0f32;
        if engine_index == 15 {
            internal_envelope_amplitude = 2.0 - p.harmonics * 6.0;
            constrain(&mut internal_envelope_amplitude, 0.0, 1.0);
            self.speech_engine_.set_prosody_amount(
                if !modulations.trigger_patched || modulations.frequency_patched {
                    0.0
                } else {
                    patch.frequency_modulation_amount
                },
            );
            self.speech_engine_.set_speed(
                if !modulations.trigger_patched || modulations.morph_patched {
                    0.0
                } else {
                    patch.morph_modulation_amount
                },
            );
        } else if engine_index == 7 {
            if modulations.trigger_patched && !modulations.timbre_patched {
                // Disable internal envelope on TIMBRE, and enable the envelope generator
                // built into the chiptune engine.
                internal_envelope_amplitude_timbre = 0.0;
                self.chiptune_engine_
                    .set_envelope_shape(patch.timbre_modulation_amount);
            } else {
                self.chiptune_engine_
                    .set_envelope_shape(ChiptuneEngine::NO_ENVELOPE as f32);
            }
        }

        p.note = Self::apply_modulations(
            patch.note + note,
            patch.frequency_modulation_amount,
            modulations.frequency_patched,
            modulations.frequency,
            use_internal_envelope,
            internal_envelope_amplitude
                * self.decay_envelope_.value()
                * self.decay_envelope_.value()
                * 48.0,
            1.0,
            -119.0,
            120.0,
        );

        p.timbre = Self::apply_modulations(
            patch.timbre,
            patch.timbre_modulation_amount,
            modulations.timbre_patched,
            modulations.timbre,
            use_internal_envelope,
            internal_envelope_amplitude_timbre * self.decay_envelope_.value(),
            0.0,
            0.0,
            1.0,
        );

        p.morph = Self::apply_modulations(
            patch.morph,
            patch.morph_modulation_amount,
            modulations.morph_patched,
            modulations.morph,
            use_internal_envelope,
            internal_envelope_amplitude * self.decay_envelope_.value(),
            0.0,
            0.0,
            1.0,
        );

        let mut already_enveloped = pp_s.already_enveloped;
        engine!(self, instance).render(
            &p,
            &mut self.out_buffer_,
            &mut self.aux_buffer_,
            size,
            &mut already_enveloped,
            &mut self.shared_buffer_,
            &mut self.random_,
        );

        let lpg_bypass =
            already_enveloped || (!modulations.level_patched && !modulations.trigger_patched);

        // Compute LPG parameters.
        if !lpg_bypass {
            let hf = patch.lpg_colour;
            let decay_tail = (20.0 * K_BLOCK_SIZE as f32) / K_SAMPLE_RATE
                * semitones_to_ratio(-72.0 * patch.decay + 12.0 * hf)
                - short_decay;

            if modulations.level_patched {
                self.lpg_envelope_
                    .process_lp(compressed_level, short_decay, decay_tail, hf);
            } else {
                let attack = note_to_frequency(p.note) * K_BLOCK_SIZE as f32 * 2.0;
                self.lpg_envelope_
                    .process_ping(attack, short_decay, decay_tail, hf);
            }
        } else {
            self.lpg_envelope_.init();
        }

        // `&frames->out` and `&frames->aux`, each at a stride of two shorts.
        let shorts: &mut [i16] = bytemuck::cast_slice_mut(&mut frames[..size]);

        self.out_post_processor_.process(
            pp_s.out_gain,
            lpg_bypass,
            self.lpg_envelope_.gain(),
            self.lpg_envelope_.frequency(),
            self.lpg_envelope_.hf_bleed(),
            &mut self.out_buffer_,
            shorts,
            size,
            2,
        );

        let aux_start = shorts.len().min(1);
        self.aux_post_processor_.process(
            pp_s.aux_gain,
            lpg_bypass,
            self.lpg_envelope_.gain(),
            self.lpg_envelope_.frequency(),
            self.lpg_envelope_.hf_bleed(),
            &mut self.aux_buffer_,
            &mut shorts[aux_start..],
            size,
            2,
        );
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn apply_modulations(
        base_value: f32,
        mut modulation_amount: f32,
        use_external_modulation: bool,
        external_modulation: f32,
        use_internal_envelope: bool,
        envelope: f32,
        default_internal_modulation: f32,
        minimum_value: f32,
        maximum_value: f32,
    ) -> f32 {
        let mut value = base_value;
        modulation_amount *= max(modulation_amount.abs() - 0.05, 0.05);
        modulation_amount *= 1.05;

        let modulation = if use_external_modulation {
            external_modulation
        } else if use_internal_envelope {
            envelope
        } else {
            default_internal_modulation
        };
        value += modulation_amount * modulation;
        constrain(&mut value, minimum_value, maximum_value);
        value
    }
}
