//! The 23-case merge-interleave catalog, ported from `usfm_onion`'s
//! `src/diff/skeleton_fixtures.rs` (itself a port of the JS prototype's
//! `cases.js`). These encode the TRUSTED behaviour: the narration here must
//! read the same as onion's before any new behaviour is asserted anywhere.
//!
//! Onion diffs token vectors and reassembles by concatenating token text; this
//! port diffs BYTE RANGES, so every "reassembles" assertion below is byte-exact
//! against the source itself.

use std::collections::BTreeMap;

use usfm_onion_2::diff::{
    CoveredSide, Decisions, DiffSkeleton, MergeError, MergeSide, SlotRole, Status, TextDiffMode,
    UnitKind, diff, diff_with_text, merge, revert, to_edits,
};
use usfm_onion_2::edit::apply_splices;

struct Case {
    n: u32,
    baseline: &'static str,
    current: &'static str,
}

const CASES: &[Case] = &[
    Case {
        n: 1,
        baseline: "\\c 1\n\\v 1 In the beginning God created the heaven and the earth.\n",
        current: "\\c 1\n\\v 1 In the beginning God created the heavens and the earth.\n",
    },
    Case {
        n: 2,
        baseline: "\\c 1\n\\v 1 one\n\\v 3 three\n",
        current: "\\c 1\n\\v 1 one\n\\v 2 two\n\\v 3 three\n",
    },
    Case {
        n: 3,
        baseline: "\\c 1\n\\v 2 two\n\\v 3 three\n",
        current: "\\c 1\n\\v 1 one\n\\v 2 two\n\\v 3 three\n",
    },
    Case {
        n: 4,
        baseline: "\\c 1\n\\v 1 one\n\\v 2 two\n",
        current: "\\c 1\n\\v 1 one\n\\v 2 two\n\\v 3 three\n",
    },
    Case {
        n: 5,
        baseline: "\\c 1\n\\v 1 one\n\\v 2 two\n\\v 3 three\n",
        current: "\\c 1\n\\v 1 one\n\\v 3 three\n",
    },
    Case {
        n: 6,
        baseline: "\\c 1\n\\v 1 The old rendering of this verse.\n",
        current: "\\c 1\n\\v 1 A completely different rendering here.\n",
    },
    Case {
        n: 7,
        baseline: "\\c 1\n\\v 1 one\n",
        current: "\\c 1\n\\v 1 one  \n",
    },
    Case {
        n: 8,
        baseline: "\\c 1\n\\p\n\\v 1 one\n\\s Section\n\\v 2 two\n",
        current: "\\c 1\n\\m\n\\v 1 one\n\\v 2 two\n",
    },
    Case {
        n: 9,
        baseline: "\\c 1\n\\v 1 Alpha beta gamma.\n\\v 2 Delta epsilon.\n",
        current: "\\c 1\n\\v 1 Alpha beta.\n\\v 2 Gamma delta epsilon.\n",
    },
    Case {
        n: 10,
        baseline: "\\c 1\n\\v 1 First verse.\n\\v 2 Second verse.\n",
        current: "\\c 1\n\\v 2 Second verse.\n\\v 1 First verse.\n",
    },
    Case {
        n: 11,
        baseline: "\\c 1\n\\v 1 one\n\\v 2 two\n\\v 3 three\n",
        current: "\\c 1\n\\v 1 one\n\\v 3 three\n\\v 2 two\n",
    },
    Case {
        n: 12,
        baseline: "\\c 1\n\\v 1 a\n\\v 2 b\n\\v 1 c\n",
        current: "\\c 1\n\\v 1 a\n\\v 2 b\n\\v 1 c edited\n",
    },
    Case {
        n: 13,
        baseline: "\\c 1\n\\v 1 a\n\\v 2 b\n\\v 1 c\n",
        current: "\\c 1\n\\v 2 b\n\\v 1 c\n",
    },
    Case {
        n: 14,
        baseline: "\\c 1\n\\v 1 a\n\\v 1-2 b\n",
        current: "\\c 1\n\\v 1 a\n\\v 1-2 b edited\n",
    },
    Case {
        n: 15,
        baseline: "\\c 1\n\\v 1 a\n\\v 2 b\n",
        current: "\\c 1\n\\v 1-2 a b\n",
    },
    Case {
        n: 16,
        baseline: "\\c 1\n\\v 1-3 a\n",
        current: "\\c 1\n\\v 1-2 a\n\\v 3 b\n",
    },
    Case {
        n: 17,
        baseline: "\\c 1\n\\v 1-2 a\n\\v 2 b\n",
        current: "\\c 1\n\\v 1-2 a\n\\v 2 b changed\n",
    },
    Case {
        n: 18,
        baseline: "\\c 1\n\\v 1 Text\\f + \\ft a note\\f* more.\n",
        current: "\\c 1\n\\v 1 Text\\f + \\ft an edited note\\f* more.\n",
    },
    Case {
        n: 19,
        baseline: "\\id GEN\n\\h Genesis\n\\c 1\n\\v 1 one\n",
        current: "\\id GEN\n\\h The Book of Genesis\n\\c 1\n\\v 1 one\n",
    },
    Case {
        n: 20,
        baseline: "\\c 1\r\n\\v 1 one\r\n\\v 2 two\r\n",
        current: "\\c 1\n\\v 1 one\n\\v 2 two\n",
    },
    Case {
        n: 21,
        baseline: "\\c 5\n\\v 10 Something entirely.\n\\v 11 Unrelated content here.\n",
        current: "\\c 9\n\\v 1 Totally different text.\n\\v 2 Nothing shared at all.\n",
    },
    Case {
        n: 22,
        baseline: "\\c 1\n\\v 1 Alpha beta gamma.\n\\v 2 To be deleted.\n\\v 3 Delta epsilon.\n",
        current: "\\c 1\n\\v 1 Alpha beta.\n\\v 3 Gamma delta epsilon.\n",
    },
    Case {
        n: 23,
        baseline: "\\c 1\n\\v 1 a\n\\v 2 b\n\\v 10 j\n\\v 11 k\n",
        current: "\\c 1\n\\v 1 a\n\\v 2 b\n\\v 10 j\n\\v 1 k\n",
    },
];

