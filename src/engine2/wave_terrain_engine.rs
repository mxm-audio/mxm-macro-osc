//! Ported from plaits/dsp/engine2/wave_terrain_engine.{h,cc}: wave terrain synthesis - a 2D
//! function evaluated along an elliptical path of adjustable center and excentricity.
//!
//! This implementation initially used pre-computed terrains stored in flash
//! memory, but even at a poor resolution of 64x64 with 8-bit samples, this
//! takes 4kb per terrain! It turned out that directly evaluating the terrain
//! function on the fly uses less flash, but is also faster than bicubic
//! interpolation of the terrain data.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `DIFFERENTIATE_WAVE_DATA` is defined in the C++, the variant built; the other is left out.
//! `Terrain` reads no member but the user terrain, which is not ported, so it is an associated
//! function: the parameter interpolators borrow the engine's fields while it runs.
//!
//! A pointer into `wav_integrated_waves` is the offset, in samples, it points at, and every read
//! goes through [`wav_integrated_waves_at`]: once the path's y passes -1, a lookup reads a
//! sample before its wave's start, which in the C++ is the end of the wave before
//! (`e05-edges`).
//!
//! Not ported: the user terrain (`TerrainLookup`, terrain 8). The module's user data is not
//! ported, so `LoadUserData` always gets `NULL` here: `user_terrain_` stays `NULL`, there are
//! eight terrains, and terrain 8 is never reached.

use crate::cpp::{max, min, wav_integrated_waves_at};
use crate::dsp::K_MAX_BLOCK_SIZE;
use crate::engine::engine::{Engine, EngineParameters, PostProcessingSettings, note_to_frequency};
use crate::oscillator::sine_oscillator::{FastSineOscillator, sine};
use crate::stmlib::dsp::dsp::make_integral_fractional;
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct WaveTerrainEngine {
    pub post_processing_settings: PostProcessingSettings,

    path_: FastSineOscillator,
    offset_: f32,
    terrain_: f32,

    /// Arena offset of `kMaxBlockSize * 4` floats.
    temp_buffer_: usize,
}

/// `InterpolateIntegratedWave(table, …)`, `table` pointing at that offset into
/// `wav_integrated_waves`, the only table it is called with.
#[inline]
fn interpolate_integrated_wave(table: i32, index_integral: i32, index_fractional: f32) -> f32 {
    let a = wav_integrated_waves_at(table.wrapping_add(index_integral)) as f32;
    let b = wav_integrated_waves_at(table.wrapping_add(index_integral).wrapping_add(1)) as f32;
    let c = wav_integrated_waves_at(table.wrapping_add(index_integral).wrapping_add(2)) as f32;
    let t = index_fractional;
    (b - a) + (c - b - b + a) * t
}

// The wavetables are stored in integrated form. Either we directly use the
// integrated data (which can have large variations in amplitude), or we
// differentiate it on the fly to recover the original waveform. This second
// option can be noisier, and it would ideally need a low pass-filter.

/// Lookup from the wavetable data re-interpreted as a terrain. :facepalm:
#[inline]
fn terrain_lookup_wt(x: f32, y: f32, bank: i32) -> f32 {
    const TABLE_SIZE: i32 = 128;
    const TABLE_SIZE_FULL: i32 = TABLE_SIZE + 4; // Includes 4 wrapped samples
    const NUM_WAVES: i32 = 64;
    let sample = (y + 1.0) * 0.5 * TABLE_SIZE as f32;
    let wt = (x + 1.0) * 0.5 * (NUM_WAVES - 1) as f32;

    // `wav_integrated_waves + bank * num_waves * table_size_full`.
    let mut waves = bank * NUM_WAVES * TABLE_SIZE_FULL;

    let (sample_integral, sample_fractional) = make_integral_fractional(sample);
    let (wt_integral, wt_fractional) = make_integral_fractional(wt);

    let value_scale = 1.0f32 / 1024.0;
    waves = waves.wrapping_add(wt_integral.wrapping_mul(TABLE_SIZE_FULL));
    let xy0 = interpolate_integrated_wave(waves, sample_integral, sample_fractional);
    waves = waves.wrapping_add(TABLE_SIZE_FULL);
    let xy1 = interpolate_integrated_wave(waves, sample_integral, sample_fractional);
    (xy0 + (xy1 - xy0) * wt_fractional) * value_scale
}

