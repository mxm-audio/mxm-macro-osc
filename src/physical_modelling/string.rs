//! Ported from plaits/dsp/physical_modelling/string.{h,cc}: comb filter / KS string. "Lite"
//! version of the implementation used in Rings.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The two delay lines' buffers are in the arena, as in the C++: `init` allocates them and keeps
//! their offsets, and `reset` and `process` take the arena. `ProcessInternal`'s template
//! parameter is a const generic. `process` is handed the voice's `Random`, which the dispersion
//! noise draws from.

use crate::cpp::min;
use crate::dsp::K_SAMPLE_RATE;
use crate::physical_modelling::delay_line::DelayLine;
use crate::resources::LUT_SVF_SHIFT;
use crate::stmlib::dsp::dsp::{cast_i64, crossfade, interpolate, one_pole};
use crate::stmlib::dsp::filter::{DCBlocker, FILTER_MODE_LOW_PASS, FREQUENCY_FAST, Svf};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

pub const K_DELAY_LINE_SIZE: usize = 1024;

pub type StringNonLinearity = u8;
pub const STRING_NON_LINEARITY_CURVED_BRIDGE: StringNonLinearity = 0;
pub const STRING_NON_LINEARITY_DISPERSION: StringNonLinearity = 1;

#[derive(Default)]
pub struct String {
    string_: DelayLine<K_DELAY_LINE_SIZE>,
    stretch_: DelayLine<{ K_DELAY_LINE_SIZE / 4 }>,
    /// Arena offset of `string_`'s `kDelayLineSize` floats.
    string_line_: usize,
    /// Arena offset of `stretch_`'s `kDelayLineSize / 4` floats.
    stretch_line_: usize,

    iir_damping_filter_: Svf,
    dc_blocker_: DCBlocker,

    delay_: f32,
    dispersion_noise_: f32,
    curved_bridge_: f32,

    // Very crappy linear interpolation upsampler used for low pitches that
    // do not fit the delay line. Rarely used.
    src_phase_: f32,
    out_sample_: [f32; 2],
}

impl String {
    pub fn init(&mut self, allocator: &mut BufferAllocator<'_>) {
        self.string_line_ = allocator
            .allocate::<f32>(K_DELAY_LINE_SIZE)
            .expect("the arena holds the string's delay line");
        self.string_.init(
            allocator
                .arena_mut()
                .view_mut::<f32>(self.string_line_, K_DELAY_LINE_SIZE),
        );
        self.stretch_line_ = allocator
            .allocate::<f32>(K_DELAY_LINE_SIZE / 4)
            .expect("the arena holds the string's stretch delay line");
        self.stretch_.init(
            allocator
                .arena_mut()
                .view_mut::<f32>(self.stretch_line_, K_DELAY_LINE_SIZE / 4),
        );
        self.delay_ = 100.0;
        self.reset(allocator.arena_mut());
    }