/// Case 19 brings its own `\id`; every other body is wrapped with one.
fn wrap(n: u32, body: &str) -> String {
    if n == 19 {
        body.to_string()
    } else {
        format!("\\id GEN\n{body}")
    }
}

fn sources(case: &Case) -> (String, String) {
    (wrap(case.n, case.baseline), wrap(case.n, case.current))
}

fn skeleton_for(n: u32) -> (DiffSkeleton, String, String) {
    let case = CASES.iter().find(|c| c.n == n).expect("case exists");
    let (baseline, current) = sources(case);
    let skeleton = diff(&baseline, &current);
    (skeleton, baseline, current)
}

fn text<'a>(source: &'a str, range: &std::ops::Range<u32>) -> &'a str {
    &source[range.start as usize..range.end as usize]
}

fn baseline_addr(skeleton: &DiffSkeleton, unit: usize) -> Option<String> {
    skeleton.units[unit].baseline_addr.map(|a| a.to_string())
}

fn current_addr(skeleton: &DiffSkeleton, unit: usize) -> Option<String> {
    skeleton.units[unit].current_addr.map(|a| a.to_string())
}

fn only(skeleton: &DiffSkeleton, kind: UnitKind) -> &usfm_onion_2::diff::DecisionUnit {
    let mut found = skeleton.units.iter().filter(|unit| unit.kind == kind);
    let unit = found.next().unwrap_or_else(|| panic!("no {kind:?} unit"));
    assert!(found.next().is_none(), "expected exactly one {kind:?} unit");
    unit
}

fn count(skeleton: &DiffSkeleton, kind: UnitKind) -> usize {
    skeleton
        .units
        .iter()
        .filter(|unit| unit.kind == kind)
        .count()
}

/// The partition law at fixture grain: every slot's bearing side concatenates,
/// in slot order, back into the source it came from — so every byte of both
/// inputs lives in exactly one bearing slot.
fn assert_partition_reproduces_sources(skeleton: &DiffSkeleton, baseline: &str, current: &str) {
    let mut baseline_out = String::new();
    let mut current_out = String::new();
    let mut seen_baseline = Vec::new();
    let mut seen_current = Vec::new();

    for slot in &skeleton.slots {
        let unit = &skeleton.units[slot.unit as usize];
        let bears_baseline = matches!(
            slot.role,
            SlotRole::Shared | SlotRole::BaselineOnly | SlotRole::PairBaseline
        );
        let bears_current = matches!(
            slot.role,
            SlotRole::Shared | SlotRole::CurrentOnly | SlotRole::PairCurrent
        );
        if bears_baseline {
            baseline_out.push_str(text(baseline, &unit.baseline));
            assert!(!seen_baseline.contains(&slot.unit), "two baseline slots");
            seen_baseline.push(slot.unit);
        }
        if bears_current {
            current_out.push_str(text(current, &unit.current));
            assert!(!seen_current.contains(&slot.unit), "two current slots");
            seen_current.push(slot.unit);
        }
    }

    assert_eq!(baseline_out, baseline, "baseline reassembly mismatch");
    assert_eq!(current_out, current, "current reassembly mismatch");
}

