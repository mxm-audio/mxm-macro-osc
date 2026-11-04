//! The reference: every case in `cases/` whose engines are ported, rendered by a fresh voice and
//! compared, by the SHA-256 of its frames, with what Plaits' C++ renders (`digests.txt`). The
//! case format and its semantics are `cases/README.md`'s.
//!
//! Debug aids, by environment variable:
//!
//! - `MXM_THANKS_EG_RAW_DIR=<dir>`: write each rendered case's frames to `<dir>/<case>.raw`, the
//!   format of the C++ harness's renders (interleaved OUT, AUX, little-endian `int16`).
//! - `MXM_THANKS_EG_REFERENCE_DIR=<dir>`: the C++ harness's renders, `<dir>/<case>.raw`; on a
//!   mismatch the first differing block and frame are printed from it.
//! - `MXM_THANKS_EG_CASE=<name>`: render only that case (`e08-ping`), whether ported or not.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use mxm_thanks_eg_engines::dsp::K_BLOCK_SIZE;
use mxm_thanks_eg_engines::stmlib::dsp::dsp::cast_i32;
use mxm_thanks_eg_engines::{Frame, Modulations, Patch, Voice};
use sha2::{Digest, Sha256};

/// The engines ported so far: a case `eNN-*` runs when `NN` is here, and a shared case when
/// every engine it uses is.
const PORTED: &[usize] = &[
    2, 3, 4, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
];

/// The engines each shared case uses (the rest are named `eNN-*`).
fn engines_of(case: &str) -> Vec<usize> {
    match case {
        "model-change" | "model-cv" => (0..24).collect(),
        "fm-cv" => vec![9],
        "timbre-cv" => vec![12],
        _ => match case
            .strip_prefix('e')
            .and_then(|rest| rest.get(..2))
            .map(str::parse)
        {
            Some(Ok(engine)) if case.as_bytes().get(3) == Some(&b'-') => vec![engine],
            _ => panic!("{case}: not a case this harness knows the engines of"),
        },
    }
}

fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("cases")
}

// ---------------------------------------------------------------------------------------------
// Flush-to-zero

/// Sets MXCSR's flush-to-zero bit (15), not denormals-are-zero (6), as the C++ harness's
/// `_MM_SET_FLUSH_ZERO_MODE(_MM_FLUSH_ZERO_ON)` does. MXCSR is per thread, and each test runs on
/// its own thread, so every render sets it first.
#[cfg(target_arch = "x86_64")]
fn set_flush_to_zero() {
    let mut csr: u32 = 0;
    // SAFETY: `stmxcsr` stores MXCSR to the u32 the pointer names, and `ldmxcsr` loads it back
    // with one more bit: flush-to-zero changes only how results that would be denormal round,
    // which is what the reference was rendered with. Nothing else in the process is affected
    // beyond this thread's floating point.
    unsafe {
        std::arch::asm!("stmxcsr [{}]", in(reg) &mut csr, options(nostack, preserves_flags));
        csr |= 1 << 15;
        std::arch::asm!("ldmxcsr [{}]", in(reg) &csr, options(nostack, preserves_flags));
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn set_flush_to_zero() {}

// ---------------------------------------------------------------------------------------------
// The case format

#[derive(Clone, Copy, Debug)]
enum Field {
    PatchF32(fn(&mut Patch) -> &mut f32),
    ModF32(fn(&mut Modulations) -> &mut f32),
    Engine,
    Bool(fn(&mut Modulations) -> &mut bool),
}

fn field(name: &str) -> Option<Field> {
    use Field::*;
    Some(match name {
        "patch.note" => PatchF32(|p| &mut p.note),
        "patch.harmonics" => PatchF32(|p| &mut p.harmonics),
        "patch.timbre" => PatchF32(|p| &mut p.timbre),
        "patch.morph" => PatchF32(|p| &mut p.morph),
        "patch.frequency_modulation_amount" => PatchF32(|p| &mut p.frequency_modulation_amount),
        "patch.timbre_modulation_amount" => PatchF32(|p| &mut p.timbre_modulation_amount),
        "patch.morph_modulation_amount" => PatchF32(|p| &mut p.morph_modulation_amount),
        "patch.engine" => Engine,
        "patch.decay" => PatchF32(|p| &mut p.decay),
        "patch.lpg_colour" => PatchF32(|p| &mut p.lpg_colour),
        "mod.engine" => ModF32(|m| &mut m.engine),
        "mod.note" => ModF32(|m| &mut m.note),
        "mod.frequency" => ModF32(|m| &mut m.frequency),
        "mod.harmonics" => ModF32(|m| &mut m.harmonics),
        "mod.timbre" => ModF32(|m| &mut m.timbre),
        "mod.morph" => ModF32(|m| &mut m.morph),
        "mod.trigger" => ModF32(|m| &mut m.trigger),
        "mod.level" => ModF32(|m| &mut m.level),
        "mod.frequency_patched" => Bool(|m| &mut m.frequency_patched),
        "mod.timbre_patched" => Bool(|m| &mut m.timbre_patched),
        "mod.morph_patched" => Bool(|m| &mut m.morph_patched),
        "mod.trigger_patched" => Bool(|m| &mut m.trigger_patched),
        "mod.level_patched" => Bool(|m| &mut m.level_patched),
        _ => return None,
    })
}

/// The harness's `assign`: a float as it is, `patch.engine` through `static_cast<int>` (as MSVC
/// compiles it), a bool true unless the value is 0.
fn assign(field: Field, value: f32, patch: &mut Patch, modulations: &mut Modulations) {
    match field {
        Field::PatchF32(get) => *get(patch) = value,
        Field::ModF32(get) => *get(modulations) = value,
        Field::Engine => patch.engine = cast_i32(value),
        Field::Bool(get) => *get(modulations) = value != 0.0,
    }
}

struct Action {
    b0: i32,
    b1: i32,
    field: Field,
    v0: f32,
    v1: f32,
}

struct Case {
    blocks: usize,
    patch: Patch,
    modulations: Modulations,
    actions: Vec<Action>,
}

/// A value as `sscanf`'s `%f` reads it into a `float`: the longest prefix that is a decimal
/// number, rounded once to the nearest float (as the UCRT and Rust both do).
fn scan_f32(word: &str) -> Option<f32> {
    let bytes = word.as_bytes();
    let mut end = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        end += 1;
    }
    let digits_start = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    if end == digits_start || (end == digits_start + 1 && bytes[digits_start] == b'.') {
        return None;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exponent = end + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        if bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            while bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
                exponent += 1;
            }
            end = exponent;
        }
    }
    word[..end].parse().ok()
}

