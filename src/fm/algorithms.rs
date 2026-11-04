//! Ported from plaits/dsp/fm/algorithms.{h,cc}: FM algorithms and how to render them.
//!
//! Copyright 2021 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! Store information about all FM algorithms, and which functions to call
//! to render them.
//!
//! The raw structure of each algorithm is stored as a sequence of "opcodes",
//! where each opcode indicates, for each operator, from where does it get
//! its phase modulation signal and to which buffer it writes the result.
//! This data is compact - 1 byte / algorithm / operator.
//!
//! At run time, this data is "compiled" into a sequence of function calls
//! to pre-compiled renderers. A renderer is specialized to efficiently render
//! (without any branching, and as much loop unrolling as possible) one
//! operator or a group of operators.
//!
//! Different code space and speed trade-off can be obtained by increasing the
//! palette of available renderers (for example by specializing the code for
//! a renderer rendering in a single pass a "tower" of 4 operators).
//!
//! The template is ported for the six operators Plaits instantiates (`Algorithms<6>`, the
//! DX7's 32 algorithms): nothing instantiates `Algorithms<4>`, the DX100's, so its table is
//! not ported. The specialization declarations in `algorithms.h` (which MSVC needed removed for
//! the reference build) declare what `algorithms.cc` defines; the definitions are these.

use super::operator::{RenderFn, render_operators};

pub const NUM_OPERATORS: usize = 6;

/// `NumAlgorithms<6>::N`.
pub const NUM_ALGORITHMS: usize = 32;

// enum OpcodeFlags
pub const DESTINATION_MASK: u8 = 0x03;
pub const SOURCE_MASK: u8 = 0x30;
pub const SOURCE_FEEDBACK: u8 = 0x30;
pub const ADDITIVE_FLAG: u8 = 0x04;
pub const FEEDBACK_SOURCE_FLAG: u8 = 0x40;

#[derive(Clone, Copy, Default)]
pub struct RenderCall {
    /// `NULL` until `Compile` sets it; every operator a chain starts at has one.
    pub render_fn: Option<RenderFn>,
    pub n: i32,
    pub input_index: i32,
    pub output_index: i32,
}

struct RendererSpecs {
    n: i32,
    modulation_source: i32,
    additive: bool,
    render_fn: Option<RenderFn>,
}

pub struct Algorithms {
    render_call_: [[RenderCall; NUM_OPERATORS]; NUM_ALGORITHMS],
}

impl Default for Algorithms {
    fn default() -> Self {
        Self {
            render_call_: [[RenderCall::default(); NUM_OPERATORS]; NUM_ALGORITHMS],
        }
    }
}

// The opcode macros of algorithms.cc.
const fn mod_(n: u8) -> u8 {
    n << 4
}
const fn add(n: u8) -> u8 {
    n | ADDITIVE_FLAG
}
const fn out(n: u8) -> u8 {
    n
}
const FB_SRC: u8 = FEEDBACK_SOURCE_FLAG;
const FB_DST: u8 = mod_(3);
const FB: u8 = FB_SRC | FB_DST;

// Syntactic sugar
const NO_MOD: u8 = mod_(0);

// This can be replaced by OUT(0) for code that does not mix into the output buffer.
const OUTPUT: u8 = add(0);