#[test]
fn all_23_cases_partition_and_reassemble() {
    for case in CASES {
        let (baseline, current) = sources(case);
        let skeleton = diff(&baseline, &current);
        assert_partition_reproduces_sources(&skeleton, &baseline, &current);
    }
}

#[test]
fn case_7_whitespace_only_change_is_flagged_and_not_usfm_structure() {
    let (skeleton, ..) = skeleton_for(7);
    let modified = skeleton
        .units
        .iter()
        .find(|unit| unit.status == Status::Modified)
        .expect("one modified unit");
    assert!(modified.is_whitespace_change);
    assert!(!modified.is_usfm_structure_change);
}

#[test]
fn case_8_paragraph_marker_change_is_usfm_structure_only() {
    let (skeleton, ..) = skeleton_for(8);
    let chapter_open = skeleton
        .units
        .iter()
        .position(|unit| unit.baseline_addr.map(|a| a.to_string()).as_deref() == Some("GEN 1:0"))
        .expect("chapter-open unit");
    assert!(skeleton.units[chapter_open].is_usfm_structure_change);
    assert!(!skeleton.units[chapter_open].is_whitespace_change);

    // The v1 heading-removal unit is a real content change: neither flag.
    let verse_1 = skeleton
        .units
        .iter()
        .position(|unit| unit.baseline_addr.map(|a| a.to_string()).as_deref() == Some("GEN 1:1"))
        .expect("verse 1 unit");
    assert!(!skeleton.units[verse_1].is_whitespace_change);
    assert!(!skeleton.units[verse_1].is_usfm_structure_change);
}

#[test]
fn case_10_moved_unit_spans_exactly_two_linked_slots_in_document_order() {
    let (skeleton, ..) = skeleton_for(10);
    let moved = only(&skeleton, UnitKind::Coalesced);
    assert_eq!(moved.status, Status::Moved);
    let moved_id = moved.id.clone();

    let baseline_order: Vec<Option<String>> = skeleton
        .slots
        .iter()
        .filter(|slot| matches!(slot.role, SlotRole::Shared | SlotRole::PairBaseline))
        .map(|slot| baseline_addr(&skeleton, slot.unit as usize))
        .collect();
    assert_eq!(
        baseline_order,
        ["GEN 0:0", "GEN 1:0", "GEN 1:1", "GEN 1:2"]
            .map(|s| Some(s.to_string()))
            .to_vec()
    );

    let current_order: Vec<Option<String>> = skeleton
        .slots
        .iter()
        .filter(|slot| matches!(slot.role, SlotRole::Shared | SlotRole::PairCurrent))
        .map(|slot| current_addr(&skeleton, slot.unit as usize))
        .collect();
    assert_eq!(
        current_order,
        ["GEN 0:0", "GEN 1:0", "GEN 1:2", "GEN 1:1"]
            .map(|s| Some(s.to_string()))
            .to_vec()
    );

    // One decision, two ghosts.
    let slots = skeleton
        .slots
        .iter()
        .filter(|slot| skeleton.units[slot.unit as usize].id == moved_id)
        .count();
    assert_eq!(slots, 2);
}

#[test]
fn case_13_full_narration() {
    let (skeleton, baseline, current) = skeleton_for(13);

    let deleted = only(&skeleton, UnitKind::Deleted);
    // 13.1: the deleted unit is the 'a'-side content.
    assert!(text(&baseline, &deleted.baseline).ends_with("a\n"));

    let survivor = only(&skeleton, UnitKind::Coalesced);
    // 13.2: survivor 'c' pairs as unchanged (a sid relabel only)...
    assert_eq!(survivor.status, Status::Unchanged);
    // 13.14: ...which is exactly what `relabeled` narrates.
    assert!(survivor.relabeled);
    assert_eq!(
        survivor.baseline_addr.map(|a| a.to_string()).as_deref(),
        Some("GEN 1:1_dup_1")
    );
    assert_eq!(
        survivor.current_addr.map(|a| a.to_string()).as_deref(),
        Some("GEN 1:1")
    );
    assert!(text(&current, &survivor.current).ends_with("c\n"));

    // 13.15: the deleted 'a' carries dup context — 2x baseline, 1x current.
    assert_eq!(deleted.dup_context.baseline_count, 2);
    assert_eq!(deleted.dup_context.current_count, 1);
    assert!(deleted.dup_context.is_dup());

    // 13.3: the deleted 'a' is anchored after GEN 1:0.
    let deleted_id = deleted.id.clone();
    let slot = skeleton
        .slots
        .iter()
        .find(|slot| skeleton.units[slot.unit as usize].id == deleted_id)
        .expect("the deleted unit owns a slot");
    let anchor = slot.after.expect("the deleted unit has an anchor");
    assert_eq!(
        current_addr(&skeleton, anchor.unit as usize).as_deref(),
        Some("GEN 1:0")
    );

    // 13.4: no two anchored hunks fight over one anchor.
    let mut seen = Vec::new();
    let mut anchored = Vec::new();
    for slot in &skeleton.slots {
        if seen.contains(&slot.unit) {
            continue;
        }
        seen.push(slot.unit);
        let unit = &skeleton.units[slot.unit as usize];
        let needs_anchor =
            unit.status == Status::Deleted || (unit.kind == UnitKind::Coalesced && unit.displaced);
        if let (true, Some(anchor)) = (needs_anchor, slot.after) {
            let sid = match anchor.side {
                MergeSide::Baseline => baseline_addr(&skeleton, anchor.unit as usize),
                MergeSide::Current => current_addr(&skeleton, anchor.unit as usize),
            };
            assert!(
                !anchored.contains(&sid),
                "two anchored hunks share an anchor"
            );
            anchored.push(sid);
        }
    }
}

