//! Ported from plaits/dsp/engine/wavetable_engine.{h,cc}: 8x8x3 wave terrain.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The wave map, `const int16_t** wave_map_`, is in the arena as in the C++, each pointer an
//! 8-byte index: the offset, in samples, of the wave's first sample in `wav_integrated_waves`.
//! As in the C++, `Init` allocates 64 entries and `LoadUserData` writes all 256 (four banks), the
//! last 192 past the allocation, in arena bytes nothing else of this engine's holds.
//!
//! Not ported: the user data's fourth bank and custom waves (the module's user data is not
//! ported, so `LoadUserData` always gets `NULL` here and the fourth bank is the scramble).

use super::engine::{Engine, EngineParameters, PostProcessingSettings, note_to_frequency};
use crate::cpp::{max, min};
use crate::dsp::A0;
use crate::oscillator::wavetable_oscillator::{Differentiator, interpolate_wave_hermite};
use crate::resources::WAV_INTEGRATED_WAVES;
use crate::stmlib::dsp::dsp::{cast_i32, make_integral_fractional, one_pole};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator, K_ARENA_SIZE};
use crate::stmlib::utils::random::Random;

const K_NUM_BANKS: i32 = 4;
const K_NUM_WAVES_PER_BANK: i32 = 64;
const K_NUM_WAVES: i32 = 192;

const K_TABLE_SIZE: usize = 128;
const K_TABLE_SIZE_F: f32 = K_TABLE_SIZE as f32;

/// The size of one entry of the wave map, a pointer on x64.
const K_POINTER_SIZE: usize = 8;

#[derive(Default)]
pub struct WavetableEngine {
    pub post_processing_settings: PostProcessingSettings,

    phase_: f32,

    x_pre_lp_: f32,
    y_pre_lp_: f32,
    z_pre_lp_: f32,

    x_lp_: f32,
    y_lp_: f32,
    z_lp_: f32,

    previous_x_: f32,
    previous_y_: f32,
    previous_z_: f32,
    previous_f0_: f32,

    /// Maps a (bank, X, Y) coordinate to a waveform index.
    /// This allows all waveforms to be reshuffled by the user to create new maps.
    /// Arena offset of the map's `u64` entries.
    wave_map_: usize,

    diff_out_: Differentiator,
}

#[inline]
fn clamp(mut x: f32, amount: f32) -> f32 {
    x -= 0.5;
    x *= amount;
    constrain(&mut x, -0.5, 0.5);
    x += 0.5;
    x
}

impl WavetableEngine {
    /// `ReadWave(x, y, z, phase_integral, phase_fractional)`: reads no member but the map, so
    /// it takes the map's offset while the interpolators hold the engine's fields. An entry
    /// outside the arena, or an index outside the waves (only a NaN gets there, where the C++
    /// reads through a wild pointer), reads as silence rather than panic.
    #[inline]
    fn read_wave(
        wave_map: usize,
        arena: &Arena,
        x: i32,
        y: i32,
        z: i32,
        phase_integral: i32,
        phase_fractional: f32,
    ) -> f32 {
        let index = x
            .wrapping_add(y.wrapping_mul(8))
            .wrapping_add(z.wrapping_mul(K_NUM_WAVES_PER_BANK));
        let entry = isize::try_from(index)
            .ok()
            .and_then(|i| i.checked_mul(K_POINTER_SIZE as isize))
            .and_then(|b| b.checked_add_unsigned(wave_map))
            .and_then(|b| usize::try_from(b).ok())
            .filter(|&b| b + K_POINTER_SIZE <= K_ARENA_SIZE)
            .map(|b| {
                let mut bytes = [0u8; K_POINTER_SIZE];
                bytes.copy_from_slice(&arena.bytes[b..b + K_POINTER_SIZE]);
                u64::from_ne_bytes(bytes)
            });
        let wave = entry
            .and_then(|offset| usize::try_from(offset).ok())
            .and_then(|offset| WAV_INTEGRATED_WAVES.get(offset..))
            .unwrap_or(&[]);
        interpolate_wave_hermite(wave, phase_integral, phase_fractional)
    }
}

