//! Ported from plaits/dsp/speech/lpc_speech_synth_controller.{h,cc}: feeds frames to the LPC10
//! speech synth.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The word bank keeps its frames and word boundaries in the arena, as the C++ does in the
//! shared buffer: [`LPCSpeechSynthWordBank`] holds their offsets, and its methods take the arena.
//! The C++ controller keeps a pointer to the word bank; here the speech engine owns both and
//! passes the word bank, with the arena and the voice's [`Random`], to
//! [`LPCSpeechSynthController::render`]. `phonemes_`, defined in `lpc_speech_synth_phonemes.cc`,
//! is [`PHONEMES_`].

use super::lpc_speech_synth::{Frame, K_LPC_SPEECH_SYNTH_DEFAULT_F0, LPCSpeechSynth};
use super::lpc_speech_synth_phonemes::PHONEMES_;
use crate::cpp::{at, min};
use crate::dsp::{K_CORRECTED_SAMPLE_RATE, K_SAMPLE_RATE};
use crate::stmlib::dsp::dsp::{cast_i32, cast_usize};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::polyblep::{next_blep_sample, this_blep_sample};
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

/// `BitStream`: the C++ keeps a pointer into the data, here a slice and the index of the next
/// byte.
pub struct BitStream {
    data_: &'static [u8],
    p_: usize,
    available_: i32,
    bits_: u16,
}

impl BitStream {
    /// `BitStream()` then `Init(p)`, `p` being `data[p..]`.
    #[inline]
    pub fn new(data: &'static [u8], p: usize) -> Self {
        Self {
            data_: data,
            p_: p,
            available_: 0,
            bits_: 0,
        }
    }

    #[inline]
    pub fn flush(&mut self) {
        while self.available_ != 0 {
            self.get_bits(1);
        }
    }

    /// `GetBits(num_bits)`. The `uint16_t` shifts are taken modulo 2¹⁶, as the C++ assigns them
    /// back; past the end of the data a byte reads as 0 (Plaits' banks end on a whole word, so
    /// it never does).
    #[inline]
    pub fn get_bits(&mut self, num_bits: i32) -> u8 {
        let mut shift = num_bits;
        if num_bits > self.available_ {
            self.bits_ <<= self.available_;
            shift -= self.available_;
            self.bits_ |= Self::reverse(at(self.data_, self.p_ as i32)) as u16;
            self.p_ += 1;
            self.available_ += 8;
        }
        self.bits_ <<= shift;
        let result = (self.bits_ >> 8) as u8;
        self.bits_ &= 0xff;
        self.available_ -= num_bits;
        result
    }

    /// `ptr()`: the index of the next byte.
    #[inline]
    pub fn ptr(&self) -> usize {
        self.p_
    }

    /// The bits of `b` in reverse order, in the C++'s three steps (the first swaps the nibbles).
    #[allow(clippy::manual_rotate)]
    #[inline]
    fn reverse(mut b: u8) -> u8 {
        b = (b >> 4) | (b << 4);
        b = ((b & 0xcc) >> 2) | ((b & 0x33) << 2);
        b = ((b & 0xaa) >> 1) | ((b & 0x55) << 1);
        b
    }
}

pub const K_LPC_SPEECH_SYNTH_MAX_WORDS: usize = 32;
pub const K_LPC_SPEECH_SYNTH_MAX_FRAMES: usize = 1024;
pub const K_LPC_SPEECH_SYNTH_NUM_VOWELS: i32 = 5;
pub const K_LPC_SPEECH_SYNTH_NUM_CONSONANTS: i32 = 10;
pub const K_LPC_SPEECH_SYNTH_NUM_PHONEMES: usize =
    (K_LPC_SPEECH_SYNTH_NUM_VOWELS + K_LPC_SPEECH_SYNTH_NUM_CONSONANTS) as usize;
pub const K_LPC_SPEECH_SYNTH_FPS: f32 = 40.0;

/// One bank of LPC10-encoded words: `const uint8_t* data; size_t size;`.
#[derive(Clone, Copy, Debug)]
pub struct LPCSpeechSynthWordBankData {
    pub data: &'static [u8],
    pub size: usize,
}

static ENERGY_LUT_: [u8; 16] = [
    0x00, 0x02, 0x03, 0x04, 0x05, 0x07, 0x0a, 0x0f, 0x14, 0x20, 0x29, 0x39, 0x51, 0x72, 0xa1, 0xff,
];

static PERIOD_LUT_: [u8; 64] = [
    0, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38,
    39, 40, 41, 42, 43, 45, 47, 49, 51, 53, 54, 57, 59, 61, 63, 66, 69, 71, 73, 77, 79, 81, 85, 87,
    92, 95, 99, 102, 106, 110, 115, 119, 123, 128, 133, 138, 143, 149, 154, 160,
];

