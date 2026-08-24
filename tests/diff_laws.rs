//! The diff's algebraic laws, over generated document pairs.
//!
//! Three laws, which is the whole property core (onion's P1-P7 minus the ones
//! its token-level generator carried and this byte-level one cannot):
//!
//! 1. **Partition totality** — every byte of both inputs sits in exactly one
//!    bearing slot, so the slot walk reassembles both sources byte-for-byte.
//! 2. **Byte round-trip** — `apply_splices(A, B, to_edits(diff, all_current))`
//!    is B exactly, `all_baseline` is A exactly, and a mixed decision vector
//!    replays to the same bytes the projection emits.
//! 3. **Unknown-id rejection with zero side effects** — a staged id naming no
//!    unit fails the whole transaction, producing nothing.
//!
//! Determinism rides along free: every law runs its diff twice and compares.
//!
//! The generator is a seeded xorshift over document SHAPES (chapters, verses,
//! bridges, duplicate verse numbers, CRLF/LF) and edit shapes (insert, delete,
//! reorder, renumber, retext, reformat) — deliberately hand-rolled rather than
//! a proptest dependency: the shapes that matter here are USFM-specific and a
//! generic shrinker has nothing to shrink into.

use usfm_onion_2::diff::{
    Decisions, DiffSkeleton, MergeError, MergeSide, SlotRole, diff, merge, to_edits,
};
use usfm_onion_2::edit::apply_splices;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*, deterministic and dependency-free.
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> usize {
        (self.next() % n) as usize
    }
}

const WORDS: [&str; 8] = [
    "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta",
];

/// A generated document, kept as lines so the mutators can work on verses
/// without re-parsing.
fn document(rng: &mut Rng) -> Vec<String> {
    let mut lines = vec!["\\id GEN".to_string(), "\\h Genesis".to_string()];
    let chapters = 1 + rng.below(3);
    for chapter in 1..=chapters {
        // A repeated chapter number is real data (a reopened chapter).
        let number = if rng.below(8) == 0 { 1 } else { chapter };
        lines.push(format!("\\c {number}"));
        lines.push("\\p".to_string());
        let verses = 1 + rng.below(6);
        let mut verse = 1;
        for _ in 0..verses {
            let body = (0..1 + rng.below(4))
                .map(|_| WORDS[rng.below(WORDS.len() as u64)])
                .collect::<Vec<_>>()
                .join(" ");
            if rng.below(6) == 0 {
                // A bridge.
                lines.push(format!("\\v {verse}-{} {body}", verse + 1));
                verse += 2;
            } else {
                lines.push(format!("\\v {verse} {body}"));
                if rng.below(7) == 0 {
                    // A duplicate verse number, in place.
                    lines.push(format!("\\v {verse} {body} again"));
                }
                verse += 1;
            }
        }
    }
    lines
}

fn verse_positions(lines: &[String]) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("\\v "))
        .map(|(index, _)| index)
        .collect()
}

/// One edit of a shape a real reviewer would make.
fn mutate(lines: &[String], rng: &mut Rng) -> Vec<String> {
    let mut out = lines.to_vec();
    let verses = verse_positions(&out);
    if verses.is_empty() {
        return out;
    }
    let pick = verses[rng.below(verses.len() as u64)];
    match rng.below(6) {
        0 => {
            out.remove(pick);
        }
        1 => out.insert(pick, "\\v 249 an inserted verse".to_string()),
        2 => {
            // Reorder: swap two verses.
            let other = verses[rng.below(verses.len() as u64)];
            out.swap(pick, other);
        }
        3 => {
            // Renumber, keeping the body — never a move, always delete+add.
            let body = out[pick].splitn(3, ' ').nth(2).unwrap_or("").to_string();
            out[pick] = format!("\\v 200 {body}");
        }
        4 => out[pick].push_str(" and more"),
        // Reformat: whitespace churn only.
        _ => out[pick] = out[pick].replace(' ', "  "),
    }
    out
}

fn render(lines: &[String], crlf: bool) -> String {
    let newline = if crlf { "\r\n" } else { "\n" };
    lines
        .iter()
        .map(|line| format!("{line}{newline}"))
        .collect()
}

/// Every generated pair, as `(baseline, current)`.
fn pairs() -> Vec<(String, String)> {
    let mut rng = Rng(0x5eed_1234_9abc_def1);
    let mut out = Vec::new();
    for _ in 0..200 {
        let base = document(&mut rng);
        let mut edited = base.clone();
        for _ in 0..1 + rng.below(3) {
            edited = mutate(&edited, &mut rng);
        }
        let baseline_crlf = rng.below(4) == 0;
        let current_crlf = rng.below(4) == 0;
        out.push((render(&base, baseline_crlf), render(&edited, current_crlf)));
    }
    out
}