#[test]
fn case_15_verses_merged_into_a_bridge() {
    let (skeleton, ..) = skeleton_for(15);

    let pair = only(&skeleton, UnitKind::Coalesced);
    // 15.1: the pair exposes both sids.
    assert_eq!(
        pair.baseline_addr.map(|a| a.to_string()).as_deref(),
        Some("GEN 1:1")
    );
    assert_eq!(
        pair.current_addr.map(|a| a.to_string()).as_deref(),
        Some("GEN 1:1-2")
    );

    let deleted = only(&skeleton, UnitKind::Deleted);
    assert_eq!(
        deleted.baseline_addr.map(|a| a.to_string()).as_deref(),
        Some("GEN 1:2")
    );

    // 15.2 + 15.3: the deleted v2 is anchored after the PAIR's baseline sid,
    // not after the shared chapter open.
    let pair_id = pair.id.clone();
    let deleted_id = deleted.id.clone();
    let deleted_slot = skeleton
        .slots
        .iter()
        .position(|slot| skeleton.units[slot.unit as usize].id == deleted_id)
        .unwrap();
    let anchor = skeleton.slots[deleted_slot]
        .after
        .expect("the deleted v2 has an anchor");
    assert_eq!(anchor.side, MergeSide::Baseline);
    assert_eq!(
        baseline_addr(&skeleton, anchor.unit as usize).as_deref(),
        Some("GEN 1:1")
    );
    assert_eq!(skeleton.units[anchor.unit as usize].id, pair_id);

    let pair_slot = skeleton
        .slots
        .iter()
        .position(|slot| {
            skeleton.units[slot.unit as usize].id == pair_id && slot.role == SlotRole::PairBaseline
        })
        .expect("the pair has a baseline slot");
    assert!(pair_slot < deleted_slot);

    // 15.4 (pin 20): the deleted GEN 1:2 is covered by the pair's 1-2 bridge.
    let covered = deleted.covered_by.expect("the deleted v2 is covered");
    assert_eq!(skeleton.units[covered.unit as usize].id, pair_id);
    assert_eq!(covered.side, CoveredSide::Current);
    assert_eq!(covered.addr.to_string(), "GEN 1:1-2");
}

#[test]
fn case_16_bridge_split_added_verse_is_covered_by_the_baseline_bridge() {
    let (skeleton, ..) = skeleton_for(16);

    let pair = only(&skeleton, UnitKind::Coalesced);
    assert_eq!(
        pair.baseline_addr.map(|a| a.to_string()).as_deref(),
        Some("GEN 1:1-3")
    );
    assert_eq!(
        pair.current_addr.map(|a| a.to_string()).as_deref(),
        Some("GEN 1:1-2")
    );

    let added = only(&skeleton, UnitKind::Added);
    assert_eq!(
        added.current_addr.map(|a| a.to_string()).as_deref(),
        Some("GEN 1:3")
    );

    // pin 21: the added GEN 1:3 was covered by the baseline 1-3 bridge.
    let covered = added.covered_by.expect("the added v3 is covered");
    assert_eq!(skeleton.units[covered.unit as usize].id, pair.id);
    assert_eq!(covered.side, CoveredSide::Baseline);
    assert_eq!(covered.addr.to_string(), "GEN 1:1-3");
}

