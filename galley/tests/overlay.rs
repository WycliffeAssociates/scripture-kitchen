//! What an overlay promises about a target's block structure.
//!
//! Instrument: SHAPES — hand-written USFM through the real lex/lint/mask
//! pipeline, small enough that every address in a fixture can be named.
//!
//! One law runs on every fixture: apply the overlay's edits to the target,
//! re-extract its skeleton, and it equals the source's skeleton with the
//! source's empty blocks folded away — less the inside blocks the target never
//! had, which the overlay leaves for the translator. `expected_rows` states it
//! here rather than calling the comparison the library uses, so the two cannot
//! drift into agreement.

use usfm_galley::overlay::{
    self, BlockAddress, Equivalent, OverlayOptions, Placement, Reason, Scope, Side, Skeleton,
};
use usfm_galley::{BookId, Pantry, Retain, Role, SourceLanes};

const GEN_SOURCE: &str = include_str!("fixtures/overlay/gen-source.usfm");
const GEN_TARGET: &str = include_str!("fixtures/overlay/gen-target.usfm");
const STRAY_TARGET: &str = include_str!("fixtures/overlay/stray-target.usfm");
const PLAIN_SOURCE: &str = include_str!("fixtures/overlay/plain-source.usfm");
const RUNS_SOURCE: &str = include_str!("fixtures/overlay/runs-source.usfm");
const RUNS_TARGET: &str = include_str!("fixtures/overlay/runs-target.usfm");
const TITLES_SOURCE: &str = include_str!("fixtures/overlay/titles-source.usfm");
const TITLES_TARGET: &str = include_str!("fixtures/overlay/titles-target.usfm");
const BRIDGE_SOURCE: &str = include_str!("fixtures/overlay/bridge-source.usfm");
const BRIDGE_TARGET: &str = include_str!("fixtures/overlay/bridge-target.usfm");
const RUNON_SOURCE: &str = include_str!("fixtures/overlay/runon-source.usfm");
const RUNON_TARGET: &str = include_str!("fixtures/overlay/runon-target.usfm");

const SKELETONS: &str = include_str!("goldens/overlay/skeletons.txt");

const TARGET: &str = "books/target.usfm";
const SOURCE: &str = "ref/source.usfm";

// ------------------------------------------------------------------ fixtures

/// A target and a searchable source, both registered — the shape the wall's
/// `update` + `updateReference(id, text, true)` produces.
fn loaded(target: &str, source: &str) -> Pantry {
    let mut pantry = Pantry::new(1 << 20);
    pantry
        .update(TARGET, Role::Target, target)
        .expect("the target registers");
    pantry
        .update_with(
            SOURCE,
            Role::Reference,
            Retain::Text,
            SourceLanes::Lengths,
            source,
        )
        .expect("the source registers with its text");
    pantry
}

fn id(name: &str) -> BookId {
    BookId::from(name)
}

/// One block row as the address that names it: `"GEN 2:23 inside 1 q1"`.
fn row(sid: &str, placement: Placement, ordinal: u32, marker: &str) -> String {
    let side = match placement {
        Placement::Leading => "leading",
        Placement::Inside => "inside",
    };
    format!("{sid} {side} {ordinal} {marker}")
}

/// A target's blocks: every one it has, numbered in document order.
fn target_rows(skeleton: &Skeleton) -> Vec<String> {
    numbered(skeleton, true)
}

/// A source's blocks: onion's empty paragraphs folded away, and what is left
/// renumbered.
fn source_rows(skeleton: &Skeleton) -> Vec<String> {
    numbered(skeleton, false)
}

fn numbered(skeleton: &Skeleton, keep_empty: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen: Vec<(String, Placement, u32)> = Vec::new();
    for block in &skeleton.blocks {
        if !keep_empty && block.empty {
            continue;
        }
        let slot = seen
            .iter_mut()
            .find(|(sid, placement, _)| *sid == block.sid && *placement == block.placement);
        let ordinal = match slot {
            Some((_, _, count)) => {
                *count += 1;
                *count
            }
            None => {
                seen.push((block.sid.clone(), block.placement, 1));
                1
            }
        };
        out.push(row(&block.sid, block.placement, ordinal, &block.marker));
    }
    out
}

