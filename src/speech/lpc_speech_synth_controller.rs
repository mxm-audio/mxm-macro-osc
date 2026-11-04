//! Ported from plaits/dsp/speech/lpc_speech_synth_controller.{h,cc}: so far only
//! `LPCSpeechSynthWordBankData`, the type `lpc_speech_synth_words` declares its banks with. The
//! rest of the file is ported with the speech engine.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).

/// One bank of LPC10-encoded words: `const uint8_t* data; size_t size;`.
#[derive(Clone, Copy, Debug)]
pub struct LPCSpeechSynthWordBankData {
    pub data: &'static [u8],
    pub size: usize,
}