#[test]
fn covered_by_never_annotates_a_one_sided_bridge_against_an_opposite_bridge() {
    // Two off-Myers baseline bridges share the pairing key "1:1"; tier 1 pairs
    // the byte-identical one, leaving `\v 1-2 B` deleted. Its OWN sid is a
    // bridge, so covered_by must not annotate it — covered_by is for a one-sided
    // VERSE, never a one-sided bridge.
    let baseline = "\\id GEN\n\\c 1\n\\v 1-3 A\n\\v 1-2 B\n";
    let current = "\\id GEN\n\\c 1\n\\v 1-4 A\n";
    let skeleton = diff(baseline, current);

    let pair = only(&skeleton, UnitKind::Coalesced);
    assert_eq!(
        pair.baseline_addr.map(|a| a.to_string()).as_deref(),
        Some("GEN 1:1-3")
    );
    assert_eq!(
        pair.current_addr.map(|a| a.to_string()).as_deref(),
        Some("GEN 1:1-4")
    );

    let deleted = only(&skeleton, UnitKind::Deleted);
    assert_eq!(
        deleted.baseline_addr.map(|a| a.to_string()).as_deref(),
        Some("GEN 1:1-2")
    );
    assert!(deleted.covered_by.is_none());
}

#[test]
fn case_23_renumber_typo_never_coalesces_across_keys() {
    let (skeleton, baseline, current) = skeleton_for(23);

    let deleted = only(&skeleton, UnitKind::Deleted);
    assert_eq!(
        deleted.baseline_addr.map(|a| a.to_string()).as_deref(),
        Some("GEN 1:11")
    );
    assert!(text(&baseline, &deleted.baseline).ends_with("k\n"));

    let added = only(&skeleton, UnitKind::Added);
    assert_eq!(
        added.current_addr.map(|a| a.to_string()).as_deref(),
        Some("GEN 1:1_dup_1")
    );
    assert!(text(&current, &added.current).ends_with("k\n"));

    // Content similarity across DIFFERENT verse numbers must never pair.
    assert_eq!(count(&skeleton, UnitKind::Coalesced), 0);
}

#[test]
fn case_11_three_verses_one_displaced_has_exactly_one_moved_unit() {
    let (skeleton, ..) = skeleton_for(11);
    assert_eq!(count(&skeleton, UnitKind::Coalesced), 1);
    assert_eq!(only(&skeleton, UnitKind::Coalesced).status, Status::Moved);
}

#[test]
fn case_13_pairs_exact_text_before_positional_leftovers() {
    let (skeleton, _, current) = skeleton_for(13);
    let survivor = only(&skeleton, UnitKind::Coalesced);
    // Survivor 'c' must exact-pair rather than mis-pair positionally with 'a'.
    assert!(text(&current, &survivor.current).ends_with("c\n"));
}

#[test]
fn a_repeated_chapter_keeps_its_pairing_key_parseable() {
    // `_cdup_N` rides in the verse segment so the chapter segment stays a bare
    // integer. If that regressed, every block of the second chapter occurrence
    // would land in one degenerate bucket and pair with unrelated content.
    let source = "\\id GEN\n\\c 1\n\\v 1 first\n\\c 1\n\\v 1 second\n";
    let skeleton = diff(source, source);
    let addrs: Vec<String> = skeleton
        .units
        .iter()
        .filter_map(|unit| unit.baseline_addr.map(|a| a.to_string()))
        .collect();
    assert_eq!(
        addrs,
        vec![
            "GEN 0:0",
            "GEN 1:0",
            "GEN 1:1",
            "GEN 1:0_cdup_1",
            "GEN 1:1_cdup_1",
        ]
    );
    // The two `\v 1`s share a pairing key across the chapter occurrences, which
    // is what dup_context counts.
    let verse_units: Vec<_> = skeleton
        .units
        .iter()
        .filter(|unit| {
            unit.baseline_addr
                .is_some_and(|addr| addr.first == 1 && addr.chapter == 1)
        })
        .collect();
    assert_eq!(verse_units.len(), 2);
    for unit in verse_units {
        assert_eq!(unit.dup_context.baseline_count, 2);
        assert_eq!(unit.dup_context.current_count, 2);
    }
}

#[test]
fn a_repeated_chapter_resets_verse_duplicate_counting() {
    let source = "\\id GEN\n\\c 1\n\\v 1 a\n\\v 1 b\n\\c 1\n\\v 1 c\n\\v 1 d\n";
    let skeleton = diff(source, source);
    let addrs: Vec<String> = skeleton
        .units
        .iter()
        .filter_map(|unit| unit.baseline_addr.map(|a| a.to_string()))
        .collect();
    assert_eq!(
        addrs,
        vec![
            "GEN 0:0",
            "GEN 1:0",
            "GEN 1:1",
            "GEN 1:1_dup_1",
            "GEN 1:0_cdup_1",
            "GEN 1:1_cdup_1",
            "GEN 1:1_cdup_1_dup_1",
        ]
    );
}

