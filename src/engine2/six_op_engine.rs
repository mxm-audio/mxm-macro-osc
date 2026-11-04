//! Ported from plaits/dsp/engine2/six_op_engine.{h,cc}: 6-operator FM synth.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The arena holds what the C++ allocates: `temp_buffer_` (96 floats), `acc_buffer_` (48) and
//! the 32 unpacked patches (`fm::Patch`, 156 bytes each, as on x64). Neither `LoadUserData` nor
//! the empty `Reset` clears the accumulator, so the first block after an engine change reads
//! what the previous engine left there, as the C++ does. An `FMVoice`'s patch pointer
//! (`&patches_[i]`) is the index `i`; the voices' pointer to `algorithms_` is a parameter of the
//! calls that use it.

use crate::dsp::{K_CORRECTED_SAMPLE_RATE, K_MAX_BLOCK_SIZE};
use crate::engine::engine::{
    Engine, EngineParameters, PostProcessingSettings, TRIGGER_HIGH, TRIGGER_RISING_EDGE,
    TRIGGER_UNPATCHED,
};
use crate::fm::algorithms::Algorithms;
use crate::fm::lfo::Lfo;
use crate::fm::patch::Patch;
use crate::fm::voice::{Parameters, Voice};
use crate::stmlib::dsp::dsp::soft_clip;
use crate::stmlib::dsp::hysteresis_quantizer::HysteresisQuantizer2;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

pub const K_NUM_SIX_OP_VOICES: usize = 2;

#[derive(Default)]
pub struct FMVoice {
    /// The patch, as its index among the engine's patches; `None` for `NULL`.
    patch_: Option<usize>,

    lfo_: Lfo,
    voice_: Voice,
    parameters_: Parameters,
}

impl FMVoice {
    /// `Init(algorithms, sample_rate)`: the algorithms are `render`'s parameter.
    pub fn init(&mut self, sample_rate: f32) {
        self.voice_.init(sample_rate);
        self.lfo_.init(sample_rate);

        self.parameters_.sustain = false;
        self.parameters_.gate = false;
        self.parameters_.note = 48.0;
        self.parameters_.velocity = 0.5;
        self.parameters_.brightness = 0.5;
        self.parameters_.envelope_control = 0.5;
        self.parameters_.pitch_mod = 0.0;
        self.parameters_.amp_mod = 0.0;

        self.patch_ = None;
    }

    /// `LoadPatch(&patches[patch])`.
    pub fn load_patch(&mut self, patch: usize, patches: &[Patch]) {
        if Some(patch) == self.patch_ {
            return;
        }
        self.patch_ = Some(patch);
        self.voice_.set_patch(self.patch_);
        self.lfo_.set(&patches[patch].modulations);
    }

    pub fn render(
        &mut self,
        algorithms: &Algorithms,
        patches: &[Patch],
        buffer: &mut [f32],
        size: usize,
    ) {
        if self.patch_.is_none() {
            return;
        }
        self.voice_
            .render(&self.parameters_, algorithms, patches, buffer, size);
    }

    #[inline]
    pub fn unload_patch(&mut self) {
        self.patch_ = None;
    }

    #[inline]
    pub fn patch(&self) -> Option<usize> {
        self.patch_
    }

    #[inline]
    pub fn mutable_parameters(&mut self) -> &mut Parameters {
        &mut self.parameters_
    }

    #[inline]
    pub fn mutable_lfo(&mut self) -> &mut Lfo {
        &mut self.lfo_
    }

    #[inline]
    pub fn lfo(&self) -> &Lfo {
        &self.lfo_
    }

    #[inline]
    pub fn set_modulations(&mut self, lfo: &Lfo) {
        self.parameters_.pitch_mod = lfo.pitch_mod();
        self.parameters_.amp_mod = lfo.amp_mod();
    }
}

const K_NUM_PATCHES_PER_BANK: usize = 32;

/// `temp_buffer_`'s floats.
const K_TEMP_BUFFER_SIZE: usize = K_MAX_BLOCK_SIZE * 4;
/// `acc_buffer_`'s floats.
const K_ACC_BUFFER_SIZE: usize = K_MAX_BLOCK_SIZE * K_NUM_SIX_OP_VOICES;

#[derive(Default)]
pub struct SixOpEngine {
    pub post_processing_settings: PostProcessingSettings,

    patch_index_quantizer_: HysteresisQuantizer2,
    algorithms_: Algorithms,
    /// Arena offset of `kNumPatchesPerBank` `fm::Patch`es.
    patches_: usize,
    voice_: [FMVoice; K_NUM_SIX_OP_VOICES],
    /// Arena offset of `kMaxBlockSize * 4` floats.
    temp_buffer_: usize,
    /// Arena offset of `kMaxBlockSize * kNumSixOpVoices` floats, just after `temp_buffer_`.
    acc_buffer_: usize,
    active_voice_: i32,
    rendered_voice_: i32,
}

impl SixOpEngine {
    /// `temp_buffer_` and `acc_buffer_`, which `Init` allocates one after the other, as one view
    /// of floats from `temp_buffer_`, and where `acc_buffer_` starts in it: a render of
    /// `kMaxBlockSize` samples has the voice's buffers run past `temp_buffer_` into
    /// `acc_buffer_`, as in the C++, and this view holds them.
    fn buffers<'a>(&self, arena: &'a mut Arena) -> (&'a mut [f32], usize) {
        let acc = (self.acc_buffer_ - self.temp_buffer_) / size_of::<f32>();
        (
            arena.view_mut::<f32>(self.temp_buffer_, acc + K_ACC_BUFFER_SIZE),
            acc,
        )
    }

    /// `voice_[i].set_modulations(voice_[j].lfo())`.
    fn set_modulations(&mut self, i: usize, j: usize) {
        let lfo = *self.voice_[j].lfo();
        self.voice_[i].set_modulations(&lfo);
    }
}