/// What the overlaid target's skeleton must be: the source's, except that a
/// verse keeps only as many inside blocks as the target already had there. The
/// overlay respells and removes inside blocks; it never opens one.
fn expected_rows(target_before: &Skeleton, source: &Skeleton) -> Vec<String> {
    let had = |sid: &str| {
        target_before
            .blocks
            .iter()
            .filter(|block| block.sid == sid && block.placement == Placement::Inside)
            .count() as u32
    };
    source_rows(source)
        .into_iter()
        .filter(|row| {
            // `"GEN 2:23 inside 1 q1"`: the sid itself holds a space.
            let mut parts = row.rsplitn(4, ' ');
            let (_, ordinal, side, sid) = (
                parts.next(),
                parts.next().and_then(|n| n.parse::<u32>().ok()),
                parts.next(),
                parts.next().unwrap_or_default(),
            );
            side != Some("inside") || ordinal.is_some_and(|n| n <= had(sid))
        })
        .collect()
}

/// The law, on every fixture: apply the overlay, re-extract, and the target's
/// skeleton is [`expected_rows`]. Nothing is pasted first: with no empty
/// inside blocks opened, every inserted marker already reads back as its own
/// address.
fn overlaid(target: &str, source: &str, opts: &OverlayOptions) -> String {
    let mut pantry = loaded(target, source);
    let applied = overlay::overlay_text(&mut pantry, &id(TARGET), &id(SOURCE), opts)
        .expect("the overlay applies");
    let target_before =
        overlay::skeleton_with(&mut pantry, &id(TARGET), opts).expect("the target has one");
    let source_skeleton =
        overlay::skeleton_with(&mut pantry, &id(SOURCE), opts).expect("the source has one");

    let mut after = Pantry::new(1 << 20);
    after
        .update(TARGET, Role::Target, &applied)
        .expect("the overlaid target is still a book");
    let target_after =
        overlay::skeleton_with(&mut after, &id(TARGET), opts).expect("it still has a skeleton");

    // Only the verses the overlay was allowed to touch can be expected to
    // match; a scoped run leaves the rest of the book alone on purpose.
    let scoped = |rows: Vec<String>| -> Vec<String> {
        match &opts.scope {
            None => rows,
            Some(Scope::Sid(sid)) => rows.into_iter().filter(|r| r.starts_with(sid)).collect(),
            Some(Scope::Chapter(chapter)) => rows
                .into_iter()
                .filter(|r| {
                    r.split(' ').nth(1).and_then(|at| at.split(':').next())
                        == Some(&chapter.to_string())
                })
                .collect(),
        }
    };
    assert_eq!(
        scoped(target_rows(&target_after)),
        scoped(expected_rows(&target_before, &source_skeleton)),
        "the overlaid target's skeleton is the source's, less the inside blocks it never had"
    );
    applied
}

// ------------------------------------------------------- the worked example

/// SHAPES: the GEN 2:21–24 example, address by address.
#[test]
fn the_source_skeleton_is_addressed_by_verse() {
    let mut pantry = loaded(GEN_TARGET, GEN_SOURCE);
    let skeleton = overlay::skeleton(&mut pantry, &id(SOURCE)).expect("a skeleton");
    assert_eq!(
        source_rows(&skeleton),
        [
            row("GEN 2:21", Placement::Leading, 1, "p"),
            row("GEN 2:23", Placement::Inside, 1, "q1"),
            row("GEN 2:23", Placement::Inside, 2, "q2"),
            row("GEN 2:23", Placement::Inside, 3, "q1"),
            row("GEN 2:23", Placement::Inside, 4, "q2"),
            row("GEN 2:24", Placement::Leading, 1, "p"),
        ]
    );
    // Footnotes and character markup are not in a skeleton at all.
    assert!(skeleton.blocks.iter().all(|block| block.marker != "f"));
}