#[test]
fn a_bridge_and_a_single_verse_never_share_a_dup_counter() {
    let source = "\\id GEN\n\\c 1\n\\v 1 a\n\\v 1-2 b\n\\v 1-2 c\n";
    let skeleton = diff(source, source);
    let addrs: Vec<String> = skeleton
        .units
        .iter()
        .filter_map(|unit| unit.baseline_addr.map(|a| a.to_string()))
        .collect();
    assert_eq!(
        addrs,
        vec![
            "GEN 0:0",
            "GEN 1:0",
            "GEN 1:1",
            "GEN 1:1-2",
            "GEN 1:1-2_dup_1",
        ]
    );
}

// ---- pure merge, strict revert -------------------------------------------

#[test]
fn all_23_cases_merge_all_baseline_and_all_current_byte_exactly() {
    // Case 20 (CRLF baseline vs LF current) rides in here: identity must stay
    // byte-exact regardless of the line-ending mix.
    for case in CASES {
        let (baseline, current) = sources(case);
        let skeleton = diff(&baseline, &current);
        let empty = Decisions::new();

        let all_baseline = merge(
            &skeleton,
            baseline.as_bytes(),
            current.as_bytes(),
            &empty,
            MergeSide::Baseline,
        )
        .unwrap();
        let all_current = merge(
            &skeleton,
            baseline.as_bytes(),
            current.as_bytes(),
            &empty,
            MergeSide::Current,
        )
        .unwrap();

        assert_eq!(all_baseline, baseline.as_bytes(), "case {}", case.n);
        assert_eq!(all_current, current.as_bytes(), "case {}", case.n);
    }
}

/// Deterministic LCG — reproducible, no rand dependency.
fn lcg(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
    *seed
}

/// Written independently of the merge's own slot walk, so a wrong-side
/// emission, a duplicated slot, or a leaked unchosen side shows up as a byte
/// mismatch rather than a self-consistent recount.
fn expected_merge(
    skeleton: &DiffSkeleton,
    baseline: &str,
    current: &str,
    decisions: &Decisions,
    default_side: MergeSide,
) -> String {
    let mut out = String::new();
    for slot in &skeleton.slots {
        let unit = &skeleton.units[slot.unit as usize];
        let side = decisions.get(&unit.id).copied().unwrap_or(default_side);
        match slot.role {
            SlotRole::Shared => out.push_str(match side {
                MergeSide::Baseline => text(baseline, &unit.baseline),
                MergeSide::Current => text(current, &unit.current),
            }),
            SlotRole::BaselineOnly | SlotRole::PairBaseline => {
                if side == MergeSide::Baseline {
                    out.push_str(text(baseline, &unit.baseline));
                }
            }
            SlotRole::CurrentOnly | SlotRole::PairCurrent => {
                if side == MergeSide::Current {
                    out.push_str(text(current, &unit.current));
                }
            }
        }
    }
    out
}

#[test]
fn all_23_cases_random_decision_vectors_merge_and_replay_identically() {
    for case in CASES {
        let (baseline, current) = sources(case);
        let skeleton = diff(&baseline, &current);
        let mut seed = 0x9e37_79b9u32 ^ case.n.wrapping_mul(2_654_435_761);

        for _trial in 0..24 {
            let mut decisions = Decisions::new();
            for unit in &skeleton.units {
                match lcg(&mut seed) % 5 {
                    0 | 1 => {
                        decisions.insert(unit.id.clone(), MergeSide::Baseline);
                    }
                    2 | 3 => {
                        decisions.insert(unit.id.clone(), MergeSide::Current);
                    }
                    // else absent, exercising default_side.
                    _ => {}
                }
            }
            let default_side = if lcg(&mut seed).is_multiple_of(2) {
                MergeSide::Baseline
            } else {
                MergeSide::Current
            };

            let merged = merge(
                &skeleton,
                baseline.as_bytes(),
                current.as_bytes(),
                &decisions,
                default_side,
            )
            .unwrap();
            let again = merge(
                &skeleton,
                baseline.as_bytes(),
                current.as_bytes(),
                &decisions,
                default_side,
            )
            .unwrap();
            assert_eq!(merged, again, "case {}: merge is not pure", case.n);
            assert_eq!(
                String::from_utf8(merged.clone()).unwrap(),
                expected_merge(&skeleton, &baseline, &current, &decisions, default_side),
                "case {}: merge must equal the independently assembled chosen-side text",
                case.n
            );

            // The replay artifact says the same thing in splices.
            let edits = to_edits(&skeleton, &decisions, default_side).unwrap();
            assert_eq!(
                apply_splices(baseline.as_bytes(), current.as_bytes(), &edits),
                merged,
                "case {}: to_edits + apply_splices must equal merge",
                case.n
            );
        }
    }
}

