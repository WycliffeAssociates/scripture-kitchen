//! `cargo run -p sous-core --bin gen-unicode` — regenerate the committed
//! `src/unicode/table.rs` from the pinned UCD 17.0.0 extracts in
//! `testdata/ucd/`.
//!
//! ```text
//! testdata/ucd/*.txt  →  src/unicode/table.rs
//!                        CLASS_RANGES  coalesced (lo, hi, u16) runs, all planes
//!                        ASCII         flat u16[128]
//!                        BLOCK_INDEX   u16[1024], one entry per BMP cp>>6
//!                        BLOCKS        deduplicated [u16; 64] blocks
//!                     →  src/unicode/pools.rs
//!                        POOLS         sorted (scalar, Pool), G2's neighbour
//!                                      categories, no row a Class bit fixes
//! ```
//!
//! Never a `build.rs`: both tables are committed, reviewable artifacts, and a
//! second run must leave `git diff --exit-code` clean.
//!
//! Two optional arguments override the output paths, table then pools; the
//! determinism test passes scratch paths and compares the bytes with the
//! committed files.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;
use sous_core::unicode::bits;

const MAX_CP: u32 = 0x10_FFFF;
/// Scalars per second-level block. Also the UTF-8 continuation-byte fanout,
/// so one pool serves the decoded two-level lookup and the byte trie.
const BLOCK: usize = 64;

fn main() -> std::io::Result<()> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ucd = manifest.join("testdata/ucd");
    let classes = classify(&ucd);
    let rendered = render(&classes);

    let mut args = std::env::args().skip(1);
    let table = args
        .next()
        .map_or_else(|| manifest.join("src/unicode/table.rs"), PathBuf::from);
    let pools_out = args
        .next()
        .map_or_else(|| manifest.join("src/unicode/pools.rs"), PathBuf::from);
    std::fs::write(&table, rendered)?;
    std::fs::write(&pools_out, render_pools(&pools(&ucd)))?;
    Ok(())
}

// ── UCD parsing ──────────────────────────────────────────────────────────

/// One `lo..hi ; Property` line, with the trailing `# comment` dropped.
fn for_each_range(path: &Path, mut visit: impl FnMut(&str, u32, u32)) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!(
            "pinned UCD extract {} must be present: {error}",
            path.display()
        )
    });
    for line in text.lines() {
        let body = line.split('#').next().unwrap_or("").trim();
        if body.is_empty() {
            continue;
        }
        let (range, rest) = body
            .split_once(';')
            .expect("a UCD data line has a property");
        // `InCB; Linker` is two fields; join them so callers name the whole
        // property path.
        let property = rest
            .split(';')
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("; ");
        let range = range.trim();
        let (lo, hi) = match range.split_once("..") {
            Some((lo, hi)) => (hex(lo), hex(hi)),
            None => (hex(range), hex(range)),
        };
        visit(&property, lo, hi);
    }
}

fn hex(text: &str) -> u32 {
    u32::from_str_radix(text.trim(), 16).expect("a UCD scalar is hexadecimal")
}

/// `class[cp]` for every scalar; surrogates keep class 0 and are never read.
fn classify(ucd: &Path) -> Vec<u16> {
    let mut class = vec![0u16; MAX_CP as usize + 1];
    let set = |lo: u32, hi: u32, bit: u16, class: &mut Vec<u16>| {
        for cp in lo..=hi.min(MAX_CP) {
            class[cp as usize] |= bit;
        }
    };

    for_each_range(
        &ucd.join("DerivedGeneralCategory.txt"),
        |property, lo, hi| {
            let bit = match property {
                "Mn" | "Mc" | "Me" => bits::MARK,
                "Pc" | "Pd" | "Ps" | "Pe" | "Pi" | "Pf" | "Po" => bits::PUNCTUATION,
                "Sm" | "Sc" | "Sk" | "So" => bits::SYMBOL,
                "Nd" => bits::DECIMAL_DIGIT,
                "Cc" => bits::CONTROL,
                "Cf" => bits::FORMAT,
                _ => return,
            };
            set(lo, hi, bit, &mut class);
        },
    );
    for_each_range(
        &ucd.join("DerivedCoreProperties.txt"),
        |property, lo, hi| {
            let bit = match property {
                "Alphabetic" => bits::ALPHABETIC,
                "Uppercase" => bits::UPPERCASE,
                "Lowercase" => bits::LOWERCASE,
                "InCB; Linker" => bits::LINKER,
                _ => return,
            };
            set(lo, hi, bit, &mut class);
        },
    );
    for_each_range(&ucd.join("PropList.txt"), |property, lo, hi| {
        if property == "White_Space" {
            set(lo, hi, bits::WHITESPACE, &mut class);
        }
    });
    for_each_range(
        &ucd.join("GraphemeBreakProperty.txt"),
        |property, lo, hi| {
            let bit = match property {
                "Extend" | "SpacingMark" | "ZWJ" => bits::EXTENDER,
                "Control" | "CR" | "LF" => bits::COMPLEX | bits::GCB_CONTROL,
                "Prepend" => bits::COMPLEX | bits::PREPEND,
                "Regional_Indicator" | "L" | "V" | "T" | "LV" | "LVT" => bits::COMPLEX,
                _ => return,
            };
            set(lo, hi, bit, &mut class);
        },
    );
    for_each_range(&ucd.join("emoji-data.txt"), |property, lo, hi| {
        if property == "Extended_Pictographic" {
            set(lo, hi, bits::COMPLEX, &mut class);
        }
    });

    // Noncharacters are a spec constant, not a file (UAX #44, table 2-3).
    for cp in 0xFDD0..=0xFDEF {
        class[cp as usize] |= bits::NONCHARACTER;
    }
    for plane in 0..=0x10u32 {
        for last in [0xFFFEu32, 0xFFFF] {
            class[((plane << 16) | last) as usize] |= bits::NONCHARACTER;
        }
    }
    for cp in 0xD800..=0xDFFFu32 {
        class[cp as usize] = 0;
    }
    class
}