static K0_LUT_: [i16; 32] = [
    -32064, -31872, -31808, -31680, -31552, -31424, -31232, -30848, -30592, -30336, -30016, -29696,
    -29376, -28928, -28480, -27968, -26368, -24256, -21632, -18368, -14528, -10048, -5184, 0, 5184,
    10048, 14528, 18368, 21632, 24256, 26368, 27968,
];

static K1_LUT_: [i16; 32] = [
    -20992, -19328, -17536, -15552, -13440, -11200, -8768, -6272, -3712, -1088, 1536, 4160, 6720,
    9216, 11584, 13824, 15936, 17856, 19648, 21248, 22656, 24000, 25152, 26176, 27072, 27840,
    28544, 29120, 29632, 30080, 30464, 32384,
];

static K2_LUT_: [i8; 16] = [
    -110, -97, -83, -70, -56, -43, -29, -16, -2, 11, 25, 38, 52, 65, 79, 92,
];

static K3_LUT_: [i8; 16] = [
    -82, -68, -54, -40, -26, -12, 1, 15, 29, 43, 57, 71, 85, 99, 113, 126,
];

static K4_LUT_: [i8; 16] = [
    -82, -70, -59, -47, -35, -24, -12, -1, 11, 23, 34, 46, 57, 69, 81, 92,
];

static K5_LUT_: [i8; 16] = [
    -64, -53, -42, -31, -20, -9, 3, 14, 25, 36, 47, 58, 69, 80, 91, 102,
];

static K6_LUT_: [i8; 16] = [
    -77, -65, -53, -41, -29, -17, -5, 7, 19, 31, 43, 55, 67, 79, 90, 102,
];

static K7_LUT_: [i8; 8] = [-64, -40, -16, 7, 31, 55, 79, 102];

static K8_LUT_: [i8; 8] = [-64, -44, -24, -4, 16, 37, 57, 77];

static K9_LUT_: [i8; 8] = [-51, -33, -15, 4, 22, 32, 59, 77];

#[derive(Default)]
pub struct LPCSpeechSynthWordBank {
    word_banks_: &'static [LPCSpeechSynthWordBankData],

    num_banks_: i32,
    loaded_bank_: i32,
    num_frames_: i32,
    num_words_: i32,

    /// Arena offset of `kLPCSpeechSynthMaxWords` `int`s.
    word_boundaries_: usize,
    /// Arena offset of `kLPCSpeechSynthMaxFrames` frames.
    frames_: usize,
}

impl LPCSpeechSynthWordBank {
    pub fn init(
        &mut self,
        word_banks: &'static [LPCSpeechSynthWordBankData],
        num_banks: i32,
        allocator: &mut BufferAllocator<'_>,
    ) {
        self.word_banks_ = word_banks;
        self.num_banks_ = num_banks;
        self.frames_ = allocator
            .allocate::<Frame>(K_LPC_SPEECH_SYNTH_MAX_FRAMES)
            .expect("the arena holds the word bank's frames");
        self.word_boundaries_ = allocator
            .allocate::<i32>(K_LPC_SPEECH_SYNTH_MAX_WORDS)
            .expect("the arena holds the word bank's word boundaries");
        self.reset(allocator.arena_mut());
    }

    pub fn reset(&mut self, arena: &mut Arena) {
        self.loaded_bank_ = -1;
        self.num_frames_ = 0;
        self.num_words_ = 0;
        arena
            .view_mut::<i32>(self.word_boundaries_, K_LPC_SPEECH_SYNTH_MAX_WORDS)
            .fill(0);
    }

    #[inline]
    pub fn num_frames(&self) -> i32 {
        self.num_frames_
    }