/// `Algorithms<6>::opcodes_`, from the DX7: each row from operator 6 down to operator 1.
const OPCODES_: [[u8; NUM_OPERATORS]; NUM_ALGORITHMS] = [
    [
        // Algorithm 1
        FB | out(1),      // Op 6
        mod_(1) | out(1), // Op 5
        mod_(1) | out(1), // Op 4
        mod_(1) | OUTPUT, // Op 3
        NO_MOD | out(1),  // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 2
        NO_MOD | out(1),  // Op 6
        mod_(1) | out(1), // Op 5
        mod_(1) | out(1), // Op 4
        mod_(1) | OUTPUT, // Op 3
        FB | out(1),      // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 3
        FB | out(1),      // Op 6
        mod_(1) | out(1), // Op 5
        mod_(1) | OUTPUT, // Op 4
        NO_MOD | out(1),  // Op 3
        mod_(1) | out(1), // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 4
        FB_DST | NO_MOD | out(1),  // Op 6
        mod_(1) | out(1),          // Op 5
        FB_SRC | mod_(1) | OUTPUT, // Op 4
        NO_MOD | out(1),           // Op 3
        mod_(1) | out(1),          // Op 2
        mod_(1) | add(0),          // Op 1
    ],
    [
        // Algorithm 5
        FB | out(1),      // Op 6
        mod_(1) | OUTPUT, // Op 5
        NO_MOD | out(1),  // Op 4
        mod_(1) | add(0), // Op 3
        NO_MOD | out(1),  // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 6
        FB_DST | NO_MOD | out(1),  // Op 6
        FB_SRC | mod_(1) | OUTPUT, // Op 5
        NO_MOD | out(1),           // Op 4
        mod_(1) | add(0),          // Op 3
        NO_MOD | out(1),           // Op 2
        mod_(1) | add(0),          // Op 1
    ],
    [
        // Algorithm 7
        FB | out(1),      // Op 6
        mod_(1) | out(1), // Op 5
        NO_MOD | add(1),  // Op 4
        mod_(1) | OUTPUT, // Op 3
        NO_MOD | out(1),  // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 8
        NO_MOD | out(1),  // Op 6
        mod_(1) | out(1), // Op 5
        FB | add(1),      // Op 4
        mod_(1) | OUTPUT, // Op 3
        NO_MOD | out(1),  // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 9
        NO_MOD | out(1),  // Op 6
        mod_(1) | out(1), // Op 5
        NO_MOD | add(1),  // Op 4
        mod_(1) | OUTPUT, // Op 3
        FB | out(1),      // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 10
        NO_MOD | out(1),  // Op 6
        NO_MOD | add(1),  // Op 5
        mod_(1) | OUTPUT, // Op 4
        FB | out(1),      // Op 3
        mod_(1) | out(1), // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 11
        FB | out(1),      // Op 6
        NO_MOD | add(1),  // Op 5
        mod_(1) | OUTPUT, // Op 4
        NO_MOD | out(1),  // Op 3
        mod_(1) | out(1), // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 12
        NO_MOD | out(1),  // Op 6
        NO_MOD | add(1),  // Op 5
        NO_MOD | add(1),  // Op 4
        mod_(1) | OUTPUT, // Op 3
        FB | out(1),      // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 13
        FB | out(1),      // Op 6
        NO_MOD | add(1),  // Op 5
        NO_MOD | add(1),  // Op 4
        mod_(1) | OUTPUT, // Op 3
        NO_MOD | out(1),  // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 14
        FB | out(1),      // Op 6
        NO_MOD | add(1),  // Op 5
        mod_(1) | out(1), // Op 4
        mod_(1) | OUTPUT, // Op 3
        NO_MOD | out(1),  // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 15
        NO_MOD | out(1),  // Op 6
        NO_MOD | add(1),  // Op 5
        mod_(1) | out(1), // Op 4
        mod_(1) | OUTPUT, // Op 3
        FB | out(1),      // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 16
        FB | out(1),      // Op 6
        mod_(1) | out(1), // Op 5
        NO_MOD | out(2),  // Op 4
        mod_(2) | add(1), // Op 3
        NO_MOD | add(1),  // Op 2
        mod_(1) | OUTPUT, // Op 1
    ],
    [
        // Algorithm 17
        NO_MOD | out(1),  // Op 6
        mod_(1) | out(1), // Op 5
        NO_MOD | out(2),  // Op 4
        mod_(2) | add(1), // Op 3
        FB | add(1),      // Op 2
        mod_(1) | OUTPUT, // Op 1
    ],
    [
        // Algorithm 18
        NO_MOD | out(1),  // Op 6
        mod_(1) | out(1), // Op 5
        mod_(1) | out(1), // Op 4
        FB | add(1),      // Op 3
        NO_MOD | add(1),  // Op 2
        mod_(1) | OUTPUT, // Op 1
    ],
    [
        // Algorithm 19
        FB | out(1),      // Op 6
        mod_(1) | OUTPUT, // Op 5
        mod_(1) | add(0), // Op 4
        NO_MOD | out(1),  // Op 3
        mod_(1) | out(1), // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 20
        NO_MOD | out(1),  // Op 6
        NO_MOD | add(1),  // Op 5
        mod_(1) | OUTPUT, // Op 4
        FB | out(1),      // Op 3
        mod_(1) | add(0), // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 21
        NO_MOD | out(1),  // Op 6
        mod_(1) | OUTPUT, // Op 5
        mod_(1) | add(0), // Op 4
        FB | out(1),      // Op 3
        mod_(1) | add(0), // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 22
        FB | out(1),      // Op 6
        mod_(1) | OUTPUT, // Op 5
        mod_(1) | add(0), // Op 4
        mod_(1) | add(0), // Op 3
        NO_MOD | out(1),  // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 23
        FB | out(1),      // Op 6
        mod_(1) | OUTPUT, // Op 5
        mod_(1) | add(0), // Op 4
        NO_MOD | out(1),  // Op 3
        mod_(1) | add(0), // Op 2
        NO_MOD | add(0),  // Op 1
    ],
    [
        // Algorithm 24
        FB | out(1),      // Op 6
        mod_(1) | OUTPUT, // Op 5
        mod_(1) | add(0), // Op 4
        mod_(1) | add(0), // Op 3
        NO_MOD | add(0),  // Op 2
        NO_MOD | add(0),  // Op 1
    ],
    [
        // Algorithm 25
        FB | out(1),      // Op 6
        mod_(1) | OUTPUT, // Op 5
        mod_(1) | add(0), // Op 4
        NO_MOD | add(0),  // Op 3
        NO_MOD | add(0),  // Op 2
        NO_MOD | add(0),  // Op 1
    ],
    [
        // Algorithm 26
        FB | out(1),      // Op 6
        NO_MOD | add(1),  // Op 5
        mod_(1) | OUTPUT, // Op 4
        NO_MOD | out(1),  // Op 3
        mod_(1) | add(0), // Op 2
        NO_MOD | add(0),  // Op 1
    ],
    [
        // Algorithm 27
        NO_MOD | out(1),  // Op 6
        NO_MOD | add(1),  // Op 5
        mod_(1) | OUTPUT, // Op 4
        FB | out(1),      // Op 3
        mod_(1) | add(0), // Op 2
        NO_MOD | add(0),  // Op 1
    ],
    [
        // Algorithm 28
        NO_MOD | OUTPUT,  // Op 6
        FB | out(1),      // Op 5
        mod_(1) | out(1), // Op 4
        mod_(1) | add(0), // Op 3
        NO_MOD | out(1),  // Op 2
        mod_(1) | add(0), // Op 1
    ],
    [
        // Algorithm 29
        FB | out(1),      // Op 6
        mod_(1) | OUTPUT, // Op 5
        NO_MOD | out(1),  // Op 4
        mod_(1) | add(0), // Op 3
        NO_MOD | add(0),  // Op 2
        NO_MOD | add(0),  // Op 1
    ],
    [
        // Algorithm 30
        NO_MOD | OUTPUT,  // Op 6
        FB | out(1),      // Op 5
        mod_(1) | out(1), // Op 4
        mod_(1) | add(0), // Op 3
        NO_MOD | add(0),  // Op 2
        NO_MOD | add(0),  // Op 1
    ],
    [
        // Algorithm 31
        FB | out(1),      // Op 6
        mod_(1) | OUTPUT, // Op 5
        NO_MOD | add(0),  // Op 4
        NO_MOD | add(0),  // Op 3
        NO_MOD | add(0),  // Op 2
        NO_MOD | add(0),  // Op 1
    ],
    [
        // Algorithm 32
        FB | OUTPUT,     // Op 6
        NO_MOD | add(0), // Op 5
        NO_MOD | add(0), // Op 4
        NO_MOD | add(0), // Op 3
        NO_MOD | add(0), // Op 2
        NO_MOD | add(0), // Op 1
    ],
];