/// A value as `%d` reads it.
fn scan_i32(word: &str) -> Option<i32> {
    let bytes = word.as_bytes();
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let digits_start = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if end == digits_start {
        return None;
    }
    word[..end].parse().ok()
}

fn parse_case(name: &str, text: &str) -> Case {
    let mut case = Case {
        blocks: 0,
        patch: Patch::default(),
        modulations: Modulations::default(),
        actions: Vec::new(),
    };
    let mut blocks: i32 = -1;
    for (number, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("");
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some(&directive) = words.first() else {
            continue;
        };
        macro_rules! bad {
            () => {
                panic!("{name}:{}: bad line {line:?}", number + 1)
            };
        }
        let field_of =
            |word: &str| field(word).unwrap_or_else(|| panic!("{name}: unknown field {word}"));
        match directive {
            "blocks" => {
                blocks = words
                    .get(1)
                    .and_then(|w| scan_i32(w))
                    .unwrap_or_else(|| bad!())
            }
            "set" => {
                let (Some(f), Some(v)) = (words.get(1), words.get(2).and_then(|w| scan_f32(w)))
                else {
                    bad!()
                };
                assign(field_of(f), v, &mut case.patch, &mut case.modulations);
            }
            "at" => {
                let (Some(b0), Some(f), Some(v)) = (
                    words.get(1).and_then(|w| scan_i32(w)),
                    words.get(2),
                    words.get(3).and_then(|w| scan_f32(w)),
                ) else {
                    bad!()
                };
                case.actions.push(Action {
                    b0,
                    b1: b0,
                    field: field_of(f),
                    v0: v,
                    v1: v,
                });
            }
            "ramp" => {
                let (Some(b0), Some(b1), Some(f), Some(v0), Some(v1)) = (
                    words.get(1).and_then(|w| scan_i32(w)),
                    words.get(2).and_then(|w| scan_i32(w)),
                    words.get(3),
                    words.get(4).and_then(|w| scan_f32(w)),
                    words.get(5).and_then(|w| scan_f32(w)),
                ) else {
                    bad!()
                };
                if b1 <= b0 {
                    bad!();
                }
                case.actions.push(Action {
                    b0,
                    b1,
                    field: field_of(f),
                    v0,
                    v1,
                });
            }
            _ => bad!(),
        }
    }
    assert!(blocks > 0, "{name}: no blocks");
    case.blocks = blocks as usize;
    case
}

// ---------------------------------------------------------------------------------------------
// The renderer