// ── The G2 pools ─────────────────────────────────────────────────────────

/// Pool discriminants, mirroring `unicode::Pool`. `Digit` and `Symbol` are
/// fixed by a `Class` bit and `Other` is the default, so none needs a row.
const POOL_NAMES: [&str; 5] = ["Quote", "Bracket", "Dash", "Terminal", "Separator"];
const NO_POOL: u8 = u8::MAX;

/// Every scalar whose pool a `Class` bit does not already fix.
///
/// Painted in reverse precedence, each pass overwriting the last, so the
/// result is `unicode::Pool`'s first-match-wins order.
fn pools(ucd: &Path) -> Vec<(u32, u8)> {
    let mut pool = vec![NO_POOL; MAX_CP as usize + 1];
    paint(ucd, "PropList.txt", &["Terminal_Punctuation"], 4, &mut pool);
    paint(ucd, "PropList.txt", &["Sentence_Terminal"], 3, &mut pool);
    paint(ucd, "PropList.txt", &["Dash"], 2, &mut pool);
    paint(
        ucd,
        "DerivedGeneralCategory.txt",
        &["Ps", "Pe", "Pi", "Pf"],
        1,
        &mut pool,
    );
    paint(ucd, "PropList.txt", &["Quotation_Mark"], 0, &mut pool);
    pool.iter()
        .enumerate()
        .filter(|&(_, &value)| value != NO_POOL)
        .map(|(cp, &value)| (cp as u32, value))
        .collect()
}

fn paint(ucd: &Path, file: &str, wanted: &[&str], value: u8, pool: &mut [u8]) {
    for_each_range(&ucd.join(file), |property, lo, hi| {
        if wanted.contains(&property) {
            for cp in lo..=hi.min(MAX_CP) {
                pool[cp as usize] = value;
            }
        }
    });
}

fn render_pools(rows: &[(u32, u8)]) -> String {
    let mut out = String::with_capacity(1 << 16);
    let _ = write!(
        out,
        "//! GENERATED by `cargo run -p sous-core --bin gen-unicode` from the\n\
         //! UCD 17.0.0 extracts in `testdata/ucd/`. Do not edit.\n\
         //!\n\
         //! {} scalars whose G2 pool no `Class` bit fixes. `Nd` answers\n\
         //! `Pool::Digit` and a bare `S*` answers `Pool::Symbol` without a row.\n\
         \n\
         use super::Pool;\n\
         \n\
         /// Sorted by scalar; `pool_of` binary searches it.\n\
         #[rustfmt::skip]\n\
         pub(super) static POOLS: &[(u32, Pool)] = &[\n",
        rows.len(),
    );
    for chunk in rows.chunks(3) {
        out.push_str("   ");
        for &(cp, value) in chunk {
            let _ = write!(out, " (0x{cp:05X}, Pool::{}),", POOL_NAMES[value as usize]);
        }
        out.push('\n');
    }
    out.push_str("];\n");
    out
}

// ── Emission ─────────────────────────────────────────────────────────────