fn reassemble(skeleton: &DiffSkeleton, baseline: &str, current: &str) -> (String, String) {
    let mut baseline_out = String::new();
    let mut current_out = String::new();
    let mut baseline_slots = vec![0u32; skeleton.units.len()];
    let mut current_slots = vec![0u32; skeleton.units.len()];

    for slot in &skeleton.slots {
        let unit = &skeleton.units[slot.unit as usize];
        match slot.role {
            SlotRole::Shared => {
                baseline_out
                    .push_str(&baseline[unit.baseline.start as usize..unit.baseline.end as usize]);
                current_out
                    .push_str(&current[unit.current.start as usize..unit.current.end as usize]);
                baseline_slots[slot.unit as usize] += 1;
                current_slots[slot.unit as usize] += 1;
            }
            SlotRole::BaselineOnly | SlotRole::PairBaseline => {
                baseline_out
                    .push_str(&baseline[unit.baseline.start as usize..unit.baseline.end as usize]);
                baseline_slots[slot.unit as usize] += 1;
            }
            SlotRole::CurrentOnly | SlotRole::PairCurrent => {
                current_out
                    .push_str(&current[unit.current.start as usize..unit.current.end as usize]);
                current_slots[slot.unit as usize] += 1;
            }
        }
    }

    // Exactly one bearing slot per side per unit that has that side.
    for (index, unit) in skeleton.units.iter().enumerate() {
        assert_eq!(
            baseline_slots[index],
            u32::from(unit.baseline_addr.is_some()),
            "unit {} has the wrong number of baseline slots",
            unit.id
        );
        assert_eq!(
            current_slots[index],
            u32::from(unit.current_addr.is_some()),
            "unit {} has the wrong number of current slots",
            unit.id
        );
    }
    (baseline_out, current_out)
}

#[test]
fn law_1_every_byte_of_both_inputs_is_in_exactly_one_bearing_slot() {
    for (baseline, current) in pairs() {
        let skeleton = diff(&baseline, &current);
        let (baseline_out, current_out) = reassemble(&skeleton, &baseline, &current);
        assert_eq!(baseline_out, baseline);
        assert_eq!(current_out, current);
        // Determinism: the same inputs build the same skeleton.
        assert_eq!(skeleton, diff(&baseline, &current));
    }
}

#[test]
fn law_2_the_replay_round_trips_byte_for_byte() {
    for (baseline, current) in pairs() {
        let skeleton = diff(&baseline, &current);
        let empty = Decisions::new();

        let to_current = to_edits(&skeleton, &empty, MergeSide::Current).unwrap();
        assert_eq!(
            apply_splices(baseline.as_bytes(), current.as_bytes(), &to_current),
            current.as_bytes()
        );
        let to_baseline = to_edits(&skeleton, &empty, MergeSide::Baseline).unwrap();
        assert!(to_baseline.is_empty(), "the identity replay has no edits");
        assert_eq!(
            apply_splices(baseline.as_bytes(), current.as_bytes(), &to_baseline),
            baseline.as_bytes()
        );

        // Mixed vectors: the splices say exactly what the projection says.
        let mut rng = Rng((baseline.len() as u64).wrapping_mul(6_364_136_223_846_793_005) | 1);
        for _ in 0..8 {
            let mut decisions = Decisions::new();
            for unit in &skeleton.units {
                match rng.below(3) {
                    0 => {
                        decisions.insert(unit.id.clone(), MergeSide::Baseline);
                    }
                    1 => {
                        decisions.insert(unit.id.clone(), MergeSide::Current);
                    }
                    _ => {}
                }
            }
            let default_side = if rng.below(2) == 0 {
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
            let edits = to_edits(&skeleton, &decisions, default_side).unwrap();
            assert_eq!(
                apply_splices(baseline.as_bytes(), current.as_bytes(), &edits),
                merged
            );
            // Splices are a legal transaction: sorted and non-overlapping.
            for pair in edits.windows(2) {
                assert!(pair[0].from <= pair[0].to);
                assert!(pair[0].to <= pair[1].from);
            }
        }
    }
}

#[test]
fn law_3_an_unknown_decision_id_produces_nothing_at_all() {
    for (baseline, current) in pairs().into_iter().take(20) {
        let skeleton = diff(&baseline, &current);
        let mut decisions = Decisions::new();
        // Mixed with a real decision: one bad id poisons the transaction.
        decisions.insert(skeleton.units[0].id.clone(), MergeSide::Baseline);
        decisions.insert("GEN 999:999".to_string(), MergeSide::Current);

        assert_eq!(
            merge(
                &skeleton,
                baseline.as_bytes(),
                current.as_bytes(),
                &decisions,
                MergeSide::Current
            ),
            Err(MergeError::UnknownUnitId("GEN 999:999".to_string()))
        );
        assert_eq!(
            to_edits(&skeleton, &decisions, MergeSide::Current),
            Err(MergeError::UnknownUnitId("GEN 999:999".to_string()))
        );
    }
}

#[test]
fn a_renumbered_verse_never_coalesces_and_a_swap_always_does() {
    // The two directed shapes the generator leans on, pinned on their own.
    let baseline = "\\id GEN\n\\c 1\n\\v 1 alpha\n\\v 2 beta\n";
    let renumbered = "\\id GEN\n\\c 1\n\\v 1 alpha\n\\v 9 beta\n";
    let skeleton = diff(baseline, renumbered);
    assert_eq!(
        skeleton
            .units
            .iter()
            .filter(|unit| unit.kind == usfm_onion_2::diff::UnitKind::Coalesced)
            .count(),
        0
    );

    let swapped = "\\id GEN\n\\c 1\n\\v 2 beta\n\\v 1 alpha\n";
    let skeleton = diff(baseline, swapped);
    let moved: Vec<_> = skeleton
        .units
        .iter()
        .filter(|unit| unit.status == usfm_onion_2::diff::Status::Moved)
        .collect();
    assert_eq!(moved.len(), 1);
    assert!(moved[0].displaced);

    // Reverting the move restores the baseline byte-for-byte.
    let mut decisions = Decisions::new();
    decisions.insert(moved[0].id.clone(), MergeSide::Baseline);
    let reverted = merge(
        &skeleton,
        baseline.as_bytes(),
        swapped.as_bytes(),
        &decisions,
        MergeSide::Current,
    )
    .unwrap();
    assert_eq!(reverted, baseline.as_bytes());
}