    /// `frames()`: the frames, viewed in the arena.
    #[inline]
    pub fn frames<'a>(&self, arena: &'a Arena) -> &'a [Frame] {
        arena.view::<Frame>(self.frames_, K_LPC_SPEECH_SYNTH_MAX_FRAMES)
    }

    /// `GetWordBoundaries(address, &start, &end)`, returning `(start, end)`.
    #[inline]
    pub fn get_word_boundaries(&self, address: f32, arena: &Arena) -> (i32, i32) {
        if self.num_words_ == 0 {
            (-1, -1)
        } else {
            let mut word = cast_i32(address * self.num_words_ as f32);
            if word >= self.num_words_ {
                word = self.num_words_ - 1;
            }
            let word_boundaries =
                arena.view::<i32>(self.word_boundaries_, K_LPC_SPEECH_SYNTH_MAX_WORDS);
            (
                at(word_boundaries, word),
                at(word_boundaries, word.wrapping_add(1)).wrapping_sub(1),
            )
        }
    }

    /// `LoadNextWord(data)`, `data` being `bank[p..]`: the bytes it consumed.
    fn load_next_word(&mut self, bank: &'static [u8], p: usize, arena: &mut Arena) -> usize {
        let mut bitstream = BitStream::new(bank, p);

        let mut frame = Frame {
            energy: 0,
            period: 0,
            k0: 0,
            k1: 0,
            k2: 0,
            k3: 0,
            k4: 0,
            k5: 0,
            k6: 0,
            k7: 0,
            k8: 0,
            k9: 0,
        };

        let frames = arena.view_mut::<Frame>(self.frames_, K_LPC_SPEECH_SYNTH_MAX_FRAMES);
        loop {
            let energy = bitstream.get_bits(4) as i32;
            if energy == 0 {
                frame.energy = 0;
            } else if energy == 0xf {
                bitstream.flush();
                break;
            } else {
                frame.energy = ENERGY_LUT_[energy as usize];
                let repeat = bitstream.get_bits(1) != 0;
                frame.period = PERIOD_LUT_[bitstream.get_bits(6) as usize];
                if !repeat {
                    frame.k0 = K0_LUT_[bitstream.get_bits(5) as usize];
                    frame.k1 = K1_LUT_[bitstream.get_bits(5) as usize];
                    frame.k2 = K2_LUT_[bitstream.get_bits(4) as usize];
                    frame.k3 = K3_LUT_[bitstream.get_bits(4) as usize];
                    if frame.period != 0 {
                        frame.k4 = K4_LUT_[bitstream.get_bits(4) as usize];
                        frame.k5 = K5_LUT_[bitstream.get_bits(4) as usize];
                        frame.k6 = K6_LUT_[bitstream.get_bits(4) as usize];
                        frame.k7 = K7_LUT_[bitstream.get_bits(3) as usize];
                        frame.k8 = K8_LUT_[bitstream.get_bits(3) as usize];
                        frame.k9 = K9_LUT_[bitstream.get_bits(3) as usize];
                    }
                }
            }
            // Plaits' banks hold at most 926 frames of the 1 024.
            if let Some(slot) = frames.get_mut(self.num_frames_ as usize) {
                *slot = frame;
            }
            self.num_frames_ += 1;
        }
        bitstream.ptr() - p
    }

    /// `Load(bank)`.
    pub fn load(&mut self, bank: i32, arena: &mut Arena) -> bool {
        if bank == self.loaded_bank_ || bank >= self.num_banks_ {
            return false;
        }

        self.num_frames_ = 0;
        self.num_words_ = 0;

        let bank_data = self.word_banks_[bank as usize];
        let data = bank_data.data;
        let mut p = 0usize;
        let mut size = bank_data.size;

        while size != 0 {
            // Plaits' banks hold at most 26 words of the 32.
            let num_frames = self.num_frames_;
            if let Some(slot) = arena
                .view_mut::<i32>(self.word_boundaries_, K_LPC_SPEECH_SYNTH_MAX_WORDS)
                .get_mut(self.num_words_ as usize)
            {
                *slot = num_frames;
            }
            let consumed = self.load_next_word(data, p, arena);

            p += consumed;
            size = size.wrapping_sub(consumed);
            self.num_words_ += 1;
        }
        let num_frames = self.num_frames_;
        if let Some(slot) = arena
            .view_mut::<i32>(self.word_boundaries_, K_LPC_SPEECH_SYNTH_MAX_WORDS)
            .get_mut(self.num_words_ as usize)
        {
            *slot = num_frames;
        }
        self.loaded_bank_ = bank;
        true
    }
}

#[derive(Default)]
pub struct LPCSpeechSynthController {
    clock_phase_: f32,
    sample_: [f32; 2],
    next_sample_: [f32; 2],
    gain_: f32,
    synth_: LPCSpeechSynth,

    playback_frame_: i32,
    last_playback_frame_: i32,
    remaining_frame_samples_: usize,
}