impl Engine for WavetableEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.phase_ = 0.0;

        self.x_lp_ = 0.0;
        self.y_lp_ = 0.0;
        self.z_lp_ = 0.0;

        self.x_pre_lp_ = 0.0;
        self.y_pre_lp_ = 0.0;
        self.z_pre_lp_ = 0.0;

        self.previous_x_ = 0.0;
        self.previous_y_ = 0.0;
        self.previous_z_ = 0.0;
        self.previous_f0_ = A0;

        self.diff_out_.init();

        self.wave_map_ = allocator
            .allocate::<u64>(K_NUM_WAVES_PER_BANK as usize)
            .expect("the arena holds the wave map");
    }

    fn reset(&mut self, _arena: &mut Arena) {}

    fn load_user_data(&mut self, _user_data: Option<&'static [u8]>, arena: &mut Arena) {
        // All four banks, past the 64 entries allocated (the module's doc comment).
        let wave_map = arena.view_mut::<u64>(
            self.wave_map_,
            (K_NUM_BANKS * K_NUM_WAVES_PER_BANK) as usize,
        );
        for bank in 0..K_NUM_BANKS {
            for wave in 0..K_NUM_WAVES_PER_BANK {
                let i = bank * K_NUM_WAVES_PER_BANK + wave;

                let mut w = i;
                if bank == K_NUM_BANKS - 1 {
                    w = w * 101 % K_NUM_WAVES;
                }

                wave_map[i as usize] = (w as usize * (K_TABLE_SIZE + 4)) as u64;
            }
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
        _random: &mut Random,
    ) {
        let f0 = note_to_frequency(parameters.note);

        one_pole(&mut self.x_pre_lp_, parameters.timbre * 6.9999, 0.2);
        one_pole(&mut self.y_pre_lp_, parameters.morph * 6.9999, 0.2);
        one_pole(&mut self.z_pre_lp_, parameters.harmonics * 6.9999, 0.05);

        let x = self.x_pre_lp_;
        let y = self.y_pre_lp_;
        let z = self.z_pre_lp_;

        let quantization = min(max(z - 3.0, 0.0), 1.0);
        let lp_coefficient = min(max(2.0 * f0 * (4.0 - 3.0 * quantization), 0.01), 0.1);

        let (x_integral, mut x_fractional) = make_integral_fractional(x);
        let (y_integral, mut y_fractional) = make_integral_fractional(y);
        let (z_integral, mut z_fractional) = make_integral_fractional(z);

        x_fractional += quantization * (clamp(x_fractional, 16.0) - x_fractional);
        y_fractional += quantization * (clamp(y_fractional, 16.0) - y_fractional);
        z_fractional += quantization * (clamp(z_fractional, 16.0) - z_fractional);

        let mut x_modulation = ParameterInterpolator::new(
            &mut self.previous_x_,
            x_integral as f32 + x_fractional,
            size,
        );
        let mut y_modulation = ParameterInterpolator::new(
            &mut self.previous_y_,
            y_integral as f32 + y_fractional,
            size,
        );
        let mut z_modulation = ParameterInterpolator::new(
            &mut self.previous_z_,
            z_integral as f32 + z_fractional,
            size,
        );

        let mut f0_modulation = ParameterInterpolator::new(&mut self.previous_f0_, f0, size);

        let wave_map = self.wave_map_;
        let arena: &Arena = arena;
        for (out, aux) in out[..size].iter_mut().zip(&mut aux[..size]) {
            let f0 = f0_modulation.next();

            let gain = (1.0 / (f0 * 131072.0)) * (0.95 - f0);
            let cutoff = min(K_TABLE_SIZE_F * f0, 1.0);

            one_pole(&mut self.x_lp_, x_modulation.next(), lp_coefficient);
            one_pole(&mut self.y_lp_, y_modulation.next(), lp_coefficient);
            one_pole(&mut self.z_lp_, z_modulation.next(), lp_coefficient);

            let x = self.x_lp_;
            let y = self.y_lp_;
            let z = self.z_lp_;

            let (x_integral, x_fractional) = make_integral_fractional(x);
            let (y_integral, y_fractional) = make_integral_fractional(y);
            let (z_integral, z_fractional) = make_integral_fractional(z);

            self.phase_ += f0;
            if self.phase_ >= 1.0 {
                self.phase_ -= 1.0;
            }

            let p = self.phase_ * K_TABLE_SIZE_F;
            let (p_integral, p_fractional) = make_integral_fractional(p);

            {
                let x0 = x_integral;
                let x1 = x_integral.wrapping_add(1);
                let y0 = y_integral;
                let y1 = y_integral.wrapping_add(1);
                let mut z0 = z_integral;
                let mut z1 = z_integral.wrapping_add(1);

                if z0 >= 4 {
                    z0 = 7 - z0;
                }
                if z1 >= 4 {
                    z1 = 7 - z1;
                }

                let read =
                    |x, y, z| Self::read_wave(wave_map, arena, x, y, z, p_integral, p_fractional);

                let x0y0z0 = read(x0, y0, z0);
                let x1y0z0 = read(x1, y0, z0);
                let xy0z0 = x0y0z0 + (x1y0z0 - x0y0z0) * x_fractional;

                let x0y1z0 = read(x0, y1, z0);
                let x1y1z0 = read(x1, y1, z0);
                let xy1z0 = x0y1z0 + (x1y1z0 - x0y1z0) * x_fractional;

                let xyz0 = xy0z0 + (xy1z0 - xy0z0) * y_fractional;

                let x0y0z1 = read(x0, y0, z1);
                let x1y0z1 = read(x1, y0, z1);
                let xy0z1 = x0y0z1 + (x1y0z1 - x0y0z1) * x_fractional;

                let x0y1z1 = read(x0, y1, z1);
                let x1y1z1 = read(x1, y1, z1);
                let xy1z1 = x0y1z1 + (x1y1z1 - x0y1z1) * x_fractional;

                let xyz1 = xy0z1 + (xy1z1 - xy0z1) * y_fractional;

                let mut mix = xyz0 + (xyz1 - xyz0) * z_fractional;
                mix = self.diff_out_.process(cutoff, mix) * gain;
                *out = mix;
                *aux = cast_i32(mix * 32.0) as f32 / 32.0;
            }
        }
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