/// SHAPES: a block's `from..end` is its paragraph as onion's grammar closes
/// it. Headings and `\c` are not rows, and still end the block before them.
#[test]
fn a_block_ends_where_its_paragraph_closes() {
    let text = "\\id GEN\n\\c 1\n\\p\n\\v 1 a \\f + \\ft n\\f* b\n\\s1 Heading\n\\p\n\\v 2 c\n\\q1 d\n\\q2 e\n\\c 2\n\\p\n\\v 1 f\n";
    let mut pantry = loaded(text, text);
    let skeleton = overlay::skeleton(&mut pantry, &id(SOURCE)).expect("a skeleton");
    let blocks: Vec<(&str, &str)> = skeleton
        .blocks
        .iter()
        .map(|block| {
            assert_eq!(
                text[block.from as usize..block.to as usize].trim_end(),
                format!("\\{}", block.marker)
            );
            (
                block.marker.as_str(),
                &text[block.from as usize..block.end as usize],
            )
        })
        .collect();
    assert_eq!(
        blocks,
        [
            ("p", "\\p\n\\v 1 a \\f + \\ft n\\f* b\n"),
            ("p", "\\p\n\\v 2 c\n"),
            ("q1", "\\q1 d\n"),
            ("q2", "\\q2 e\n"),
            ("p", "\\p\n\\v 1 f\n"),
        ]
    );
    // The heading's own bytes are in no block.
    let heading = text.find("\\s1").expect("a heading") as u32;
    assert!(
        skeleton
            .blocks
            .iter()
            .all(|block| !(block.from..block.end).contains(&heading))
    );
}

/// SHAPES: en_ulb's `\\s5`, registered as a legacy standalone, stops closing
/// the paragraph it sits in — so the block runs past it to the heading.
#[test]
fn a_registered_legacy_marker_does_not_end_a_block() {
    use usfm_galley::onion::extensions::{
        CustomMarker, ExtensionCategory, ExtensionOptions, set_extensions, set_extensions_with,
    };
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            set_extensions(&[]);
        }
    }
    let _restore = Restore;

    let text = "\\id GEN\n\\c 1\n\\p\n\\v 1 a\n\\s5\n\\v 2 b\n\\s1 Heading\n\\p\n\\v 3 c\n";
    let first_block = || {
        let mut pantry = loaded(text, text);
        let skeleton = overlay::skeleton(&mut pantry, &id(SOURCE)).expect("a skeleton");
        let block = &skeleton.blocks[0];
        text[block.from as usize..block.end as usize].to_owned()
    };
    assert_eq!(
        first_block(),
        "\\p\n\\v 1 a\n",
        "unregistered, `\\s5` is unknown and closes it"
    );

    let reports = set_extensions_with(
        &[CustomMarker {
            name: "s5".to_owned(),
            category: ExtensionCategory::Standalone,
            description: String::new(),
            attributes: Vec::new(),
        }],
        &ExtensionOptions {
            relax_z_prefix: true,
        },
    );
    assert!(reports.is_empty(), "{reports:?}");
    assert_eq!(first_block(), "\\p\n\\v 1 a\n\\s5\n\\v 2 b\n");
}

/// SHAPES: a verse-only target gets the paragraph before v21 and the one
/// before v24. The four poetry lines inside v23 split its text, which is the
/// translator's call, so no empty line is opened for them.
#[test]
fn a_verse_only_target_takes_the_sources_shape() {
    let applied = overlaid(GEN_TARGET, GEN_SOURCE, &OverlayOptions::default());
    assert!(applied.contains("\\p\n\\v 21 Kwa hiyo"), "{applied}");
    assert!(
        applied.contains("mwanamume.”\n\\p\n\\v 24 Kwa sababu"),
        "{applied}"
    );
    assert!(
        !applied.contains("\\q"),
        "no inside block is opened: {applied}"
    );
    // The source's footnotes are the source's; the target's text is its own.
    assert!(!applied.contains("\\f "), "{applied}");
    assert!(!applied.contains("Cited in Matthew"), "{applied}");
}

