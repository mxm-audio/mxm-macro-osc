# Notice

mxm-macro-osc — Copyright (c) 2026 mxm. MIT licence: see `LICENSE`. It comes with no warranty. The
MXM name is not covered by that licence.

## The code it is ported from

**It is ported from Emilie Gillet's code for Mutable Instruments Plaits** (`plaits/` in
[pichenettes/eurorack](https://github.com/pichenettes/eurorack), commit
`f4cfa7697095a28d292bff758beb51deb49b5a86`) and the parts of
[pichenettes/stmlib](https://github.com/pichenettes/stmlib) it uses (commit
`e3bd7c9cc00e4364166f9905c0509b6ffd0535ec`), under the MIT licence below. Each ported file keeps
the copyright line of the file it was ported from. Mutable Instruments is a registered trademark;
Emilie Gillet has no part in this crate and does not endorse it.

**The data the engines play is Plaits'**, as Plaits ships it:

- **The speech engine's words**, LPC-10 encoded; Plaits' source: *"LPC10 encoded words extracted
  from various TI ROMs"* (`plaits/dsp/speech/lpc_speech_synth_words.cc`).
- **The six-operator FM engines' 96 patches**; Plaits' source: *"My favorite DX7 patches"*
  (`plaits/resources/fm_patches.py`), taken from the DX7 banks in `plaits/resources/syx/`.
- **The wavetables**, made by Plaits' `resources/wavetables.py`, some from the Braids waves in
  `plaits/resources/waves.bin`.

## Emilie Gillet's licence

```text
Copyright Emilie Gillet (emilie.o.gillet@gmail.com), the years in each file's header.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

## Its dependencies

`bytemuck` (MIT, Apache-2.0 or Zlib); for its tests, `sha2` (MIT or Apache-2.0).
