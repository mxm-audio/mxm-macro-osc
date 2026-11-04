# The reference cases

Each `.case` file drives one Plaits voice from its first block; `../digests.txt` holds the SHA-256
of the frames Plaits' own C++ renders for it (built with MSVC, x64, `/O2 /fp:precise`, flush-to-zero
on, `kCorrectedSampleRate` at 48 000; nothing else changed). The frames are interleaved OUT and AUX,
little-endian `int16`, `Voice::Render`'s output, one second (4000 blocks of 12) a case. The C++
voice starts each case fresh, its random state at its initial value.

## A case

Text, one directive a line, `#` to the line's end a comment:

| Directive | Meaning |
|---|---|
| `blocks <n>` | Render *n* blocks |
| `set <field> <value>` | The field's value before block 0 |
| `at <block> <field> <value>` | The field takes the value at the start of that block |
| `ramp <b0> <b1> <field> <v0> <v1>` | At the start of each block *b* from *b0* to *b1*, the field takes `v0 + (v1 − v0) × t` with `t = (b − b0) as f32 / (b1 − b0) as f32`, all in f32 |

Fields are `Patch`'s and `Modulations`' members as `patch.<name>` and `mod.<name>`. Values are
parsed as f32 (C's `%f`); `patch.engine` takes the value truncated to an integer, a bool is true
unless the value is 0. Every field starts at zero (false) before the `set` lines. Each block, the
`at` and `ramp` directives that cover it act in the order they are written, then the block renders.

## The cases

For each engine `NN` from 00 to 23, numbered as `dsp/voice.cc` registers them (00–07 firmware 1.2's
bank, 08–15 the pitched bank, 16–23 noise and percussion):

| Case | Covers |
|---|---|
| `eNN-ping` | pinged at four notes, the controls at the middle |
| `eNN-held` | two notes held 0.4 s each |
| `eNN-harmonics`, `-timbre`, `-morph` | each control from 0 to 1 over the second, pinged |
| `eNN-drone` | no trigger and no level, so the gate is bypassed; the note from 24 to 96 |
| `eNN-level` | Level patched: the gate follows it up and down |
| `eNN-envelope` | the decay envelope through the FM, Timbre and Morph attenuverters |
| `eNN-edges` | the controls, decay and colour at 0, then at 1 |

and four that cross engines: `model-change` (a held note while the engine steps through all 24),
`model-cv` (the engine CV swept, latched at each trigger), `fm-cv` (FM patched and swept, engine 9)
and `timbre-cv` (Timbre patched, engine 12).