fn render(class: &[u16]) -> String {
    let ranges = coalesce(class);
    let astral_start = ranges
        .iter()
        .position(|&(lo, _, _)| lo >= 0x1_0000)
        .expect("plane 1 and above carry classified scalars");
    assert_eq!(
        ranges[astral_start].0, 0x1_0000,
        "a range must start exactly at U+10000 so the astral search is a clean suffix"
    );

    let (index, blocks) = two_level(class);
    let mut out = String::with_capacity(1 << 20);
    let _ = write!(
        out,
        "//! GENERATED by `cargo run -p sous-core --bin gen-unicode` from the\n\
         //! UCD 17.0.0 extracts in `testdata/ucd/`. Do not edit.\n\
         //!\n\
         //! {ranges} nonzero ranges, {blocks} deduplicated {BLOCK}-scalar blocks.\n\
         \n\
         /// Coalesced runs of equal nonzero bits over every plane, ascending.\n\
         #[rustfmt::skip]\n\
         pub(super) const CLASS_RANGES: &[(u32, u32, u16)] = &[\n",
        ranges = ranges.len(),
        blocks = blocks.len(),
    );
    for chunk in ranges.chunks(4) {
        out.push_str("   ");
        for &(lo, hi, bits) in chunk {
            let _ = write!(out, " (0x{lo:05X}, 0x{hi:05X}, 0x{bits:04X}),");
        }
        out.push('\n');
    }
    out.push_str("];\n\n");

    let _ = write!(
        out,
        "/// Index of the first [`CLASS_RANGES`] entry at or above U+10000.\n\
         pub(super) const ASTRAL_START: usize = {astral_start};\n\n\
         /// Flat classes for U+0000..=U+007F: one load, no indirection.\n\
         #[rustfmt::skip]\n\
         pub(super) static ASCII: [u16; 128] = [\n"
    );
    emit_u16_rows(&mut out, &class[..128], 8);
    out.push_str("];\n\n");

    let _ = write!(
        out,
        "/// One [`BLOCKS`] id per BMP `cp >> 6`. A UTF-8 lead/continuation\n\
         /// pair yields the same index without decoding the scalar.\n\
         #[rustfmt::skip]\n\
         pub(super) static BLOCK_INDEX: [u16; {}] = [\n",
        index.len()
    );
    emit_u16_rows(&mut out, &index, 16);
    out.push_str("];\n\n");

    let _ = write!(
        out,
        "/// Deduplicated second-level blocks; block 0 is all-zero.\n\
         #[rustfmt::skip]\n\
         pub(super) static BLOCKS: [[u16; {BLOCK}]; {}] = [\n",
        blocks.len()
    );
    for block in &blocks {
        out.push_str("    [\n");
        emit_u16_rows(&mut out, block, 8);
        out.push_str("    ],\n");
    }
    out.push_str("];\n");
    out
}

fn emit_u16_rows(out: &mut String, values: &[u16], per_row: usize) {
    for row in values.chunks(per_row) {
        out.push_str("    ");
        for value in row {
            let _ = write!(out, "0x{value:04X}, ");
        }
        out.truncate(out.len() - 1);
        out.push('\n');
    }
}

fn coalesce(class: &[u16]) -> Vec<(u32, u32, u16)> {
    let mut ranges: Vec<(u32, u32, u16)> = Vec::new();
    for (cp, &bits) in class.iter().enumerate() {
        if bits == 0 {
            continue;
        }
        let cp = cp as u32;
        match ranges.last_mut() {
            Some(last) if last.1 + 1 == cp && last.2 == bits => last.1 = cp,
            _ => ranges.push((cp, cp, bits)),
        }
    }
    ranges
}

/// The BMP as `cp >> 6` indices into deduplicated blocks. Block ids are
/// assigned in first-seen order, so the emission is deterministic.
fn two_level(class: &[u16]) -> (Vec<u16>, Vec<[u16; BLOCK]>) {
    let mut blocks: Vec<[u16; BLOCK]> = vec![[0; BLOCK]];
    let mut seen: FxHashMap<[u16; BLOCK], u16> = FxHashMap::default();
    seen.insert([0; BLOCK], 0);
    let mut index = Vec::with_capacity(0x1_0000 / BLOCK);
    for base in (0..0x1_0000).step_by(BLOCK) {
        let mut block = [0u16; BLOCK];
        block.copy_from_slice(&class[base..base + BLOCK]);
        let id = *seen.entry(block).or_insert_with(|| {
            blocks.push(block);
            u16::try_from(blocks.len() - 1).expect("BMP holds at most 1024 distinct blocks")
        });
        index.push(id);
    }
    (index, blocks)
}
