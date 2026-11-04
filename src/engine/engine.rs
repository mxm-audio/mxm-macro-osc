//! Ported from plaits/dsp/engine/engine.h: base class for all engines.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `Engine` is a trait. What the C++ engines reach through globals and pointers they kept, a
//! Rust engine is handed: the voice's [`Arena`] (the shared buffer its regions are offsets into)
//! and its [`Random`] (stmlib's one static generator). `post_processing_settings`, a public
//! member of the C++ base class, is a field of each engine, reached through the trait.
//!
//! The registry holds pointers to the voice's engine members in the C++; here it holds a handle
//! of the voice's choosing (`I`) per registered engine, which the voice turns into the member.

use crate::dsp::A0;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[inline]
pub fn note_to_frequency(mut midi_note: f32) -> f32 {
    midi_note -= 9.0;
    constrain(&mut midi_note, -128.0, 127.0);
    A0 * 0.25 * semitones_to_ratio(midi_note)
}

pub type TriggerState = i32;
pub const TRIGGER_LOW: TriggerState = 0;
pub const TRIGGER_RISING_EDGE: TriggerState = 1;
pub const TRIGGER_UNPATCHED: TriggerState = 2;
pub const TRIGGER_HIGH: TriggerState = 4;

#[derive(Clone, Copy, Default, Debug)]
pub struct EngineParameters {
    pub trigger: i32,
    pub note: f32,
    pub timbre: f32,
    pub morph: f32,
    pub harmonics: f32,
    pub accent: f32,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct PostProcessingSettings {
    /// A negative value indicates that a limiter must be used.
    pub out_gain: f32,
    pub aux_gain: f32,

    /// When this flag is set to true, the engine declares that it will
    /// render a signal that already has an envelope (eg: modal drum, 808 kick).
    /// By reporting this information, the synthesis voice upstream will
    /// bypass the internal envelope/LPG.
    ///
    /// This parameter can be changed on a per-call basis when calling Render()
    /// This is used by the speech synthesis engine, which renders either
    /// a continuous vowel sound (which needs to be enveloped by the LPG)
    /// or a word/sentence (which is already enveloped).
    pub already_enveloped: bool,
}

pub trait Engine {
    /// `Init(BufferAllocator* allocator)`: the voice frees the allocator before each engine's
    /// `init`, so every engine's regions start at byte 0 of the arena.
    fn init(&mut self, allocator: &mut BufferAllocator<'_>);

    fn reset(&mut self, arena: &mut Arena);

    /// `LoadUserData(const uint8_t* user_data)`: `None` for `NULL`. The module's user data is
    /// not ported, so this is only ever the built-in six-op banks, or `None`.
    fn load_user_data(&mut self, user_data: Option<&'static [u8]>, arena: &mut Arena);

    /// `Render(parameters, out, aux, size, already_enveloped)`: `out` and `aux` are the voice's
    /// `kMaxBlockSize` buffers, of which the engine renders the first `size` samples.
    #[allow(clippy::too_many_arguments)]
    fn render(
        &mut self,
        parameters: &EngineParameters,
        out: &mut [f32],
        aux: &mut [f32],
        size: usize,
        already_enveloped: &mut bool,
        arena: &mut Arena,
        random: &mut Random,
    );

    fn post_processing_settings(&self) -> &PostProcessingSettings;

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings;
}

pub struct EngineRegistry<I: Copy + Default, const MAX_SIZE: usize> {
    engine_: [I; MAX_SIZE],
    num_engines_: i32,
}

impl<I: Copy + Default, const MAX_SIZE: usize> Default for EngineRegistry<I, MAX_SIZE> {
    fn default() -> Self {
        Self {
            engine_: [I::default(); MAX_SIZE],
            num_engines_: 0,
        }
    }
}

impl<I: Copy + Default, const MAX_SIZE: usize> EngineRegistry<I, MAX_SIZE> {
    pub fn init(&mut self) {
        self.num_engines_ = 0;
    }

    /// The handle registered at `index`, which the C++ indexes without a check; the voice's
    /// quantizer keeps it in range.
    #[inline]
    pub fn get(&self, index: i32) -> I {
        self.engine_[index as usize]
    }

    /// `RegisterInstance(instance, already_enveloped, out_gain, aux_gain)`: `handle` names the
    /// instance for `get`, and `engine` is it, whose settings this writes. One instance
    /// registered twice keeps the settings of its last registration, as in the C++.
    pub fn register_instance(
        &mut self,
        handle: I,
        engine: &mut dyn Engine,
        already_enveloped: bool,
        out_gain: f32,
        aux_gain: f32,
    ) {
        if self.num_engines_ >= MAX_SIZE as i32 {
            return;
        }
        self.engine_[self.num_engines_ as usize] = handle;
        let s = engine.post_processing_settings_mut();
        s.already_enveloped = already_enveloped;
        s.out_gain = out_gain;
        s.aux_gain = aux_gain;
        self.num_engines_ += 1;
    }

    #[inline]
    pub fn size(&self) -> i32 {
        self.num_engines_
    }
}