/// SHAPES: the report names every block it placed — leading blocks only.
#[test]
fn the_report_names_what_it_placed() {
    let mut pantry = loaded(GEN_TARGET, GEN_SOURCE);
    let overlay = overlay::overlay(
        &mut pantry,
        &id(TARGET),
        &id(SOURCE),
        &OverlayOptions::default(),
    )
    .expect("the overlay computes");
    let inserted: Vec<String> = overlay
        .report
        .inserted
        .iter()
        .map(|row| row.address.to_string())
        .collect();
    assert_eq!(inserted, ["GEN 2:21 leading 1", "GEN 2:24 leading 1"]);
    assert_eq!(overlay.report.removed, []);
    assert_eq!(overlay.report.unpaired, []);
    assert!(
        overlay
            .edits
            .windows(2)
            .all(|pair| pair[0].from <= pair[0].to && pair[0].to <= pair[1].from),
        "the transaction is ascending and non-overlapping"
    );
}

/// SHAPES: verses that share a line still get their paragraphs on lines of
/// their own — a leading marker opens the line it needs.
#[test]
fn a_leading_marker_opens_its_own_line() {
    let applied = overlaid(RUNON_TARGET, RUNON_SOURCE, &OverlayOptions::default());
    for line in applied.lines() {
        assert!(
            !line.contains("\\p") || line == "\\p",
            "a paragraph marker sits mid-line: {line:?}"
        );
    }
    assert_eq!(
        applied.matches("\\p\n\\v ").count(),
        3,
        "one per verse: {applied}"
    );
    assert!(
        applied.starts_with("\\id GEN\n\\c 1\n\\p\n\\v 1 Hapo"),
        "{applied}"
    );
}

// ----------------------------------------------------------- removal and fold

/// SHAPES: a target block the source lacks is removed, and its text joins the
/// block before it.
#[test]
fn a_block_the_source_lacks_is_removed() {
    let applied = overlaid(STRAY_TARGET, PLAIN_SOURCE, &OverlayOptions::default());
    assert_eq!(
        applied,
        "\\id GEN\n\\c 1\n\\v 1 In the beginning God created\nthe heavens and the earth.\n\
         \\v 2 Now the earth was formless and empty.\n"
    );
}

/// SHAPES: `\m \p` and `\p \p` in the source are one block each; `\b` is
/// empty by design and survives.
#[test]
fn source_empty_runs_fold_and_b_survives() {
    let mut pantry = loaded(RUNS_TARGET, RUNS_SOURCE);
    let overlay = overlay::overlay(
        &mut pantry,
        &id(TARGET),
        &id(SOURCE),
        &OverlayOptions::default(),
    )
    .expect("the overlay computes");
    let folded: Vec<(&str, &str, u32)> = overlay
        .report
        .collapsed
        .iter()
        .map(|row| (row.sid.as_str(), row.marker.as_str(), row.count))
        .collect();
    assert_eq!(folded, [("GEN 1:2", "p", 2), ("GEN 1:3", "p", 2)]);

    let applied = overlaid(RUNS_TARGET, RUNS_SOURCE, &OverlayOptions::default());
    assert!(applied.contains("\\p\n\\v 2 Nayo"), "{applied}");
    assert!(applied.contains("\\p\n\\v 3 Mungu"), "{applied}");
    assert!(applied.contains("\\b\n\\v 4 Mungu"), "{applied}");
    assert!(!applied.contains("\\m"), "the folded \\m does not cross");
}

// ------------------------------------------------------------------- markers

/// SHAPES: section titles stay home by default and cross when the host lists
/// them.
#[test]
fn titles_cross_only_when_listed() {
    let default = overlaid(TITLES_TARGET, TITLES_SOURCE, &OverlayOptions::default());
    assert!(!default.contains("\\s"), "{default}");
    assert!(default.contains("\\p\n\\v 2 Nayo"), "{default}");

    let listed = OverlayOptions {
        markers: Some(vec!["p".into(), "q".into(), "s".into()]),
        ..OverlayOptions::default()
    };
    let with_titles = overlaid(TITLES_TARGET, TITLES_SOURCE, &listed);
    assert!(
        with_titles.contains("\\s\n\\p\n\\v 2 Nayo"),
        "{with_titles}"
    );
    // The heading arrives EMPTY: the translator writes its words.
    assert!(!with_titles.contains("The Second Day"), "{with_titles}");
}

