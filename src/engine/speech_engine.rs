//! Ported from plaits/dsp/engine/speech_engine.{h,cc}: various flavours of speech synthesis.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The arena holds, from byte 0, the word bank's 1 024 frames and 32 word boundaries, then the
//! two temp buffers: 14 656 bytes. The naive and SAM synths are handed
//! `trigger == TRIGGER_RISING_EDGE`, which never holds (the voice sends a rising edge with
//! `TRIGGER_HIGH`), as in the C++.

use super::engine::{
    Engine, EngineParameters, PostProcessingSettings, TRIGGER_RISING_EDGE, TRIGGER_UNPATCHED,
    note_to_frequency,
};
use crate::dsp::K_MAX_BLOCK_SIZE;
use crate::speech::lpc_speech_synth_controller::{
    LPCSpeechSynthController, LPCSpeechSynthWordBank,
};
use crate::speech::lpc_speech_synth_words::{LPC_SPEECH_SYNTH_NUM_WORD_BANKS, WORD_BANKS_};
use crate::speech::naive_speech_synth::NaiveSpeechSynth;
use crate::speech::sam_speech_synth::SAMSpeechSynth;
use crate::stmlib::dsp::hysteresis_quantizer::HysteresisQuantizer2;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct SpeechEngine {
    pub post_processing_settings: PostProcessingSettings,

    word_bank_quantizer_: HysteresisQuantizer2,

    naive_speech_synth_: NaiveSpeechSynth,
    sam_speech_synth_: SAMSpeechSynth,

    lpc_speech_synth_controller_: LPCSpeechSynthController,
    lpc_speech_synth_word_bank_: LPCSpeechSynthWordBank,

    /// Arena offsets of two buffers of `kMaxBlockSize` floats.
    temp_buffer_: [usize; 2],
    prosody_amount_: f32,
    speed_: f32,
}

impl SpeechEngine {
    #[inline]
    pub fn set_prosody_amount(&mut self, prosody_amount: f32) {
        self.prosody_amount_ = prosody_amount;
    }

    #[inline]
    pub fn set_speed(&mut self, speed: f32) {
        self.speed_ = speed;
    }
}

impl Engine for SpeechEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.sam_speech_synth_.init();
        self.naive_speech_synth_.init();
        self.lpc_speech_synth_word_bank_.init(
            &WORD_BANKS_,
            LPC_SPEECH_SYNTH_NUM_WORD_BANKS as i32,
            allocator,
        );
        self.lpc_speech_synth_controller_.init();
        self.word_bank_quantizer_
            .init(LPC_SPEECH_SYNTH_NUM_WORD_BANKS as i32 + 1, 0.1, false);

        self.temp_buffer_[0] = allocator
            .allocate::<f32>(K_MAX_BLOCK_SIZE)
            .expect("the arena holds the engine's temp buffers");
        self.temp_buffer_[1] = allocator
            .allocate::<f32>(K_MAX_BLOCK_SIZE)
            .expect("the arena holds the engine's temp buffers");

        self.prosody_amount_ = 0.0;
        self.speed_ = 0.0;
    }

    fn reset(&mut self, arena: &mut Arena) {
        self.lpc_speech_synth_word_bank_.reset(arena);
    }

    fn load_user_data(&mut self, _user_data: Option<&'static [u8]>, _arena: &mut Arena) {}

    fn render(
        &mut self,
        parameters: &EngineParameters,
        out: &mut [f32],
        aux: &mut [f32],
        size: usize,
        already_enveloped: &mut bool,
        arena: &mut Arena,
        random: &mut Random,
    ) {
        let f0 = note_to_frequency(parameters.note);

        let group = parameters.harmonics * 6.0;

        // Interpolates between the 3 models: naive, SAM, LPC.
        if group <= 2.0 {
            *already_enveloped = false;

            let mut blend = group;
            if group <= 1.0 {
                let temp_0 = arena.view_mut::<f32>(self.temp_buffer_[0], K_MAX_BLOCK_SIZE);
                self.naive_speech_synth_.render(
                    parameters.trigger == TRIGGER_RISING_EDGE,
                    f0,
                    parameters.morph,
                    parameters.timbre,
                    temp_0,
                    aux,
                    out,
                    size,
                );
            } else {
                self.lpc_speech_synth_controller_.render(
                    (parameters.trigger & TRIGGER_UNPATCHED) != 0,
                    (parameters.trigger & TRIGGER_RISING_EDGE) != 0,
                    -1,
                    f0,
                    0.0,
                    0.0,
                    parameters.morph,
                    parameters.timbre,
                    1.0,
                    aux,
                    out,
                    size,
                    &mut self.lpc_speech_synth_word_bank_,
                    arena,
                    random,
                );
                blend = 2.0 - blend;
            }

            let (temp_0, temp_1) = arena.view_mut2::<f32, f32>(
                self.temp_buffer_[0],
                K_MAX_BLOCK_SIZE,
                self.temp_buffer_[1],
                K_MAX_BLOCK_SIZE,
            );
            self.sam_speech_synth_.render(
                parameters.trigger == TRIGGER_RISING_EDGE,
                f0,
                parameters.morph,
                parameters.timbre,
                temp_0,
                temp_1,
                size,
            );

            // Two smoothsteps, as the C++ writes them: `blend * (blend * (3 - 2 blend))`, which
            // rounds unlike the `(blend * blend) * …` clippy would write.
            #[allow(clippy::misrefactored_assign_op)]
            {
                blend *= blend * (3.0 - 2.0 * blend);
                blend *= blend * (3.0 - 2.0 * blend);
            }
            for i in 0..size {
                aux[i] += (temp_0[i] - aux[i]) * blend;
                out[i] += (temp_1[i] - out[i]) * blend;
            }
        } else {
            // Change phonemes/words for LPC.
            let word_bank = self
                .word_bank_quantizer_
                .process((group - 2.0) * 0.275)
                .wrapping_sub(1);

            let replay_prosody = word_bank >= 0 && (parameters.trigger & TRIGGER_UNPATCHED) == 0;

            *already_enveloped = replay_prosody;

            self.lpc_speech_synth_controller_.render(
                (parameters.trigger & TRIGGER_UNPATCHED) != 0,
                (parameters.trigger & TRIGGER_RISING_EDGE) != 0,
                word_bank,
                f0,
                self.prosody_amount_,
                self.speed_,
                parameters.morph,
                parameters.timbre,
                if replay_prosody {
                    parameters.accent
                } else {
                    1.0
                },
                aux,
                out,
                size,
                &mut self.lpc_speech_synth_word_bank_,
                arena,
                random,
            );
        }
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
