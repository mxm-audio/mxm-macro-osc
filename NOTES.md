# NOTES — mxm-macro-osc

The reasoning behind [AGENTS.md](AGENTS.md) and the owner's rulings, dated.

## Its own repository (2026-11-05)

The port was written for an MXM instrument, in that instrument's repository, and held there to
Plaits' C++ bit for bit: all 24 engines, all 220 reference cases (2026-11-04). The owner, on
2026-11-05: *"Well, since it is done we might as well make it public. And in the readme emphasize
that it is bit identical to the original."* It was split out with its history, and the
instrument depends on it by its `main`, locked.

- **MIT**, as the code it is ported from and the MXM kit are: a port of MIT code that only GPL
  programs could use would leave most users with nothing.
- **The name keeps Plaits' out of it**: "Plaits" and "Mutable Instruments" appear in the README,
  the notice and these docs, never in a product's name. Plaits is a macro-oscillator, hence
  `mxm-macro-osc`.
- **Bit identical is the point of it.** Another Rust port of Plaits exists; what this one adds is
  that it is checked against the C++, frame by frame, so the README leads with that claim and
  says exactly what it covers: the reference build, the one rate change made in both, the reads
  past a table that follow the reference build rather than the module, flush-to-zero, and the
  platforms checked.
