//! Ported from plaits/dsp/fm/operator.h: FM operator.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `RenderOperators<n, modulation_source, additive>` is `render_operators::<N, …>`. Its
//! `modulation` and `out` pointers point into the voice's buffers, and may be the same buffer
//! (an operator reading buffer 1 and writing it, `MOD(1) | OUT(1)`), so both are offsets into
//! the one `buffer` slice, read and written sample by sample in the C++'s order.

use crate::cpp::min;
use crate::oscillator::sine_oscillator::sine_pm;
use crate::stmlib::dsp::dsp::cast_u32;

pub const MODULATION_SOURCE_EXTERNAL: i32 = -2;
pub const MODULATION_SOURCE_NONE: i32 = -1;
pub const MODULATION_SOURCE_FEEDBACK: i32 = 0;

#[derive(Clone, Copy, Default, Debug)]
pub struct Operator {
    pub phase: u32,
    pub amplitude: f32,
}

impl Operator {
    #[inline]
    pub fn reset(&mut self) {
        self.phase = 0;
        self.amplitude = 0.0;
    }
}

/// `RenderFn(ops, f, a, fb_state, fb_amount, modulation, out, size)`: `modulation` and `out` are
/// offsets into `buffer`.
pub type RenderFn = fn(
    ops: &mut [Operator],
    f: &[f32],
    a: &[f32],
    fb_state: &mut [f32; 2],
    fb_amount: i32,
    buffer: &mut [f32],
    modulation: usize,
    out: usize,
    size: usize,
);

#[allow(clippy::too_many_arguments)]
pub fn render_operators<const N: usize, const MODULATION_SOURCE: i32, const ADDITIVE: bool>(
    ops: &mut [Operator],
    f: &[f32],
    a: &[f32],
    fb_state: &mut [f32; 2],
    fb_amount: i32,
    buffer: &mut [f32],
    mut modulation: usize,
    mut out: usize,
    mut size: usize,
) {
    // Uninitialised in the C++ unless the source is feedback, and then never read.
    let mut previous_0 = 0.0f32;
    let mut previous_1 = 0.0f32;

    if MODULATION_SOURCE >= MODULATION_SOURCE_FEEDBACK {
        previous_0 = fb_state[0];
        previous_1 = fb_state[1];
    }

    let mut frequency = [0u32; N];
    let mut phase = [0u32; N];
    let mut amplitude = [0.0f32; N];
    let mut amplitude_increment = [0.0f32; N];

    let scale = 1.0 / size as f32;
    for i in 0..N {
        frequency[i] = cast_u32(min(f[i], 0.5) * 4294967296.0);
        phase[i] = ops[i].phase;
        amplitude[i] = ops[i].amplitude;
        amplitude_increment[i] = (min(a[i], 4.0) - amplitude[i]) * scale;
    }

    let fb_scale = if fb_amount != 0 {
        (1i32 << fb_amount) as f32 / 512.0
    } else {
        0.0
    };

    while size > 0 {
        size -= 1;
        let mut pm = 0.0f32;
        if MODULATION_SOURCE >= MODULATION_SOURCE_FEEDBACK {
            pm = (previous_0 + previous_1) * fb_scale;
        } else if MODULATION_SOURCE == MODULATION_SOURCE_EXTERNAL {
            pm = buffer[modulation];
            modulation += 1;
        }
        for i in 0..N {
            phase[i] = phase[i].wrapping_add(frequency[i]);
            pm = sine_pm(phase[i], pm) * amplitude[i];
            amplitude[i] += amplitude_increment[i];
            if i as i32 == MODULATION_SOURCE {
                previous_1 = previous_0;
                previous_0 = pm;
            }
        }
        if ADDITIVE {
            buffer[out] += pm;
        } else {
            buffer[out] = pm;
        }
        out += 1;
    }

    for i in 0..N {
        ops[i].phase = phase[i];
        ops[i].amplitude = amplitude[i];
    }

    if MODULATION_SOURCE >= MODULATION_SOURCE_FEEDBACK {
        fb_state[0] = previous_0;
        fb_state[1] = previous_1;
    }
}