/// SHAPES: a name that names no marker row is a refusal, never a silent
/// no-op.
#[test]
fn an_unknown_marker_name_refuses() {
    let mut pantry = loaded(TITLES_TARGET, TITLES_SOURCE);
    let opts = OverlayOptions {
        markers: Some(vec!["zzz".into()]),
        ..OverlayOptions::default()
    };
    let error = overlay::overlay(&mut pantry, &id(TARGET), &id(SOURCE), &opts)
        .expect_err("an unknown name fails");
    assert!(error.to_string().contains("zzz"), "{error}");
}

// -------------------------------------------------------- pairing and scope

/// SHAPES: a bridge pairs with the same bridge and takes edits; a verse with
/// no pair takes none and reports once per side.
#[test]
fn bridges_pair_and_unpaired_verses_report() {
    let mut pantry = loaded(BRIDGE_TARGET, BRIDGE_SOURCE);
    let overlay = overlay::overlay(
        &mut pantry,
        &id(TARGET),
        &id(SOURCE),
        &OverlayOptions::default(),
    )
    .expect("the overlay computes");
    assert_eq!(
        overlay
            .report
            .inserted
            .iter()
            .map(|row| (row.address.sid.as_str(), row.marker.as_str()))
            .collect::<Vec<_>>(),
        [("GEN 1:1-2", "p")],
        "the bridge takes its paragraph"
    );
    let unpaired: Vec<(&str, Side, Reason)> = overlay
        .report
        .unpaired
        .iter()
        .map(|row| (row.sid.as_str(), row.side, row.reason))
        .collect();
    assert_eq!(
        unpaired,
        [
            ("GEN 1:5", Side::Target, Reason::Absent),
            ("GEN 1:4", Side::Source, Reason::Absent),
        ]
    );
    overlaid(BRIDGE_TARGET, BRIDGE_SOURCE, &OverlayOptions::default());
}

/// SHAPES: a scoped overlay edits that verse and nothing else.
#[test]
fn scope_bounds_the_transaction() {
    let opts = OverlayOptions {
        scope: Some(Scope::Sid("GEN 2:24".into())),
        ..OverlayOptions::default()
    };
    let applied = overlaid(GEN_TARGET, GEN_SOURCE, &opts);
    assert!(applied.contains("\\p\n\\v 24 Kwa sababu"), "{applied}");
    assert!(!applied.contains("\\p\n\\v 21"), "{applied}");
}

/// SHAPES: every block between one verse's text and the next `\\v` leads
/// that verse, so a stanza break crosses with the line it opens.
#[test]
fn a_stanza_break_leads_with_its_line() {
    let source = "\\id PSA\n\\c 1\n\\q1\n\\v 1 Blessed is the man\n\\q2 who walks not.\n\
                  \\b\n\\q1\n\\v 2 But his delight\n";
    let target = "\\id PSA\n\\c 1\n\\v 1 Heri mtu yule asiyekwenda.\n\\v 2 Bali sheria.\n";
    let applied = overlaid(target, source, &OverlayOptions::default());
    assert_eq!(
        applied,
        "\\id PSA\n\\c 1\n\\q1\n\\v 1 Heri mtu yule asiyekwenda.\n\\b\n\\q1\n\\v 2 Bali sheria.\n"
    );
}

/// SHAPES: a run is cut at a `\\c`, so nothing the source closes a chapter
/// with is carried past the next chapter's number.
#[test]
fn a_chapter_cuts_the_leading_run() {
    let source = "\\id PSA\n\\c 1\n\\q1\n\\v 1 One.\n\\b\n\\c 2\n\\q1\n\\v 1 Two.\n";
    let target = "\\id PSA\n\\c 1\n\\v 1 Moja.\n\\c 2\n\\v 1 Mbili.\n";
    let applied = overlaid(target, source, &OverlayOptions::default());
    assert_eq!(
        applied,
        "\\id PSA\n\\c 1\n\\q1\n\\v 1 Moja.\n\\c 2\n\\q1\n\\v 1 Mbili.\n"
    );
}