impl Engine for SixOpEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.patch_index_quantizer_.init(32, 0.005, false);

        self.algorithms_.init();
        for i in 0..K_NUM_SIX_OP_VOICES {
            self.voice_[i].init(K_CORRECTED_SAMPLE_RATE);
        }
        self.temp_buffer_ = allocator
            .allocate::<f32>(K_TEMP_BUFFER_SIZE)
            .expect("the arena holds the six-op temp buffer");
        self.acc_buffer_ = allocator
            .allocate::<f32>(K_ACC_BUFFER_SIZE)
            .expect("the arena holds the six-op accumulator");
        self.patches_ = allocator
            .allocate::<Patch>(K_NUM_PATCHES_PER_BANK)
            .expect("the arena holds the six-op patches");

        self.active_voice_ = K_NUM_SIX_OP_VOICES as i32 - 1;
        self.rendered_voice_ = 0;
    }

    fn reset(&mut self, _arena: &mut Arena) {}

    /// `LoadUserData(user_data)`: the voice passes this engine a bank of
    /// `kNumPatchesPerBank × SYX_SIZE` bytes, never `NULL`.
    fn load_user_data(&mut self, user_data: Option<&'static [u8]>, arena: &mut Arena) {
        if let Some(user_data) = user_data {
            let patches = arena.view_mut::<Patch>(self.patches_, K_NUM_PATCHES_PER_BANK);
            for (i, patch) in patches.iter_mut().enumerate() {
                patch.unpack(&user_data[i * Patch::SYX_SIZE..]);
            }
        }
        for i in 0..K_NUM_SIX_OP_VOICES {
            self.voice_[i].unload_patch();
        }
    }

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
        let patch_index = self
            .patch_index_quantizer_
            .process(parameters.harmonics * 1.02) as usize;

        {
            let patches = arena.view::<Patch>(self.patches_, K_NUM_PATCHES_PER_BANK);

            if parameters.trigger & TRIGGER_UNPATCHED != 0 {
                let t = parameters.morph;
                self.voice_[0]
                    .mutable_lfo()
                    .scrub(2.0 * K_CORRECTED_SAMPLE_RATE * t, random);

                for i in 0..K_NUM_SIX_OP_VOICES {
                    self.voice_[i].load_patch(patch_index, patches);
                    let p = self.voice_[i].mutable_parameters();
                    p.sustain = i == 0;
                    p.gate = false;
                    p.note = parameters.note;
                    p.velocity = parameters.accent;
                    p.brightness = parameters.timbre;
                    p.envelope_control = t;
                    self.set_modulations(i, 0);
                }
            } else {
                if parameters.trigger & TRIGGER_RISING_EDGE != 0 {
                    self.active_voice_ = (self.active_voice_ + 1) % K_NUM_SIX_OP_VOICES as i32;
                    let active = self.active_voice_ as usize;
                    self.voice_[active].load_patch(patch_index, patches);
                    self.voice_[active].mutable_lfo().reset();
                }
                let active = self.active_voice_ as usize;
                let p = self.voice_[active].mutable_parameters();
                p.note = parameters.note;
                p.velocity = parameters.accent;
                p.envelope_control = parameters.morph;
                self.voice_[active].mutable_lfo().step(size as f32, random);

                for i in 0..K_NUM_SIX_OP_VOICES {
                    let p = self.voice_[i].mutable_parameters();
                    p.brightness = parameters.timbre;
                    p.sustain = false;
                    p.gate = (parameters.trigger & TRIGGER_HIGH != 0) && (i == active);
                    if self.voice_[i].patch() != self.voice_[active].patch() {
                        self.voice_[i].mutable_lfo().step(size as f32, random);
                        self.set_modulations(i, i);
                    } else {
                        self.set_modulations(i, active);
                    }
                }
            }
        }

        // Naive block rendering.
        // fill(temp_buffer_[0], temp_buffer_[size], 0.0f);
        // for (int i = 0; i < kNumSixOpVoices; ++i) {
        //   voice_[i].Render(temp_buffer_, size);
        // }

        // Staggered rendering.
        let (buffers, acc) = self.buffers(arena);
        buffers.copy_within(acc..acc + (K_NUM_SIX_OP_VOICES - 1) * size, 0);
        buffers[(K_NUM_SIX_OP_VOICES - 1) * size..K_NUM_SIX_OP_VOICES * size].fill(0.0);
        self.rendered_voice_ = (self.rendered_voice_ + 1) % K_NUM_SIX_OP_VOICES as i32;

        // The buffers end where the patches start, so both can be viewed at once.
        let buffers_size = acc + K_ACC_BUFFER_SIZE;
        let (temp_buffer, patches) = arena.view_mut2::<f32, Patch>(
            self.temp_buffer_,
            buffers_size,
            self.patches_,
            K_NUM_PATCHES_PER_BANK,
        );
        self.voice_[self.rendered_voice_ as usize].render(
            &self.algorithms_,
            patches,
            temp_buffer,
            size * K_NUM_SIX_OP_VOICES,
        );

        for i in 0..size {
            out[i] = soft_clip(temp_buffer[i] * 0.25);
            aux[i] = out[i];
        }
        temp_buffer.copy_within(size..K_NUM_SIX_OP_VOICES * size, acc);
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
