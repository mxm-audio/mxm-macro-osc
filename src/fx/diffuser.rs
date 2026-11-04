//! Ported from plaits/dsp/fx/diffuser.h: granular diffuser.
//!
//! Copyright 2016 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! Its delay memory is 8 192 `uint16_t` of the arena (12-bit samples): `init` takes the offset
//! the engine allocated, and `reset` and `process` the arena.

use super::fx_engine::{DelayLine, Format12Bit, FxEngine, LFO_1, TAIL, delay_line, fits};
use crate::stmlib::utils::buffer_allocator::Arena;

type E = FxEngine<8192, Format12Bit>;

/// `E::Reserve<126, E::Reserve<180, … E::Reserve<3411> > > > > > >`.
const MEMORY: [i32; 7] = [126, 180, 269, 444, 1653, 2010, 3411];
const AP1: DelayLine = delay_line(&MEMORY, 0);
const AP2: DelayLine = delay_line(&MEMORY, 1);
const AP3: DelayLine = delay_line(&MEMORY, 2);
const AP4: DelayLine = delay_line(&MEMORY, 3);
const DAPA: DelayLine = delay_line(&MEMORY, 4);
const DAPB: DelayLine = delay_line(&MEMORY, 5);
const DEL: DelayLine = delay_line(&MEMORY, 6);
const _: () = assert!(fits(DEL, 8192), "delay_memory_full");

#[derive(Default)]
pub struct Diffuser {
    engine_: E,
    lp_decay_: f32,
}

impl Diffuser {
    /// `Init(uint16_t* buffer)`: the arena offset of 8 192 `uint16_t`.
    pub fn init(&mut self, buffer: usize) {
        self.engine_.init(buffer);
        self.engine_.set_lfo_frequency(LFO_1, 0.3 / 48000.0);
        self.lp_decay_ = 0.0;
    }

    pub fn reset(&mut self, arena: &mut Arena) {
        let buffer = arena.view_mut::<u16>(self.engine_.buffer(), 8192);
        self.engine_.clear(buffer);
    }

    pub fn process(
        &mut self,
        amount: f32,
        rt: f32,
        in_out: &mut [f32],
        size: usize,
        arena: &mut Arena,
    ) {
        let buffer = arena.view_mut::<u16>(self.engine_.buffer(), 8192);
        let kap = 0.625f32;
        let klp = 0.75f32;
        let mut lp = self.lp_decay_;
        for sample in &mut in_out[..size] {
            let mut wet = 0.0f32;
            let mut c = self.engine_.start(buffer);
            c.read_value(*sample);
            c.read_offset(AP1, TAIL, kap);
            c.write_all_pass(AP1, -kap);
            c.read_offset(AP2, TAIL, kap);
            c.write_all_pass(AP2, -kap);
            c.read_offset(AP3, TAIL, kap);
            c.write_all_pass(AP3, -kap);
            c.interpolate_lfo(AP4, 400.0, LFO_1, 43.0, kap);
            c.write_all_pass(AP4, -kap);
            c.interpolate_lfo(DEL, 3070.0, LFO_1, 340.0, rt);
            c.lp(&mut lp, klp);
            c.read_offset(DAPA, TAIL, -kap);
            c.write_all_pass(DAPA, kap);
            c.read_offset(DAPB, TAIL, kap);
            c.write_all_pass(DAPB, -kap);
            c.write(DEL, 2.0);
            c.write_value_scaled(&mut wet, 0.0);
            *sample += amount * (wet - *sample);
        }
        self.lp_decay_ = lp;
    }
}
