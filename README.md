# mxm-macro-osc

Plaits' voice and all twenty-four of its engines, ported to Rust from Emilie Gillet's MIT code for
Mutable Instruments Plaits, and **bit identical to the original**: every frame this crate renders
equals, to the bit, the frame Plaits' own C++ renders from the same input.

This is not "sounds the same" or "within a tolerance". The test suite drives the Rust voice and
the C++ voice through 220 reference cases, one second each, and compares every 16-bit sample of
both outputs, OUT and AUX, by SHA-256. Every case matches. `cargo test` checks all 220 of them
again.

## What bit identical means here

- **The original** is Plaits' DSP as Emilie Gillet wrote it:
  [`pichenettes/eurorack`](https://github.com/pichenettes/eurorack) `plaits/dsp/` at commit
  `f4cfa769`, with the parts of [`pichenettes/stmlib`](https://github.com/pichenettes/stmlib) it
  uses, at `e3bd7c9c`. It was built for x86-64 with MSVC (`/O2 /fp:precise`, flush-to-zero on)
  and run, untouched, by a harness that renders each case block by block through `Voice::Render`.
- **A frame** is what `Voice::Render` returns: OUT and AUX as 16-bit integers, after Plaits' own
  output stage. That is exactly what the module hands its DAC, with its quantisation, inversion
  and clipping, so the port is held to the module's output, not to an approximation of it.
- **The cases** cover every engine: pinged at four notes, notes held, each of Harmonics, Timbre
  and Morph swept from 0 to 1, a drone through the whole pitch range, Level patched, the decay
  envelope through each attenuverter, and every control at its extremes. Four more cross engines:
  a held note while the engine steps through all 24, the engine CV swept, FM patched and swept,
  and Timbre patched. [`tests/cases/README.md`](tests/cases/README.md) has the format, and
  [`tests/digests.txt`](tests/digests.txt) the C++'s digests.
- **One change, made in both:** the sample rate. The module runs at its DAC's 47 872.34 Hz, so
  Plaits computes pitch from that rate; this port runs at exactly 48 kHz, and the reference was
  built with the same one-line change, so both sides render at 48 kHz. Nothing else differs.
- **Even the C++'s accidents are kept.** Where Plaits reads past the end of a table, the port
  reads what the reference build keeps in those bytes. Where an `Init` sets four of five voices,
  the fifth starts at zero. Float-to-integer conversions behave as MSVC's do, `std::max` and
  `std::min` as C++ evaluates them with a NaN, and every expression in the C++'s order and
  precision, `double` where the C++ promotes to `double`.
- **Each voice has its own random stream**, starting where Plaits' one generator starts, so every
  voice draws exactly what the module's one voice draws.

The reference is the C++ built for x86-64. On the module itself, GCC for ARM, the few reads past
a table see other bytes, so where those reads happen (wave terrain with every control at 1) the
module's frames may differ from both.
The cases have been checked on Windows, x86-64. The port uses no floating-point library call but
`abs` and `sqrt`, both exact, so other platforms should give the same bits, but they have not been
checked yet. To render the reference's bits, run with flush-to-zero on, as the reference did. The
tests set it on x86-64. Without it the sound is the same to the ear, but a sample that passes
through a denormal can differ in its last bit.

## Using it

```rust
use mxm_macro_osc::dsp::K_BLOCK_SIZE;
use mxm_macro_osc::{Frame, Modulations, Patch, Voice};

let mut voice = Voice::new(); // its 16 KB engine memory, allocated once
let patch = Patch {
    engine: 8, // 0 to 23, as the module numbers them
    note: 48.0,
    harmonics: 0.5,
    timbre: 0.5,
    morph: 0.5,
    decay: 0.5,
    lpg_colour: 0.5,
    ..Patch::default()
};
let modulations = Modulations::default(); // nothing patched: the voice drones
let mut frames = [Frame::default(); K_BLOCK_SIZE];
voice.render(&patch, &modulations, &mut frames); // 12 frames of OUT and AUX at 48 kHz
```

`Patch` and `Modulations` are Plaits' own, field for field: the knobs and the CV inputs, with a
`*_patched` flag for each jack that changes behaviour when patched. Set `trigger_patched` and send
`trigger` above 0.3 to strike the voice, and patch `level` to play it through the low-pass gate.
Call `render` once per block of 12 frames, as the module does. At other rates, resample.

- **Realtime safe:** no allocation after `Voice::new`, no locks, no file IO, and no panic on any
  input; `#![forbid(unsafe_code)]`.
- **One dependency:** `bytemuck`, for the typed views of the voice's engine memory.
- **Polyphony** is many voices. Each is self-contained, and `clear` returns one to its first
  state without allocating.
- Builds for WebAssembly (`wasm32-unknown-unknown`), so it can run in a web browser.
  WebAssembly has no flush-to-zero, so the last-bit caveat above applies there.

## The engines

Numbered as Plaits numbers them:

| | |
|---|---|
| 0–7 | virtual analogue with a filter, phase distortion, three banks of six-operator FM (32 patches each), wave terrain, string machine, chiptune |
| 8–15 | virtual analogue, waveshaping, two-operator FM, granular formant, harmonic, wavetable, chords, speech |
| 16–23 | granular swarm, filtered noise, particle noise, inharmonic string, modal resonator, bass drum, snare drum, hi-hat |

The data the engines play ships as Plaits ships it: the speech engine's LPC-encoded words, the
six-operator engines' 96 patches and the wavetables. Not ported: the module's user data, and
everything of the hardware (its interface, settings, calibration and drivers).

## How it is organised

One Rust module per C++ file, at the same path and in the same order of statements, so the port
reads line for line beside the original: `src/voice.rs` is `plaits/dsp/voice.{h,cc}`,
`src/engine/virtual_analog_engine.rs` is `plaits/dsp/engine/virtual_analog_engine.{h,cc}`. The
tables are generated from Plaits' own compiled values by `gen/tables.py`, never recomputed.
[`AGENTS.md`](AGENTS.md) has the porting rules that keep it bit identical.

## Licence

MIT; see [`LICENSE`](LICENSE). The code it is ported from is Emilie Gillet's, under the MIT licence
in [`NOTICE.md`](NOTICE.md), and each ported file keeps its original's copyright line. Emilie Gillet
has no part in this crate and does not endorse it. Mutable Instruments is a registered trademark.
The MXM name is not covered by the licence.

Contributions are welcome; see [`CONTRIBUTING.md`](CONTRIBUTING.md).
