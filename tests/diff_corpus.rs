//! The diff over REAL books: the round-trip law on corpus pairs, and the
//! narration on genuine data (a real doubled `\v 10`, two translations of one
//! book).
//!
//! The exhaustive sweep (every en_ulb book against its en_ult twin, plus a
//! mutation of each shape) is `#[ignore]`d as the pass-end gate; the fast slice
//! above it runs in every `cargo test`.

use std::path::{Path, PathBuf};

use usfm_onion_2::diff::{
    Decisions, DiffSkeleton, MergeSide, SlotRole, Status, UnitKind, diff, merge, to_edits,
};
use usfm_onion_2::edit::apply_splices;

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{path}: {err}"))
}

/// The round-trip law, both directions, plus the partition it rests on.
fn check_pair(label: &str, baseline: &str, current: &str) -> DiffSkeleton {
    let skeleton = diff(baseline, current);
    let empty = Decisions::new();

    let mut baseline_out = String::new();
    let mut current_out = String::new();
    for slot in &skeleton.slots {
        let unit = &skeleton.units[slot.unit as usize];
        if matches!(
            slot.role,
            SlotRole::Shared | SlotRole::BaselineOnly | SlotRole::PairBaseline
        ) {
            baseline_out
                .push_str(&baseline[unit.baseline.start as usize..unit.baseline.end as usize]);
        }
        if matches!(
            slot.role,
            SlotRole::Shared | SlotRole::CurrentOnly | SlotRole::PairCurrent
        ) {
            current_out.push_str(&current[unit.current.start as usize..unit.current.end as usize]);
        }
    }
    assert_eq!(baseline_out, baseline, "{label}: baseline partition");
    assert_eq!(current_out, current, "{label}: current partition");

    let to_current = to_edits(&skeleton, &empty, MergeSide::Current).unwrap();
    assert_eq!(
        apply_splices(baseline.as_bytes(), current.as_bytes(), &to_current),
        current.as_bytes(),
        "{label}: all-current replay"
    );
    let to_baseline = to_edits(&skeleton, &empty, MergeSide::Baseline).unwrap();
    assert!(
        to_baseline.is_empty(),
        "{label}: identity replay has no edits"
    );
    assert_eq!(
        merge(
            &skeleton,
            baseline.as_bytes(),
            current.as_bytes(),
            &empty,
            MergeSide::Baseline
        )
        .unwrap(),
        baseline.as_bytes(),
        "{label}: all-baseline merge"
    );
    skeleton
}

// ---- mutators: a real book, one edit ------------------------------------

fn drop_a_verse(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut done = false;
    for line in source.split_inclusive('\n') {
        if !done && line.trim_start().starts_with("\\v 3 ") {
            done = true;
            continue;
        }
        out.push_str(line);
    }
    out
}

fn duplicate_a_verse(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut done = false;
    for line in source.split_inclusive('\n') {
        out.push_str(line);
        if !done && line.trim_start().starts_with("\\v 3 ") {
            out.push_str(line);
            done = true;
        }
    }
    out
}

/// Swap the first two adjacent verse BLOCKS — a pure reorder. Verse text runs
/// over several lines in a real book, so a line swap would be a content edit;
/// a block runs from its `\v` to the next `\v`/`\c`.
fn reorder_two_verses(source: &str) -> String {
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    let starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            let line = line.trim_start();
            line.starts_with("\\v ") || line.starts_with("\\c ")
        })
        .map(|(index, _)| index)
        .collect();
    let first = starts
        .iter()
        .position(|index| lines[*index].trim_start().starts_with("\\v "));
    let Some(first) = first.filter(|first| {
        starts
            .get(first + 1)
            .is_some_and(|next| lines[*next].trim_start().starts_with("\\v "))
    }) else {
        return source.to_string();
    };
    let (a, b, c) = (
        starts[first],
        starts[first + 1],
        starts.get(first + 2).copied().unwrap_or(lines.len()),
    );
    let mut out = String::with_capacity(source.len());
    out.push_str(&lines[..a].concat());
    out.push_str(&lines[b..c].concat());
    out.push_str(&lines[a..b].concat());
    out.push_str(&lines[c..].concat());
    out
}

// ---- the fast slice ------------------------------------------------------

const MRK_ULB: &str = "example-corpora/en_ulb/42-MRK.usfm";
const MRK_ULT: &str = "example-corpora/en_ult/42-MRK.usfm";
const ROM_BDF: &str = "example-corpora/bdf_reg/46-ROM.usfm";

#[test]
fn two_translations_of_mark_round_trip() {
    let baseline = read(MRK_ULB);
    let current = read(MRK_ULT);
    let skeleton = check_pair("MRK ulb vs ult", &baseline, &current);

    // Two renderings of one book: mostly aligned verses, mostly Modified.
    let modified = skeleton
        .units
        .iter()
        .filter(|unit| unit.status == Status::Modified)
        .count();
    assert!(
        modified > 500,
        "expected a heavily modified diff, got {modified} modified units"
    );
    // Every unit's address renders on both sides where both sides exist.
    for unit in &skeleton.units {
        if let (Some(baseline_addr), Some(current_addr)) = (unit.baseline_addr, unit.current_addr) {
            assert!(baseline_addr.to_string().starts_with("MRK "));
            assert!(current_addr.to_string().starts_with("MRK "));
        }
    }
}

