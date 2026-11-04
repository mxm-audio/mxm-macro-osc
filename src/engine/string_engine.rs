//! Ported from plaits/dsp/engine/string_engine.{h,cc}: three voices of string synthesis.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `temp_buffer_`, each string's two delay lines and `f0_delay_`'s line are in the arena, as in
//! the C++, in that order of allocation. The voices share `temp_buffer_` while each works on its
//! own delay lines in the arena too, so the engine hands them a copy of it and writes the copy
//! back after the last: the arena ends each block holding the bytes the C++'s does.

use super::engine::{
    Engine, EngineParameters, PostProcessingSettings, TRIGGER_RISING_EDGE, TRIGGER_UNPATCHED,
    note_to_frequency,
};
use crate::dsp::K_MAX_BLOCK_SIZE;
use crate::physical_modelling::delay_line::DelayLine;
use crate::physical_modelling::string_voice::StringVoice;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

pub const K_NUM_STRINGS: usize = 3;

#[derive(Default)]
pub struct StringEngine {
    pub post_processing_settings: PostProcessingSettings,

    voice_: [StringVoice; K_NUM_STRINGS],

    f0_: [f32; K_NUM_STRINGS],
    f0_delay_: DelayLine<16>,
    /// Arena offset of `f0_delay_`'s 16 floats.
    f0_delay_line_: usize,
    active_string_: i32,
    /// Arena offset of `kMaxBlockSize` floats.
    temp_buffer_: usize,
}

impl Engine for StringEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.temp_buffer_ = allocator
            .allocate::<f32>(K_MAX_BLOCK_SIZE)
            .expect("the arena holds the string engine's buffer");
        for i in 0..K_NUM_STRINGS {
            self.voice_[i].init(allocator);
            self.f0_[i] = 0.01;
        }
        self.active_string_ = K_NUM_STRINGS as i32 - 1;
        self.f0_delay_line_ = allocator
            .allocate::<f32>(16)
            .expect("the arena holds the string engine's pitch delay");
        self.f0_delay_.init(
            allocator
                .arena_mut()
                .view_mut::<f32>(self.f0_delay_line_, 16),
        );
    }

    fn reset(&mut self, arena: &mut Arena) {
        self.f0_delay_
            .reset(arena.view_mut::<f32>(self.f0_delay_line_, 16));
        for i in 0..K_NUM_STRINGS {
            self.voice_[i].reset(arena);
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
        if parameters.trigger & TRIGGER_RISING_EDGE != 0 {
            // 8 in original firmware version.
            // 05.01.18: mic.w: problem with microbrute.
            self.f0_[self.active_string_ as usize] = self
                .f0_delay_
                .read(arena.view::<f32>(self.f0_delay_line_, 16), 14.0);
            self.active_string_ = (self.active_string_ + 1) % K_NUM_STRINGS as i32;
        }

        let f0 = note_to_frequency(parameters.note);
        self.f0_[self.active_string_ as usize] = f0;
        self.f0_delay_
            .write(arena.view_mut::<f32>(self.f0_delay_line_, 16), f0);

        out[..size].fill(0.0);
        aux[..size].fill(0.0);

        let mut temp_buffer = [0.0f32; K_MAX_BLOCK_SIZE];
        temp_buffer.copy_from_slice(arena.view::<f32>(self.temp_buffer_, K_MAX_BLOCK_SIZE));
        for i in 0..K_NUM_STRINGS {
            let active = i as i32 == self.active_string_;
            self.voice_[i].render(
                parameters.trigger & TRIGGER_UNPATCHED != 0 && active,
                parameters.trigger & TRIGGER_RISING_EDGE != 0 && active,
                parameters.accent,
                self.f0_[i],
                parameters.harmonics,
                parameters.timbre * parameters.timbre,
                parameters.morph,
                &mut temp_buffer,
                out,
                aux,
                size,
                arena,
                random,
            );
        }
        arena
            .view_mut::<f32>(self.temp_buffer_, K_MAX_BLOCK_SIZE)
            .copy_from_slice(&temp_buffer);
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