#[inline]
fn squash(mut x: f32, a: f32) -> f32 {
    x *= a;
    x / (1.0 + x.abs())
}

impl WaveTerrainEngine {
    #[inline]
    fn terrain(x: f32, y: f32, terrain_index: i32) -> f32 {
        // The Sine function only works for a positive argument.
        // Thus, all calls to Sine include a positive offset of the argument!
        let k = 4.0f32;
        match terrain_index {
            0 => (squash(sine(k + x * 1.273), 2.0) - sine(k + y * (x + 1.571) * 0.637)) * 0.57,
            1 => {
                let xy = x * y;
                sine(k + sine(k + (x + y) * 0.637) / (0.2 + xy * xy) * 0.159)
            }
            2 => {
                let xy = x * y;
                sine(k + sine(k + 2.387 * xy) / (0.350 + xy * xy) * 0.159)
            }
            3 => {
                let xy = x * y;
                let xys = (x - 0.25) * (y + 0.25);
                sine(k + xy / (2.0 + (5.0 * xys).abs()) * 6.366)
            }
            4 => sine(
                0.159 / (0.170 + (y - 0.25).abs())
                    + 0.477 / (0.350 + ((x + 0.5) * (y + 1.5)).abs())
                    + k,
            ),
            5..=7 => terrain_lookup_wt(x, y, 2 - (terrain_index - 5)),
            // 8, `TerrainLookup(x, y, user_terrain_)`: not ported (the module doc).
            _ => 0.0,
        }
    }
}

impl Engine for WaveTerrainEngine {
    fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.path_.init();
        self.offset_ = 0.0;
        self.terrain_ = 0.0;
        self.temp_buffer_ = allocator
            .allocate::<f32>(K_MAX_BLOCK_SIZE * 4)
            .expect("the arena holds the engine's temp buffer");
    }

    fn reset(&mut self, _arena: &mut Arena) {}

    /// `user_terrain_ = (const int8_t*)(user_data)`: always `NULL` here (the module doc).
    fn load_user_data(&mut self, _user_data: Option<&'static [u8]>, _arena: &mut Arena) {}

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
        const K_OVERSAMPLING: usize = 2;
        let k_scale = 1.0 / K_OVERSAMPLING as f32;

        let temp_buffer = arena.view_mut::<f32>(self.temp_buffer_, K_MAX_BLOCK_SIZE * 4);
        let (path_x, path_y) = temp_buffer.split_at_mut(K_OVERSAMPLING * size);

        let f0 = note_to_frequency(parameters.note);
        let attenuation = max(1.0 - 8.0 * f0, 0.0);
        let radius = 0.1 + 0.9 * parameters.timbre * attenuation * (2.0 - attenuation);

        // Use the "magic sine" algorithm to generate sin and cos functions for the
        // trajectory coordinates.
        self.path_
            .render_quadrature(f0 * k_scale, radius, path_x, path_y, size * K_OVERSAMPLING);

        let mut offset =
            ParameterInterpolator::new(&mut self.offset_, 1.9 * parameters.morph - 1.0, size);
        // `user_terrain_ ? 9 : 8`, the user terrain always `NULL` here.
        let num_terrains: i32 = 8;
        let mut terrain = ParameterInterpolator::new(
            &mut self.terrain_,
            min(parameters.harmonics * 1.05, 1.0) * (num_terrains as f32 - 1.0001),
            size,
        );

        let mut ij = 0;
        for i in 0..size {
            let x_offset = offset.next();

            let z = terrain.next();
            let (z_integral, z_fractional) = make_integral_fractional(z);

            let mut out_s = 0.0f32;
            let mut aux_s = 0.0f32;

            for _ in 0..K_OVERSAMPLING {
                let x = path_x[ij] * (1.0 - x_offset.abs()) + x_offset;
                let y = path_y[ij];
                ij += 1;

                let z0 = Self::terrain(x, y, z_integral);
                let z1 = Self::terrain(x, y, z_integral.wrapping_add(1));
                let z = z0 + (z1 - z0) * z_fractional;
                out_s += z;
                aux_s += y + z;
            }
            out[i] = k_scale * out_s;
            aux[i] = sine(1.0 + 0.5 * k_scale * aux_s);
        }
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
