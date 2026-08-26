//! `src/tables/generated.rs` is what the current authored rows + generator
//! would produce. On failure the fix is always `cargo run --bin codegen`.
//!
//! It exists because `generated.rs` is CHECKED IN — consumers never run codegen,
//! so nothing else would notice a row edit that was not regenerated.

use usfm_onion_2::lint::{self, LINT_ROWS};
use usfm_onion_2::tables::schema::{Numbering, SpellingShape};
use usfm_onion_2::tables::{emit, generated, rows};

const CHECKED_IN: &str = include_str!("../src/tables/generated.rs");
const CHECKED_IN_DIAGNOSTICS: &str = include_str!("../onion-wasm/diagnostics.json");

#[test]
fn generated_rs_is_not_stale() {
    let fresh = emit::generated_rs();
    if fresh == CHECKED_IN {
        return;
    }

    // Report the first differing line rather than dumping 1000 lines of Rust.
    let at = fresh
        .lines()
        .zip(CHECKED_IN.lines())
        .position(|(a, b)| a != b);
    match at {
        Some(line) => panic!(
            "src/tables/generated.rs is STALE — run `cargo run --bin codegen`.\n\
             First difference at line {}:\n  checked in: {}\n  fresh:      {}",
            line + 1,
            CHECKED_IN.lines().nth(line).unwrap_or("<eof>"),
            fresh.lines().nth(line).unwrap_or("<eof>"),
        ),
        None => panic!(
            "src/tables/generated.rs is STALE — run `cargo run --bin codegen`.\n\
             Lines agree as far as they go; length differs ({} checked in vs {} fresh).",
            CHECKED_IN.lines().count(),
            fresh.lines().count(),
        ),
    }
}

/// The same deal for the diagnostics side-table the JS bundle loads: it is the
/// ONE place a lint code's name, ladder rung and message template reach a
/// consumer, and nothing else would notice a row edit that was not regenerated.
#[test]
fn diagnostics_json_is_not_stale() {
    let fresh = lint::diagnostics_json();
    if fresh == CHECKED_IN_DIAGNOSTICS {
        return;
    }
    let at = fresh
        .lines()
        .zip(CHECKED_IN_DIAGNOSTICS.lines())
        .position(|(a, b)| a != b);
    panic!(
        "onion-wasm/diagnostics.json is STALE — run `cargo run --bin codegen`.\n\
         First difference at line {:?}:\n  checked in: {}\n  fresh:      {}",
        at.map(|line| line + 1),
        at.and_then(|line| CHECKED_IN_DIAGNOSTICS.lines().nth(line))
            .unwrap_or("<eof>"),
        at.and_then(|line| fresh.lines().nth(line))
            .unwrap_or("<eof>"),
    );
}

/// The side-table is INDEX-ADDRESSED: a finding carries `code` as a number, so
/// entry `n` must be the `n`th row and the array must be dense.
#[test]
fn the_side_table_is_dense_and_index_addressed() {
    let json = lint::diagnostics_json();
    for (code, row) in LINT_ROWS.iter().enumerate() {
        assert_eq!(row.code as usize, code, "{} is out of order", row.name);
        assert!(
            json.contains(&format!("\"code\": {code}, \"name\": \"{}\"", row.name)),
            "{} is missing from the side table",
            row.name
        );
    }
}

/// The generator's whole job as a round trip: every authored row's own name
/// resolves back to that row's index, in every spelling the row claims. A wrong
/// u64 key, a bad strip order or an off-by-one in the emitted match shows here.
#[test]
fn every_row_resolves_to_itself() {
    for (idx, row) in rows::ROWS.iter().enumerate() {
        if row.marker.is_empty() {
            continue; // index 0 has no name by design
        }
        let shape = match row.shape {
            SpellingShape::MilestoneOnly => SpellingShape::MilestoneOnly,
            _ => SpellingShape::PlainOnly,
        };

        // The bare spelling is always legal.
        let mut spellings = vec![row.marker.to_string()];
        match row.numbered_max {
            Numbering::UpTo(cap) => {
                spellings.extend((1..=cap).map(|n| format!("{}{n}", row.marker)));
            }
            Numbering::Unbounded => spellings.push(format!("{}1", row.marker)),
            Numbering::TableColumns => {
                spellings.push(format!("{}1", row.marker));
                spellings.push(format!("{}1-2", row.marker));
            }
            Numbering::Unnumbered => {}
        }
        // A milestone row's real spellings carry the side suffix.
        if matches!(row.shape, SpellingShape::MilestoneOnly) {
            spellings = spellings
                .iter()
                .flat_map(|s| [format!("{s}-s"), format!("{s}-e")])
                .collect();
        }

        for spelling in spellings {
            assert_eq!(
                generated::marker_idx(spelling.as_bytes(), shape),
                idx as generated::MarkerIdx,
                "`\\{spelling}` did not resolve to row {idx} ({})",
                row.marker
            );
        }
    }
}

/// The unresolved path is a ROW, not an error: every shape of "we don't know
/// this" lands on index 0.
#[test]
fn unknown_markers_land_on_the_empty_row() {
    let cases: &[&[u8]] = &[
        b"",           // nothing at all
        b"zaln",       // unconfigured `\z` extension — bails before any match
        b"zaln-s",     //   …including the milestone spelling
        b"notamarker", // unknown name
        b"s5",         // real name, illegal level (`s` is UpTo(4))
        b"p9",         // real name, `p` takes no digits
        b"ADD",        // right name, wrong case
    ];
    for case in cases {
        assert_eq!(
            generated::marker_idx(case, SpellingShape::PlainOnly),
            generated::UNRESOLVED,
            "`\\{}` should have resolved to the empty row",
            String::from_utf8_lossy(case)
        );
    }
}

/// The facts codegen bakes agree with the schema `const fn`s they came from.
/// No consumer ever recomputes them, so nothing else would catch a drift.
#[test]
fn baked_derived_facts_match_the_schema() {
    for (idx, row) in rows::ROWS.iter().enumerate() {
        let idx = idx as generated::MarkerIdx;
        assert_eq!(
            generated::contributes_context(idx),
            row.contributes_context(),
            "{}: baked contributes_context disagrees with schema",
            row.marker
        );
    }
}

/// The context mask is EXACTLY what the row authored — nothing added. The
/// generator must never PROMOTE bits (e.g. Footnote onto every block-legal
/// character marker): that flattens a real per-marker distinction — `bd` and
/// `it` do list Footnote, `nd`/`add`/`wj`/`fm` do not. If a marker is legal
/// somewhere, its ROW says so.
#[test]
fn context_mask_invents_nothing() {
    for (idx, row) in rows::ROWS.iter().enumerate() {
        let authored = row
            .allowed_contexts
            .iter()
            .fold(0u32, |mask, ctx| mask | generated::context_bit(*ctx));
        assert_eq!(
            generated::context_mask(idx as generated::MarkerIdx),
            authored,
            "{}: packed context mask is not exactly `allowed_contexts`",
            row.marker
        );
    }
}