impl LPCSpeechSynthController {
    /// `Init(word_bank)`: the word bank is the speech engine's, passed to `render`.
    pub fn init(&mut self) {
        self.clock_phase_ = 0.0;
        self.playback_frame_ = -1;
        self.last_playback_frame_ = -1;
        self.remaining_frame_samples_ = 0;

        self.sample_.fill(0.0);
        self.next_sample_.fill(0.0);

        self.gain_ = 0.0;

        self.synth_.init();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        free_running: bool,
        trigger: bool,
        bank: i32,
        frequency: f32,
        prosody_amount: f32,
        speed: f32,
        address: f32,
        formant_shift: f32,
        gain: f32,
        excitation: &mut [f32],
        output: &mut [f32],
        size: usize,
        word_bank: &mut LPCSpeechSynthWordBank,
        arena: &mut Arena,
        random: &mut Random,
    ) {
        let rate_ratio = semitones_to_ratio((formant_shift - 0.5) * 36.0);
        let rate = rate_ratio / 6.0;

        // All utterances have been normalized for an average f0 of 100 Hz.
        let pitch_shift =
            frequency / (rate_ratio * K_LPC_SPEECH_SYNTH_DEFAULT_F0 / K_CORRECTED_SAMPLE_RATE);
        let time_stretch = semitones_to_ratio(
            -speed * 24.0
                + (if formant_shift < 0.4 {
                    (formant_shift - 0.4) * -45.0
                } else if formant_shift > 0.6 {
                    (formant_shift - 0.6) * -45.0
                } else {
                    0.0
                }),
        );

        if bank != -1 {
            let reset_everything = word_bank.load(bank, arena);
            if reset_everything {
                self.playback_frame_ = -1;
                self.last_playback_frame_ = -1;
            }
        }

        let num_frames = if bank == -1 {
            K_LPC_SPEECH_SYNTH_NUM_VOWELS
        } else {
            word_bank.num_frames()
        };

        let frames: &[Frame] = if bank == -1 {
            &PHONEMES_
        } else {
            word_bank.frames(arena)
        };

        if trigger {
            if bank == -1 {
                // Pick a pseudo-random consonant, and play it for the duration of a
                // frame.
                let r = cast_i32((address + 3.0 * formant_shift + 7.0 * frequency) * 8.0);
                self.playback_frame_ = r.wrapping_rem(K_LPC_SPEECH_SYNTH_NUM_CONSONANTS);
                self.playback_frame_ += K_LPC_SPEECH_SYNTH_NUM_VOWELS;
                self.last_playback_frame_ = self.playback_frame_ + 1;
            } else {
                (self.playback_frame_, self.last_playback_frame_) =
                    word_bank.get_word_boundaries(address, arena);
            }
            self.remaining_frame_samples_ = 0;
        }

        if self.playback_frame_ == -1 && self.remaining_frame_samples_ == 0 {
            self.synth_
                .play_frame(frames, address * (num_frames as f32 - 1.0001), true);
        } else {
            if self.remaining_frame_samples_ == 0 {
                self.synth_
                    .play_frame(frames, self.playback_frame_ as f32, false);
                self.remaining_frame_samples_ =
                    cast_usize(K_SAMPLE_RATE / K_LPC_SPEECH_SYNTH_FPS * time_stretch);
                self.playback_frame_ = self.playback_frame_.wrapping_add(1);
                if self.playback_frame_ >= self.last_playback_frame_ {
                    let back_to_scan_mode = bank == -1 || free_running;
                    self.playback_frame_ = if back_to_scan_mode {
                        -1
                    } else {
                        self.last_playback_frame_
                    };
                }
            }
            self.remaining_frame_samples_ -= min(size, self.remaining_frame_samples_);
        }

        let mut gain_modulation = ParameterInterpolator::new(&mut self.gain_, gain, size);

        for (excitation, output) in excitation[..size].iter_mut().zip(&mut output[..size]) {
            let mut this_sample = self.next_sample_;
            self.next_sample_.fill(0.0);

            self.clock_phase_ += rate;
            if self.clock_phase_ >= 1.0 {
                self.clock_phase_ -= 1.0;
                let reset_time = self.clock_phase_ / rate;
                let mut new_sample = [0.0f32; 2];

                {
                    let (excitation, output) = new_sample.split_at_mut(1);
                    self.synth_
                        .render(prosody_amount, pitch_shift, excitation, output, 1, random);
                }

                let discontinuity = [
                    new_sample[0] - self.sample_[0],
                    new_sample[1] - self.sample_[1],
                ];
                this_sample[0] += discontinuity[0] * this_blep_sample(reset_time);
                self.next_sample_[0] += discontinuity[0] * next_blep_sample(reset_time);
                this_sample[1] += discontinuity[1] * this_blep_sample(reset_time);
                self.next_sample_[1] += discontinuity[1] * next_blep_sample(reset_time);
                self.sample_ = new_sample;
            }
            self.next_sample_[0] += self.sample_[0];
            self.next_sample_[1] += self.sample_[1];
            let gain = gain_modulation.next();
            *excitation = this_sample[0] * gain;
            *output = this_sample[1] * gain;
        }
    }
}