/// `INSTANTIATE_RENDERER(n, m, a)`.
macro_rules! instantiate_renderer {
    ($n:literal, $m:literal, $a:literal) => {
        RendererSpecs {
            n: $n,
            modulation_source: $m,
            additive: $a,
            render_fn: Some(render_operators::<$n, { $m }, $a>),
        }
    };
}

/// `Algorithms<6>::renderers_`.
static RENDERERS_: [RendererSpecs; 9] = [
    // Core
    instantiate_renderer!(1, -2, false),
    instantiate_renderer!(1, -2, true),
    instantiate_renderer!(1, -1, false),
    instantiate_renderer!(1, -1, true),
    instantiate_renderer!(1, 0, false),
    instantiate_renderer!(1, 0, true),
    // Pesky feedback loops spanning several operators
    instantiate_renderer!(3, 2, true),
    instantiate_renderer!(2, 1, true),
    // Optimized
    /*INSTANTIATE_RENDERER(4, -1, true),
    INSTANTIATE_RENDERER(4,  0, true),
    INSTANTIATE_RENDERER(3, -1, false),
    INSTANTIATE_RENDERER(3, -1, true),
    INSTANTIATE_RENDERER(3,  0, true),
    INSTANTIATE_RENDERER(2, -2, true),
    INSTANTIATE_RENDERER(2, -1, false),
    INSTANTIATE_RENDERER(2, -1, true),
    INSTANTIATE_RENDERER(2,  0, false),
    INSTANTIATE_RENDERER(2,  0, true),*/
    RendererSpecs {
        n: 0,
        modulation_source: 0,
        additive: false,
        render_fn: None,
    },
];