/// SHAPES: inside blocks the target already has are respelled to the
/// source's, ordinal by ordinal, and the ones it lacks are not opened.
#[test]
fn inside_blocks_are_respelled_never_opened() {
    let target = GEN_TARGET.replace(
        "na nyama ya nyama yangu;",
        "\n\\q2 na nyama ya nyama yangu;",
    );
    let applied = overlaid(&target, GEN_SOURCE, &OverlayOptions::default());
    assert!(
        applied.contains("mifupa yangu \n\\q1 na nyama"),
        "{applied}"
    );
    assert_eq!(applied.matches("\\q").count(), 1, "{applied}");
}

/// SHAPES: `\\id gen` and `\\id GEN` are one book. The Toc keeps the code
/// verbatim; the overlay compares it folded, so a lowercase export still
/// pairs, and its sids read in capitals like every other book's.
#[test]
fn a_lowercase_book_code_still_pairs() {
    let lower = GEN_TARGET.replacen("\\id GEN", "\\id gen", 1);
    assert_ne!(lower, GEN_TARGET);
    let mut pantry = loaded(&lower, GEN_SOURCE);
    let computed = overlay::overlay(
        &mut pantry,
        &id(TARGET),
        &id(SOURCE),
        &OverlayOptions::default(),
    )
    .expect("the overlay computes");
    assert_eq!(computed.report.unpaired, []);
    assert_eq!(computed.report.inserted.len(), 2);
    let applied = overlaid(&lower, GEN_SOURCE, &OverlayOptions::default());
    assert!(
        applied.starts_with("\\id gen\n"),
        "the file keeps its spelling: {applied}"
    );
    assert!(applied.contains("\\p\n\\v 24 Kwa sababu"), "{applied}");
}

// ------------------------------------------------------------- the node doors

/// SHAPES: found, absent-with-a-place, and unpaired — the three answers.
#[test]
fn a_node_answers_found_absent_or_unpaired() {
    let mut pantry = loaded(BRIDGE_TARGET, BRIDGE_SOURCE);
    let opts = OverlayOptions::default();

    // The source's poetry line has no equivalent in the target yet, and the
    // overlay would put it after the bridge's text.
    let wanted = BlockAddress {
        sid: "GEN 1:1-2".into(),
        placement: Placement::Inside,
        ordinal: 1,
        marker: "q1".into(),
    };
    let answer = overlay::target_node_for(&mut pantry, &id(TARGET), &id(SOURCE), &wanted, &opts)
        .expect("an answer");
    let Equivalent::Absent {
        insert_at,
        placement,
        ..
    } = answer
    else {
        panic!("expected absent, got {answer:?}");
    };
    assert_eq!(placement, Placement::Inside);
    assert!(insert_at > 0);

    // The same address, asked of the source, finds the block itself.
    let found = overlay::source_node_for(&mut pantry, &id(TARGET), &id(SOURCE), &wanted, &opts)
        .expect("an answer");
    let Equivalent::Found { found } = found else {
        panic!("expected found, got {found:?}");
    };
    assert_eq!(found.marker, "q1");

    // A verse with no pair answers about the verse, not about the block.
    let orphan = BlockAddress {
        sid: "GEN 1:5".into(),
        placement: Placement::Leading,
        ordinal: 1,
        marker: "p".into(),
    };
    let answer = overlay::source_node_for(&mut pantry, &id(TARGET), &id(SOURCE), &orphan, &opts)
        .expect("an answer");
    assert_eq!(
        answer,
        Equivalent::Unpaired {
            unpaired: true,
            reason: Reason::Absent,
        }
    );
}

