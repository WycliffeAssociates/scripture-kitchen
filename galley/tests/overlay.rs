//! What an overlay promises about a target's block structure.
//!
//! Instrument: SHAPES — hand-written USFM through the real lex/lint/mask
//! pipeline, small enough that every address in a fixture can be named.
//!
//! One law runs on every fixture: apply the overlay's edits to the target,
//! re-extract its skeleton, and it equals the source's skeleton with the
//! source's empty blocks folded away. `equal_skeletons` states it here rather
//! than calling the comparison the library uses, so the two cannot drift into
//! agreement.

use usfm_galley::onion::Edit;
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

/// The law, on every fixture: apply the overlay, give every block it inserted
/// EMPTY the word a translator would type, re-extract, and the target's
/// skeleton is the source's.
///
/// The paste is not a softening. A block marker with nothing under it but the
/// next `\v` is, in USFM, the block that verse sits in — there is no byte in
/// the file that says otherwise — so a scaffold reads back as its own address
/// only once it holds text, which is exactly what the scaffold is for.
/// Everything else the law claims — which addresses, in what order, spelled
/// how — is asserted here unchanged.
fn overlaid(target: &str, source: &str, opts: &OverlayOptions) -> String {
    let mut pantry = loaded(target, source);
    let computed = overlay::overlay(&mut pantry, &id(TARGET), &id(SOURCE), opts)
        .expect("the overlay computes");
    let applied = overlay::overlay_text(&mut pantry, &id(TARGET), &id(SOURCE), opts)
        .expect("the overlay applies");
    let pasted = with_pasted_text(target, &computed.edits);

    let mut after = Pantry::new(1 << 20);
    after
        .update(TARGET, Role::Target, &pasted)
        .expect("the overlaid target is still a book");
    let target_after =
        overlay::skeleton_with(&mut after, &id(TARGET), opts).expect("it still has a skeleton");
    let source_skeleton =
        overlay::skeleton_with(&mut pantry, &id(SOURCE), opts).expect("the source has one");

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
        scoped(source_rows(&source_skeleton)),
        "the overlaid target's skeleton is the source's"
    );
    applied
}

/// The transaction applied, with a word typed into every inside block it
/// opened. An inside insert is the one that opens a line it does not finish;
/// a leading insert ends in the newline its `\v` follows.
fn with_pasted_text(target: &str, edits: &[Edit]) -> String {
    let mut out = String::new();
    let mut cursor = 0usize;
    for edit in edits {
        out.push_str(&target[cursor..edit.from as usize]);
        out.push_str(edit.insert.as_str());
        let insert = edit.insert.as_str();
        if insert.contains('\\') && !insert.ends_with('\n') {
            out.push_str(" x");
        }
        cursor = edit.to as usize;
    }
    out.push_str(&target[cursor..]);
    out
}

/// The same skeleton comparison with NOTHING pasted — the form that holds
/// wherever the source's own blocks disambiguate the scaffold.
fn strictly_equal(target: &str, source: &str, opts: &OverlayOptions) {
    let mut pantry = loaded(target, source);
    let applied = overlay::overlay_text(&mut pantry, &id(TARGET), &id(SOURCE), opts)
        .expect("the overlay applies");
    let mut after = Pantry::new(1 << 20);
    after
        .update(TARGET, Role::Target, &applied)
        .expect("still a book");
    let target_after =
        overlay::skeleton_with(&mut after, &id(TARGET), opts).expect("still a skeleton");
    let source_skeleton =
        overlay::skeleton_with(&mut pantry, &id(SOURCE), opts).expect("the source has one");
    assert_eq!(
        target_rows(&target_after),
        source_rows(&source_skeleton),
        "the empty scaffold already reads back as the source's skeleton"
    );
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

/// SHAPES: a verse-only target gets the paragraph before v21, four EMPTY
/// poetry lines after v23's text, and the paragraph before v24.
#[test]
fn a_verse_only_target_takes_the_sources_shape() {
    let applied = overlaid(GEN_TARGET, GEN_SOURCE, &OverlayOptions::default());
    assert!(applied.contains("\\p\n\\v 21 Kwa hiyo"), "{applied}");
    assert!(
        applied.contains("mwanamume.”\n\\q1\n\\q2\n\\q1\n\\q2\n\\p\n\\v 24 Kwa sababu"),
        "{applied}"
    );
    // Here the source's own `\p` closes the poetry run, so the scaffold needs
    // no text to read back as the source's skeleton.
    strictly_equal(GEN_TARGET, GEN_SOURCE, &OverlayOptions::default());
    // The source's footnotes are the source's; the target's text is its own.
    assert!(!applied.contains("\\f "), "{applied}");
    assert!(!applied.contains("Cited in Matthew"), "{applied}");
}

/// SHAPES: the report names every block it placed, and says which arrived
/// empty.
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
    assert_eq!(overlay.report.inserted.len(), 6);
    assert_eq!(overlay.report.removed, []);
    assert_eq!(overlay.report.unpaired, []);
    let empty: Vec<&str> = overlay
        .report
        .inserted
        .iter()
        .filter(|row| row.empty)
        .map(|row| row.marker.as_str())
        .collect();
    assert_eq!(empty, ["q1", "q2", "q1", "q2"], "inside blocks await text");
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
        [("GEN 1:1-2", "q1")],
        "the bridge takes its poetry line"
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
    assert!(!applied.contains("\\q1"), "{applied}");
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