impl Algorithms {
    #[inline]
    pub fn init(&mut self) {
        for i in 0..NUM_ALGORITHMS {
            self.compile(i);
        }
    }

    /// `render_call(algorithm, op)`: `algorithm` is a patch's, five bits, so in range.
    #[inline]
    pub fn render_call(&self, algorithm: usize, op: usize) -> &RenderCall {
        &self.render_call_[algorithm][op]
    }

    #[inline]
    pub fn is_modulator(&self, algorithm: usize, op: usize) -> bool {
        OPCODES_[algorithm][op] & DESTINATION_MASK != 0
    }

    #[inline]
    fn get_renderer(n: i32, modulation_source: i32, additive: bool) -> Option<RenderFn> {
        for r in RENDERERS_.iter().take_while(|r| r.n != 0) {
            if r.n == n && r.modulation_source == modulation_source && r.additive == additive {
                return r.render_fn;
            }
        }
        None
    }

    #[inline]
    fn compile(&mut self, algorithm: usize) {
        let opcodes = &OPCODES_[algorithm];
        let mut i = 0usize;
        while i < NUM_OPERATORS {
            let opcode = opcodes[i];

            let mut n = 1usize;

            while i + n < NUM_OPERATORS {
                let from = opcodes[i + n - 1];
                let to = (opcodes[i + n] & SOURCE_MASK) >> 4;

                let has_additive = from & ADDITIVE_FLAG != 0;
                let broken = (from & DESTINATION_MASK) != to;

                if has_additive || broken {
                    if to == (opcode & DESTINATION_MASK) {
                        // If the same modulation happens to be reused by subsequent
                        // operators (algorithms 19 to 25), discard the chain.
                        n = 1;
                    }
                    break;
                }
                n += 1;
            }

            // Try to find if a pre-compiled renderer is available for this chain.
            for _attempt in 0..2 {
                let out_opcode = opcodes[i + n - 1];
                let additive = out_opcode & ADDITIVE_FLAG != 0;

                let mut modulation_source = -3;
                if opcode & SOURCE_MASK == 0 {
                    modulation_source = -1;
                } else if (opcode & SOURCE_MASK) != SOURCE_FEEDBACK {
                    modulation_source = -2;
                } else {
                    for j in 0..n {
                        if opcodes[i + j] & FEEDBACK_SOURCE_FLAG != 0 {
                            modulation_source = j as i32;
                        }
                    }
                }
                let fn_ = Self::get_renderer(n as i32, modulation_source, additive);
                if fn_.is_some() {
                    let call = &mut self.render_call_[algorithm][i];
                    call.render_fn = fn_;
                    call.n = n as i32;
                    call.input_index = i32::from((opcode & SOURCE_MASK) >> 4);
                    call.output_index = i32::from(out_opcode & DESTINATION_MASK);
                    break;
                } else if n == 1 {
                    // assert(false);
                } else {
                    n = 1;
                }
            }
            i += n;
        }
    }
}