/// One fresh voice renders the case, block by block, into interleaved little-endian `int16`.
fn render(case: &Case) -> Vec<u8> {
    set_flush_to_zero();
    let mut patch = case.patch;
    let mut modulations = case.modulations;
    let mut voice = Voice::new();
    let mut frames = [Frame::default(); K_BLOCK_SIZE];
    let mut bytes = Vec::with_capacity(case.blocks * K_BLOCK_SIZE * 4);
    for block in 0..case.blocks as i32 {
        for a in &case.actions {
            if block < a.b0 || block > a.b1 {
                continue;
            }
            let t = if a.b1 == a.b0 {
                0.0
            } else {
                (block - a.b0) as f32 / (a.b1 - a.b0) as f32
            };
            assign(
                a.field,
                a.v0 + (a.v1 - a.v0) * t,
                &mut patch,
                &mut modulations,
            );
        }
        voice.render(&patch, &modulations, &mut frames);
        for frame in &frames {
            bytes.extend_from_slice(&frame.out.to_le_bytes());
            bytes.extend_from_slice(&frame.aux.to_le_bytes());
        }
    }
    bytes
}

fn digests() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("digests.txt");
    let text = std::fs::read_to_string(&path).expect("tests/digests.txt");
    text.lines()
        .filter_map(|line| {
            let (digest, file) = line.split_once(" *")?;
            Some((
                file.trim().strip_suffix(".raw")?.to_string(),
                digest.trim().to_lowercase(),
            ))
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Where our frames first differ from the C++'s, block and frame, when its render is at hand.
fn first_difference(case: &str, ours: &[u8]) -> String {
    let Some(dir) = std::env::var_os("MXM_THANKS_EG_REFERENCE_DIR") else {
        return "set MXM_THANKS_EG_REFERENCE_DIR to the C++ renders to see the first differing frame"
            .to_string();
    };
    let path = Path::new(&dir).join(format!("{case}.raw"));
    let Ok(theirs) = std::fs::read(&path) else {
        return format!("no reference render at {}", path.display());
    };
    let sample = |bytes: &[u8], i: usize| i16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]);
    let samples = ours.len().min(theirs.len()) / 2;
    match (0..samples).find(|&i| sample(ours, i) != sample(&theirs, i)) {
        None if ours.len() == theirs.len() => "identical to the reference render".to_string(),
        None => format!(
            "{} bytes against the reference's {}",
            ours.len(),
            theirs.len()
        ),
        Some(i) => {
            let frame = i / 2;
            let channel = if i % 2 == 0 { "OUT" } else { "AUX" };
            format!(
                "first difference at block {}, frame {} ({channel}): ours {}, the C++ {}",
                frame / K_BLOCK_SIZE,
                frame % K_BLOCK_SIZE,
                sample(ours, i),
                sample(&theirs, i)
            )
        }
    }
}

#[test]
fn reference_cases() {
    let digests = digests();
    let only = std::env::var("MXM_THANKS_EG_CASE").ok();
    let raw_dir = std::env::var_os("MXM_THANKS_EG_RAW_DIR").map(PathBuf::from);
    let mut names: Vec<String> = std::fs::read_dir(cases_dir())
        .expect("tests/cases")
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            Some(name.strip_suffix(".case")?.to_string())
        })
        .collect();
    names.sort();

    let mut ran = 0;
    let mut failures = Vec::new();
    for name in &names {
        let selected = match &only {
            Some(only) => only == name,
            None => engines_of(name).iter().all(|e| PORTED.contains(e)),
        };
        if !selected {
            continue;
        }
        let text = std::fs::read_to_string(cases_dir().join(format!("{name}.case"))).expect("case");
        let frames = render(&parse_case(name, &text));
        if let Some(dir) = &raw_dir {
            std::fs::create_dir_all(dir).expect("MXM_THANKS_EG_RAW_DIR");
            std::fs::write(dir.join(format!("{name}.raw")), &frames).expect("write raw");
        }
        let digest = hex(&Sha256::digest(&frames));
        let expected = digests
            .get(name)
            .unwrap_or_else(|| panic!("{name}: no digest"));
        ran += 1;
        if &digest == expected {
            println!("{name}: bit for bit");
        } else {
            let at = first_difference(name, &frames);
            println!("{name}: MISMATCH ({at})");
            failures.push(format!("{name}: {at}"));
        }
    }
    assert!(ran > 0, "no case ran");
    assert!(
        failures.is_empty(),
        "{} of {ran} cases differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn the_case_values_round_once() {
    // `%f` into a float: whether the UCRT rounds the decimal to float once, or to double and
    // then to float, gives the same float for every value in the cases (so the parse above is
    // the C++'s either way).
    for entry in std::fs::read_dir(cases_dir()).expect("tests/cases") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "case") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("case");
        for word in text
            .lines()
            .flat_map(|l| l.split('#').next().unwrap_or("").split_whitespace())
        {
            if let Some(once) = scan_f32(word) {
                let twice = word.parse::<f64>().map(|d| d as f32).unwrap_or(once);
                assert_eq!(
                    once.to_bits(),
                    twice.to_bits(),
                    "{}: {word}",
                    path.display()
                );
            }
        }
    }
}