#[test]
fn every_cases_round_trip_is_byte_exact_in_both_directions() {
    for case in CASES {
        let (baseline, current) = sources(case);
        let skeleton = diff(&baseline, &current);
        let empty = Decisions::new();

        let to_current = to_edits(&skeleton, &empty, MergeSide::Current).unwrap();
        assert_eq!(
            apply_splices(baseline.as_bytes(), current.as_bytes(), &to_current),
            current.as_bytes(),
            "case {}: all-current replay must reproduce current",
            case.n
        );

        let to_baseline = to_edits(&skeleton, &empty, MergeSide::Baseline).unwrap();
        assert!(
            to_baseline.is_empty(),
            "case {}: the all-baseline replay is the identity, so it has no edits",
            case.n
        );
        assert_eq!(
            apply_splices(baseline.as_bytes(), current.as_bytes(), &to_baseline),
            baseline.as_bytes(),
        );
    }
}

#[test]
fn single_revert_of_every_changed_unit_equals_a_one_decision_merge() {
    for case in CASES {
        let (baseline, current) = sources(case);
        let skeleton = diff(&baseline, &current);

        for unit in skeleton
            .units
            .iter()
            .filter(|unit| unit.status != Status::Unchanged)
        {
            let reverted = revert(&skeleton, baseline.as_bytes(), current.as_bytes(), &unit.id)
                .expect("a real unit id");
            let mut decisions = Decisions::new();
            decisions.insert(unit.id.clone(), MergeSide::Baseline);
            let merged = merge(
                &skeleton,
                baseline.as_bytes(),
                current.as_bytes(),
                &decisions,
                MergeSide::Current,
            )
            .unwrap();
            assert_eq!(reverted, merged, "case {}: revert({})", case.n, unit.id);
        }
    }
}

#[test]
fn an_unknown_unit_id_is_rejected_with_zero_output() {
    let (skeleton, baseline, current) = skeleton_for(13);
    let mut decisions = Decisions::new();
    decisions.insert("__no_such_unit__".to_string(), MergeSide::Current);

    let merged = merge(
        &skeleton,
        baseline.as_bytes(),
        current.as_bytes(),
        &decisions,
        MergeSide::Current,
    );
    assert_eq!(
        merged,
        Err(MergeError::UnknownUnitId("__no_such_unit__".to_string()))
    );
    assert_eq!(
        merged.unwrap_err().to_string(),
        "unknown decision unit id: __no_such_unit__"
    );
    assert_eq!(
        to_edits(&skeleton, &decisions, MergeSide::Current),
        Err(MergeError::UnknownUnitId("__no_such_unit__".to_string()))
    );
    // A stale id poisons the whole transaction, even mixed with real ones.
    decisions.insert(skeleton.units[0].id.clone(), MergeSide::Baseline);
    assert!(
        merge(
            &skeleton,
            baseline.as_bytes(),
            current.as_bytes(),
            &decisions,
            MergeSide::Current
        )
        .is_err()
    );
    assert!(revert(&skeleton, baseline.as_bytes(), current.as_bytes(), "nope").is_err());
}

#[test]
fn unit_ids_are_unique_and_decisions_are_a_plain_string_map() {
    for case in CASES {
        let (baseline, current) = sources(case);
        let skeleton = diff(&baseline, &current);
        let mut ids: Vec<&str> = skeleton.units.iter().map(|unit| unit.id.as_str()).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "case {}: duplicate unit id", case.n);
    }
    // The consumer contract: `{unitId: side}`, nothing else.
    let _: BTreeMap<String, MergeSide> = Decisions::new();
}

// ---- intra-unit text diff -------------------------------------------------

#[test]
fn text_diff_is_status_gated_and_reconstructs_each_side() {
    for case in CASES {
        let (baseline, current) = sources(case);
        let (skeleton, texts) = diff_with_text(&baseline, &current, TextDiffMode::Words);
        assert_eq!(skeleton.units.len(), texts.len());

        for (unit, diff) in skeleton.units.iter().zip(&texts) {
            match unit.status {
                Status::Unchanged | Status::Moved => {
                    assert!(
                        diff.is_none(),
                        "case {}: a pure move must not highlight",
                        case.n
                    )
                }
                _ => {
                    let diff = diff.as_ref().expect("a changed unit has runs");
                    // The runs are one-sided by kind.
                    assert!(
                        diff.baseline
                            .iter()
                            .all(|run| run.kind != usfm_onion_2::diff::RunKind::Added)
                    );
                    assert!(
                        diff.current
                            .iter()
                            .all(|run| run.kind != usfm_onion_2::diff::RunKind::Removed)
                    );
                }
            }
        }
    }
}