/// SHAPES: an address whose position is still there but now spells something
/// else is refused by BOTH doors, naming the two spellings.
#[test]
fn a_stale_address_is_refused_by_name() {
    let mut pantry = loaded(GEN_TARGET, GEN_SOURCE);
    let opts = OverlayOptions::default();
    // The source's second inside block at GEN 2:23 is a `\q2`; a host holding
    // a `\q1` there is looking at a node that moved.
    let stale = BlockAddress {
        sid: "GEN 2:23".into(),
        placement: Placement::Inside,
        ordinal: 2,
        marker: "q1".into(),
    };
    let error = overlay::target_node_for(&mut pantry, &id(TARGET), &id(SOURCE), &stale, &opts)
        .expect_err("targetNodeFor checks the source address it was handed");
    assert_eq!(
        error.to_string(),
        "GEN 2:23 inside 2 names q1 but the node there is q2 — the address is stale"
    );

    // The mirror door checks the TARGET side, where GEN 2:24's leading block
    // is a `\p` once the overlay has run.
    let applied = overlay::overlay_text(&mut pantry, &id(TARGET), &id(SOURCE), &opts)
        .expect("the overlay applies");
    let mut after = loaded(&applied, GEN_SOURCE);
    let stale = BlockAddress {
        sid: "GEN 2:24".into(),
        placement: Placement::Leading,
        ordinal: 1,
        marker: "q1".into(),
    };
    let error = overlay::source_node_for(&mut after, &id(TARGET), &id(SOURCE), &stale, &opts)
        .expect_err("sourceNodeFor checks the target address it was handed");
    assert_eq!(
        error.to_string(),
        "GEN 2:24 leading 1 names q1 but the node there is p — the address is stale"
    );

    // The same position, named correctly, still answers.
    let fresh = BlockAddress {
        marker: "p".into(),
        ..stale
    };
    assert!(
        overlay::source_node_for(&mut after, &id(TARGET), &id(SOURCE), &fresh, &opts).is_ok(),
        "the position is still the key"
    );
}

/// SHAPES: a target that already has the source's shape is a no-op, which is
/// what makes the overlay idempotent.
#[test]
fn a_second_overlay_changes_nothing() {
    let once = overlaid(GEN_TARGET, GEN_SOURCE, &OverlayOptions::default());
    let twice = overlaid(&once, GEN_SOURCE, &OverlayOptions::default());
    assert_eq!(once, twice);
}

/// Every field `skeleton` computes, rendered one row per line — the golden
/// that says a change to HOW the blocks are grouped changed nothing about
/// WHAT they are.
fn render(skeleton: &Skeleton) -> String {
    let mut out = String::new();
    for verse in &skeleton.verses {
        out.push_str(&format!(
            "verse {} {}..{} text {}..{}\n",
            verse.sid, verse.from, verse.to, verse.text_from, verse.text_to
        ));
    }
    for block in &skeleton.blocks {
        out.push_str(&format!(
            "block {} {} {} {} {}..{} end {} empty={}\n",
            block.sid,
            match block.placement {
                Placement::Leading => "leading",
                Placement::Inside => "inside",
            },
            block.ordinal,
            block.marker,
            block.from,
            block.to,
            block.end,
            block.empty
        ));
    }
    for run in &skeleton.collapsed {
        out.push_str(&format!(
            "collapsed {} {} {}\n",
            run.sid, run.marker, run.count
        ));
    }
    out
}

/// SHAPES: every offset, address and flag `skeleton` computes, pinned. A
/// change to how the blocks are GROUPED has to leave what they ARE untouched.
#[test]
fn the_skeletons_are_what_they_were() {
    let mut out = String::new();
    let mut runs = loaded(RUNS_TARGET, RUNS_SOURCE);
    for (name, book) in [("target", TARGET), ("source", SOURCE)] {
        out.push_str(&format!("# runs {name}\n"));
        out.push_str(&render(
            &overlay::skeleton(&mut runs, &id(book)).expect("a skeleton"),
        ));
    }
    let mut books = loaded(GEN_TARGET, GEN_SOURCE);
    for (name, book) in [("target", TARGET), ("source", SOURCE)] {
        out.push_str(&format!("# gen {name}\n"));
        out.push_str(&render(
            &overlay::skeleton(&mut books, &id(book)).expect("a skeleton"),
        ));
    }
    assert_eq!(out, SKELETONS);
}