    pub fn reset(&mut self, arena: &mut Arena) {
        self.string_
            .reset(arena.view_mut::<f32>(self.string_line_, K_DELAY_LINE_SIZE));
        self.stretch_
            .reset(arena.view_mut::<f32>(self.stretch_line_, K_DELAY_LINE_SIZE / 4));
        self.iir_damping_filter_.init();
        self.dc_blocker_.init(1.0 - 20.0 / K_SAMPLE_RATE);
        self.dispersion_noise_ = 0.0;
        self.curved_bridge_ = 0.0;
        self.out_sample_[1] = 0.0;
        self.out_sample_[0] = self.out_sample_[1];
        self.src_phase_ = 0.0;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        f0: f32,
        non_linearity_amount: f32,
        brightness: f32,
        damping: f32,
        in_: &[f32],
        out: &mut [f32],
        size: usize,
        arena: &mut Arena,
        random: &mut Random,
    ) {
        if non_linearity_amount <= 0.0 {
            self.process_internal::<STRING_NON_LINEARITY_CURVED_BRIDGE>(
                f0,
                -non_linearity_amount,
                brightness,
                damping,
                in_,
                out,
                size,
                arena,
                random,
            );
        } else {
            self.process_internal::<STRING_NON_LINEARITY_DISPERSION>(
                f0,
                non_linearity_amount,
                brightness,
                damping,
                in_,
                out,
                size,
                arena,
                random,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn process_internal<const NON_LINEARITY: StringNonLinearity>(
        &mut self,
        f0: f32,
        non_linearity_amount: f32,
        mut brightness: f32,
        damping: f32,
        in_: &[f32],
        out: &mut [f32],
        size: usize,
        arena: &mut Arena,
        random: &mut Random,
    ) {
        let mut delay = 1.0 / f0;
        constrain(&mut delay, 4.0, K_DELAY_LINE_SIZE as f32 - 4.0);

        // If there is not enough delay time in the delay line, we play at the
        // lowest possible note and we upsample on the fly with a shitty linear
        // interpolator. We don't care because it's a corner case (f0 < 11.7Hz)
        let mut src_ratio = delay * f0;
        if src_ratio >= 0.9999 {
            // When we are above 11.7 Hz, we make sure that the linear interpolator
            // does not get in the way.
            self.src_phase_ = 1.0;
            src_ratio = 1.0;
        }

        let mut damping_cutoff = min(12.0 + damping * damping * 60.0 + brightness * 24.0, 84.0);
        let mut damping_f = min(f0 * semitones_to_ratio(damping_cutoff), 0.499);

        // Crossfade to infinite decay.
        if damping >= 0.95 {
            let to_infinite = 20.0 * (damping - 0.95);
            brightness += to_infinite * (1.0 - brightness);
            damping_f += to_infinite * (0.4999 - damping_f);
            damping_cutoff += to_infinite * (128.0 - damping_cutoff);
        }

        self.iir_damping_filter_
            .set_f_q::<FREQUENCY_FAST>(damping_f, 0.5);

        let damping_compensation = interpolate(&LUT_SVF_SHIFT, damping_cutoff, 1.0);

        // Linearly interpolate delay time.
        let mut delay_modulation =
            ParameterInterpolator::new(&mut self.delay_, delay * damping_compensation, size);

        let stretch_point = non_linearity_amount * (2.0 - non_linearity_amount) * 0.225;
        let mut stretch_correction = (160.0 / K_SAMPLE_RATE) * delay;
        constrain(&mut stretch_correction, 1.0, 2.1);

        let noise_amount_sqrt = if non_linearity_amount > 0.75 {
            4.0 * (non_linearity_amount - 0.75)
        } else {
            0.0
        };
        let noise_amount = noise_amount_sqrt * noise_amount_sqrt * 0.1;
        let noise_filter = 0.06 + 0.94 * brightness * brightness;

        let bridge_curving_sqrt = non_linearity_amount;
        let bridge_curving = bridge_curving_sqrt * bridge_curving_sqrt * 0.01;

        let ap_gain = -0.618 * non_linearity_amount / (0.15 + non_linearity_amount.abs());

        let (string_line, stretch_line) = arena.view_mut2::<f32, f32>(
            self.string_line_,
            K_DELAY_LINE_SIZE,
            self.stretch_line_,
            K_DELAY_LINE_SIZE / 4,
        );

        for (x, y) in in_[..size].iter().zip(&mut out[..size]) {
            self.src_phase_ += src_ratio;
            if self.src_phase_ > 1.0 {
                self.src_phase_ -= 1.0;

                let mut delay = delay_modulation.next();
                let mut s;

                if NON_LINEARITY == STRING_NON_LINEARITY_DISPERSION {
                    let noise = random.get_float() - 0.5;
                    one_pole(&mut self.dispersion_noise_, noise, noise_filter);
                    delay *= 1.0 + self.dispersion_noise_ * noise_amount;
                } else {
                    delay *= 1.0 - self.curved_bridge_ * bridge_curving;
                }

                if NON_LINEARITY == STRING_NON_LINEARITY_DISPERSION {
                    let ap_delay = delay * stretch_point;
                    let main_delay =
                        delay - ap_delay * (0.408 - stretch_point * 0.308) * stretch_correction;
                    if ap_delay >= 4.0 && main_delay >= 4.0 {
                        s = self.string_.read(string_line, main_delay);
                        // `Allpass` takes its delay as a `size_t`: `ap_delay` is at least 4
                        // here and far below 2⁶³, where MSVC's conversion to `size_t` is the
                        // 64-bit `cvttss2si`.
                        s = self.stretch_.allpass(
                            stretch_line,
                            s,
                            cast_i64(ap_delay) as usize,
                            ap_gain,
                        );
                    } else {
                        s = self.string_.read_hermite(string_line, delay);
                    }
                } else {
                    s = self.string_.read_hermite(string_line, delay);
                }

                if NON_LINEARITY == STRING_NON_LINEARITY_CURVED_BRIDGE {
                    let value = s.abs() - 0.025;
                    let sign = if s > 0.0 { 1.0 } else { -1.5 };
                    self.curved_bridge_ = (value.abs() + value) * sign;
                }

                s += *x;
                constrain(&mut s, -20.0, 20.0);

                self.dc_blocker_.process(std::slice::from_mut(&mut s), 1);
                s = self.iir_damping_filter_.process::<FILTER_MODE_LOW_PASS>(s);
                self.string_.write(string_line, s);

                self.out_sample_[1] = self.out_sample_[0];
                self.out_sample_[0] = s;
            }
            *y += crossfade(self.out_sample_[1], self.out_sample_[0], self.src_phase_);
        }
    }
}