#[test]
fn requesting_a_text_diff_never_perturbs_the_skeleton() {
    for case in CASES {
        let (baseline, current) = sources(case);
        let plain = diff(&baseline, &current);
        for mode in [TextDiffMode::None, TextDiffMode::Words, TextDiffMode::Chars] {
            let (with_text, _) = diff_with_text(&baseline, &current, mode);
            assert_eq!(plain, with_text, "case {}: {mode:?}", case.n);
        }
    }
}

#[test]
fn a_modified_unit_splits_into_word_runs_that_rebuild_the_reader_text() {
    let (skeleton, texts) = diff_with_text(
        &wrap(1, CASES[0].baseline),
        &wrap(1, CASES[0].current),
        TextDiffMode::Words,
    );
    let index = skeleton
        .units
        .iter()
        .position(|unit| unit.status == Status::Modified)
        .expect("case 1 modifies one verse");
    let diff = texts[index].as_ref().unwrap();
    let baseline: String = diff.baseline.iter().map(|run| run.text.as_str()).collect();
    let current: String = diff.current.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(
        baseline,
        "In the beginning God created the heaven and the earth.\n"
    );
    assert_eq!(
        current,
        "In the beginning God created the heavens and the earth.\n"
    );
    // The changed word is the only difference, and it lands as one run a side.
    let removed: Vec<&str> = diff
        .baseline
        .iter()
        .filter(|run| run.kind == usfm_onion_2::diff::RunKind::Removed)
        .map(|run| run.text.as_str())
        .collect();
    let added: Vec<&str> = diff
        .current
        .iter()
        .filter(|run| run.kind == usfm_onion_2::diff::RunKind::Added)
        .map(|run| run.text.as_str())
        .collect();
    assert_eq!(removed, vec!["heaven"]);
    assert_eq!(added, vec!["heavens"]);
}

#[test]
fn a_char_marker_wrap_is_not_glued_with_a_synthetic_space() {
    // `word\add ed\add*` reads "worded", never "word ed": the markers drop and
    // nothing is inserted in their place.
    let baseline = "\\id GEN\n\\c 1\n\\v 1 word\\add ed\\add*\n";
    let current = "\\id GEN\n\\c 1\n\\v 1 word\\add ing\\add*\n";
    let (skeleton, texts) = diff_with_text(baseline, current, TextDiffMode::Words);
    let index = skeleton
        .units
        .iter()
        .position(|unit| unit.status == Status::Modified)
        .expect("one modified verse");
    let diff = texts[index].as_ref().unwrap();
    let baseline_text: String = diff.baseline.iter().map(|run| run.text.as_str()).collect();
    assert!(baseline_text.contains("worded"), "got {baseline_text:?}");
    assert!(!baseline_text.contains("word ed"));
    // The note prose the mask keeps is the reader's text; markup is not in it.
    assert!(!baseline_text.contains("add"));
}

#[test]
fn a_markup_only_change_reads_as_structure_with_equal_reader_text() {
    let baseline = "\\id GEN\n\\c 1\n\\v 1 a plain word\n";
    let current = "\\id GEN\n\\c 1\n\\v 1 a plain \\add word\\add*\n";
    let skeleton = diff(baseline, current);
    let unit = skeleton
        .units
        .iter()
        .find(|unit| unit.status == Status::Modified)
        .expect("one modified verse");
    assert!(unit.is_usfm_structure_change);
    assert!(!unit.is_whitespace_change);
}

#[test]
fn a_reformat_only_pair_is_whitespace_change_everywhere() {
    let baseline = "\\id GEN\n\\c 1\n\\p\n\\v 1 one\n\\v 2 two\n";
    let current = "\\id GEN\n\\c 1\n\\p \\v 1 one   \\v 2 two\n";
    let skeleton = diff(baseline, current);
    let changed: Vec<&usfm_onion_2::diff::DecisionUnit> = skeleton
        .units
        .iter()
        .filter(|unit| unit.status == Status::Modified)
        .collect();
    assert!(!changed.is_empty());
    for unit in changed {
        assert!(
            unit.is_whitespace_change,
            "{} is not flagged whitespace-only",
            unit.id
        );
        assert!(!unit.is_usfm_structure_change);
    }
}
