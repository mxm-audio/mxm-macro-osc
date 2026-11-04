//! Ported from plaits/dsp/fm/patch.h: DX7 patch.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The six-op engine keeps its 32 patches in the arena (`Allocate<fm::Patch>(32)`), so `Patch`
//! and the types inside it are `#[repr(C)]` and `Pod`: all bytes, 156 a patch with no padding, as
//! on x64. The C++'s nested types (`Patch::Envelope`, `Patch::KeyboardScaling`,
//! `Patch::Operator`, `Patch::ModulationParameters`) are this module's, under the same names.

use bytemuck::{Pod, Zeroable};

use crate::cpp::min;

// The layout conveniently matches the 156 bytes SysEx format.

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct Envelope {
    pub rate: [u8; 4],
    pub level: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct KeyboardScaling {
    pub break_point: u8,
    pub left_depth: u8,
    pub right_depth: u8,
    pub left_curve: u8,
    pub right_curve: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct Operator {
    pub envelope: Envelope,
    pub keyboard_scaling: KeyboardScaling,

    pub rate_scaling: u8,
    pub amp_mod_sensitivity: u8,
    pub velocity_sensitivity: u8,
    pub level: u8,

    pub mode: u8,
    pub coarse: u8,
    /// x frequency by 1 + 0.01 x fine
    pub fine: u8,
    pub detune: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct ModulationParameters {
    pub rate: u8,
    pub delay: u8,
    pub pitch_mod_depth: u8,
    pub amp_mod_depth: u8,
    pub reset_phase: u8,
    pub waveform: u8,
    pub pitch_mod_sensitivity: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct Patch {
    pub op: [Operator; 6],

    pub pitch_envelope: Envelope,

    pub algorithm: u8,
    pub feedback: u8,

    pub reset_phase: u8,

    pub modulations: ModulationParameters,

    pub transpose: u8,
    pub name: [u8; 10],
    pub active_operators: u8,
}

const _: () = assert!(size_of::<Operator>() == 21, "Patch::Operator's x64 layout");
const _: () = assert!(size_of::<Patch>() == 156, "Patch's x64 layout");

/// `std::min(int, int)`, stored in a `uint8_t`.
#[inline]
fn min_int(value: u8, limit: i32) -> u8 {
    min(i32::from(value), limit) as u8
}

impl Patch {
    pub const SYX_SIZE: usize = 128;

    #[inline]
    pub fn bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }

    /// `Unpack(data)`: `data` holds at least `SYX_SIZE` bytes, one packed SysEx voice.
    #[inline]
    pub fn unpack(&mut self, data: &[u8]) {
        for i in 0..6 {
            let o = &mut self.op[i];
            let op_data = &data[i * 17..];
            for j in 0..4 {
                o.envelope.rate[j] = min_int(op_data[j] & 0x7f, 99);
                o.envelope.level[j] = min_int(op_data[4 + j] & 0x7f, 99);
            }
            o.keyboard_scaling.break_point = min_int(op_data[8] & 0x7f, 99);
            o.keyboard_scaling.left_depth = min_int(op_data[9] & 0x7f, 99);
            o.keyboard_scaling.right_depth = min_int(op_data[10] & 0x7f, 99);
            o.keyboard_scaling.left_curve = op_data[11] & 0x3;
            o.keyboard_scaling.right_curve = (op_data[11] >> 2) & 0x3;

            o.rate_scaling = op_data[12] & 0x7;
            o.amp_mod_sensitivity = op_data[13] & 0x3;
            o.velocity_sensitivity = (op_data[13] >> 2) & 0x7;
            o.level = min_int(op_data[14] & 0x7f, 99);
            o.mode = op_data[15] & 0x1;
            o.coarse = (op_data[15] >> 1) & 0x1f;
            o.fine = min_int(op_data[16] & 0x7f, 99);
            o.detune = min_int((op_data[12] >> 3) & 0xf, 14);
        }

        for j in 0..4 {
            self.pitch_envelope.rate[j] = min_int(data[102 + j] & 0x7f, 99);
            self.pitch_envelope.level[j] = min_int(data[106 + j] & 0x7f, 99);
        }

        self.algorithm = data[110] & 0x1f;
        self.feedback = data[111] & 0x7;
        self.reset_phase = (data[111] >> 3) & 0x1;

        self.modulations.rate = min_int(data[112] & 0x7f, 99);
        self.modulations.delay = min_int(data[113] & 0x7f, 99);
        self.modulations.pitch_mod_depth = min_int(data[114] & 0x7f, 99);
        self.modulations.amp_mod_depth = min_int(data[115] & 0x7f, 99);
        self.modulations.reset_phase = data[116] & 0x1;
        self.modulations.waveform = min_int((data[116] >> 1) & 0x7, 5);
        self.modulations.pitch_mod_sensitivity = data[116] >> 4;

        self.transpose = min_int(data[117] & 0x7f, 48);

        for i in 0..self.name.len() {
            self.name[i] = data[118 + i] & 0x7f;
        }
        self.active_operators = 0x3f;
    }
}