#[test]
fn a_book_against_itself_is_all_unchanged_and_costs_no_edits() {
    let source = read(MRK_ULB);
    let skeleton = check_pair("MRK self", &source, &source);
    assert!(
        skeleton
            .units
            .iter()
            .all(|unit| unit.status == Status::Unchanged && unit.kind == UnitKind::Shared),
        "a book against itself must be entirely Unchanged"
    );
    let edits = to_edits(&skeleton, &Decisions::new(), MergeSide::Current).unwrap();
    assert!(edits.is_empty(), "identity must produce zero edits");
}

#[test]
fn a_deleted_a_duplicated_and_a_reordered_verse_all_round_trip() {
    let source = read(MRK_ULB);

    let deleted = drop_a_verse(&source);
    let skeleton = check_pair("MRK verse deleted", &source, &deleted);
    assert_eq!(
        skeleton
            .units
            .iter()
            .filter(|unit| unit.kind == UnitKind::Deleted)
            .count(),
        1
    );

    let duplicated = duplicate_a_verse(&source);
    let skeleton = check_pair("MRK verse duplicated", &source, &duplicated);
    let added = skeleton
        .units
        .iter()
        .filter(|unit| unit.kind == UnitKind::Added)
        .collect::<Vec<_>>();
    assert_eq!(added.len(), 1);
    // The clone's dup context narrates the cross-document count.
    assert_eq!(added[0].dup_context.current_count, 2);
    assert_eq!(added[0].dup_context.baseline_count, 1);

    let reordered = reorder_two_verses(&source);
    let skeleton = check_pair("MRK verses reordered", &source, &reordered);
    let moved: Vec<_> = skeleton
        .units
        .iter()
        .filter(|unit| unit.status == Status::Moved)
        .collect();
    assert_eq!(moved.len(), 1, "a swap is one moved unit");
    // Reverting the one move restores the baseline byte-for-byte.
    let mut decisions = Decisions::new();
    decisions.insert(moved[0].id.clone(), MergeSide::Baseline);
    assert_eq!(
        merge(
            &skeleton,
            source.as_bytes(),
            reordered.as_bytes(),
            &decisions,
            MergeSide::Current
        )
        .unwrap(),
        source.as_bytes()
    );
}

#[test]
fn a_real_doubled_verse_carries_dup_context() {
    // bdf_reg ROM 3 really does write `\v 10` twice.
    let source = read(ROM_BDF);
    let skeleton = check_pair("bdf ROM self", &source, &source);
    let doubled: Vec<_> = skeleton
        .units
        .iter()
        .filter(|unit| {
            unit.baseline_addr
                .is_some_and(|addr| addr.chapter == 3 && addr.first == 10)
        })
        .collect();
    assert_eq!(doubled.len(), 2, "ROM 3 has two `\\v 10` blocks");
    assert_eq!(
        doubled[1].baseline_addr.unwrap().to_string(),
        "ROM 3:10_dup_1"
    );
    for unit in doubled {
        assert_eq!(unit.dup_context.baseline_count, 2);
        assert_eq!(unit.dup_context.current_count, 2);
        assert!(unit.dup_context.is_dup());
    }
}

// ---- the exhaustive gate -------------------------------------------------

fn collect_usfm(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_usfm(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "usfm") {
            out.push(path);
        }
    }
}

#[test]
#[ignore = "exhaustive corpus gate (minutes); run with `cargo test -- --include-ignored`"]
fn every_corpus_book_round_trips_against_its_twin_and_its_mutations() {
    let mut paths = Vec::new();
    collect_usfm(Path::new("example-corpora"), &mut paths);
    paths.sort();
    assert!(!paths.is_empty(), "expected corpus fixtures");

    let mut checked = 0usize;
    for path in &paths {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let label = path.to_string_lossy().into_owned();
        check_pair(&format!("{label} self"), &source, &source);
        check_pair(
            &format!("{label} verse deleted"),
            &source,
            &drop_a_verse(&source),
        );
        check_pair(
            &format!("{label} verse duplicated"),
            &source,
            &duplicate_a_verse(&source),
        );
        check_pair(
            &format!("{label} verses reordered"),
            &source,
            &reorder_two_verses(&source),
        );

        // The same book in the other translation, where there is one.
        if let Some(twin) = path
            .to_str()
            .filter(|p| p.contains("/en_ulb/"))
            .map(|p| p.replace("/en_ulb/", "/en_ult/"))
            && let Ok(other) = std::fs::read_to_string(&twin)
        {
            check_pair(&format!("{label} vs {twin}"), &source, &other);
        }
        checked += 1;
    }
    assert!(checked > 100, "expected the whole corpus, saw {checked}");
}
