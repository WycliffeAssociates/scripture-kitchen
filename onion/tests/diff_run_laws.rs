//! The five laws a unit's runs obey, over hand cases and the whole test tier.
//!
//! ```text
//! \v 1 Jesus \add wept\add*.
//!      runs  [\v 1 ]Markup [Jesus]Text [ ]Whitespace [\add ]Markup
//!            [wept]Text [\add*]Markup [.]Text            ← tiles the unit
//!      reading  (drop Markup)  "Jesus wept."             ← the `text` cut
//! ```
//!
//! L1 tiling, L2 the text view, L3 the two unit flags agree with the runs,
//! L4 unchanged means equal, L5 no regression against the pre-position
//! builder, with a note as its own reading. L6 is the wire's and lives in
//! `onion-wasm/tests/conformance.mjs`.
//!
//! Instrument: VOLUME — the whole test tier, `testData/exampleCorpora`, each
//! book against five scripted edits of itself. Absent bytes are a loud
//! failure, never a silent skip.

use std::path::{Path, PathBuf};

use similar::{ChangeTag, TextDiff};
use unicode_segmentation::UnicodeSegmentation;

use usfm_onion::tables::generated;
use usfm_onion::tables::schema::MarkerKind;

use usfm_onion::diff::{
    DecisionUnit, RunKind, RunWhat, Status, TextDiffMode, TextDiffRun, TotalText, UnitTextDiff,
    diff_with_text,
};

// ---------------------------------------------------------------- the laws

/// L1: the runs of one side tile that side's unit span — sorted, gapless,
/// non-overlapping, opening and closing on the span.
fn tiling(runs: &[TextDiffRun], span: &std::ops::Range<u32>, label: &str) {
    if runs.is_empty() {
        assert_eq!(span.start, span.end, "{label}: an empty unit has no runs");
        return;
    }
    assert_eq!(runs[0].from, span.start, "{label}: opens the span");
    assert_eq!(runs[runs.len() - 1].to, span.end, "{label}: closes it");
    for pair in runs.windows(2) {
        assert_eq!(pair[0].to, pair[1].from, "{label}: gapless and disjoint");
    }
    for run in runs {
        assert!(run.from < run.to, "{label}: a run is never empty");
    }
}

/// A run's bytes: `source[from..to]` on its own side, never carried on the
/// run itself.
fn run_text<'a>(source: &'a [u8], run: &TextDiffRun) -> &'a str {
    std::str::from_utf8(&source[run.from as usize..run.to as usize])
        .expect("run spans fall on character boundaries")
}

/// L2: concatenating what is not markup IS the `text` cut of the same span.
fn text_view(runs: &[TextDiffRun], side: &TotalText<'_>, span: &std::ops::Range<u32>, label: &str) {
    let reading: String = runs
        .iter()
        .filter(|run| run.what != RunWhat::Markup)
        .map(|run| run_text(side.source, run))
        .collect();
    assert_eq!(reading, side.slice(span), "{label}: the text view");
}

/// L3: the unit's two flags and the runs say the same thing.
///
/// Both flags are byte claims made after ASCII whitespace is stripped, so the
/// runs answer in the same terms: what a side LOST and what the other side
/// GAINED have to be the same non-whitespace bytes, or the flag is lying about
/// what moved. A token-grain pass can still mark `\p` changed when only the
/// space after it moved, which is why the comparison strips.
fn flags_agree(
    unit: &DecisionUnit,
    text: &UnitTextDiff,
    baseline: &TotalText<'_>,
    current: &TotalText<'_>,
    label: &str,
) {
    if unit.is_usfm_structure_change {
        assert_eq!(
            stripped(
                &text.baseline,
                baseline.source,
                RunKind::Removed,
                Some(RunWhat::Text)
            ),
            stripped(
                &text.current,
                current.source,
                RunKind::Added,
                Some(RunWhat::Text)
            ),
            "{label}: a structure change moves no words"
        );
        assert!(
            both(text).any(|run| run.what == RunWhat::Markup && run.kind != RunKind::Unchanged),
            "{label}: …and moves at least one markup run"
        );
    }
    if unit.is_whitespace_change {
        assert_eq!(
            stripped(&text.baseline, baseline.source, RunKind::Removed, None),
            stripped(&text.current, current.source, RunKind::Added, None),
            "{label}: a whitespace change moves nothing but spacing"
        );
    }
}

/// The non-whitespace bytes of one kind of run, optionally of one `what`.
fn stripped(runs: &[TextDiffRun], source: &[u8], kind: RunKind, what: Option<RunWhat>) -> String {
    runs.iter()
        .filter(|run| run.kind == kind && what.is_none_or(|want| run.what == want))
        .flat_map(|run| run_text(source, run).chars())
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect()
}

fn both(text: &UnitTextDiff) -> impl Iterator<Item = &TextDiffRun> {
    text.baseline.iter().chain(&text.current)
}

/// L4: unchanged means equal. The word differ pairs the two sides' unchanged
/// TEXT byte for byte; the markup pass pairs unchanged markup token for token.
fn unchanged_means_equal(
    text: &UnitTextDiff,
    baseline: &TotalText<'_>,
    current: &TotalText<'_>,
    label: &str,
) {
    let reading = |runs: &[TextDiffRun], source: &[u8]| -> String {
        runs.iter()
            .filter(|run| run.kind == RunKind::Unchanged && run.what != RunWhat::Markup)
            .map(|run| run_text(source, run))
            .collect()
    };
    assert_eq!(
        reading(&text.baseline, baseline.source),
        reading(&text.current, current.source),
        "{label}: unchanged text"
    );
    // Concatenated, not listed: coalescing is per side, so the same unchanged
    // token sequence can arrive as one run where the other side has two.
    let markup = |runs: &[TextDiffRun], source: &[u8]| -> String {
        runs.iter()
            .filter(|run| run.kind == RunKind::Unchanged && run.what == RunWhat::Markup)
            .map(|run| run_text(source, run))
            .collect()
    };
    assert_eq!(
        markup(&text.baseline, baseline.source),
        markup(&text.current, current.source),
        "{label}: unchanged markup"
    );
}

/// L5: the pre-position builder, kept as the oracle.
///
/// Its output is what the door shipped before runs carried a span: the `text`
/// cut of each side, word-diffed, same-kind neighbours coalesced — segmented
/// where the reading enters or leaves a note, so `Jesus\f + why\f*` is
/// `Jesus` and `why`, never `Jesuswhy`. The note edges come from the tree
/// here, not from the builder.
type Reduced = Vec<(String, RunKind)>;

/// Every note's byte extent, off the tree, ascending.
fn notes(tokens: &[usfm_onion::Token]) -> Vec<std::ops::Range<u32>> {
    let cst = usfm_onion::cst::build(tokens);
    let mut notes: Vec<_> = (1..cst.nodes.len() as u32)
        .filter(|&node| {
            let opener = cst.nodes[node as usize].token as usize;
            generated::kind(tokens[opener].marker_idx) == MarkerKind::Note
        })
        .map(|node| cst.extent(node, tokens))
        .collect();
    notes.sort_by_key(|note| note.start);
    notes
}

/// One side's slice cut into the differ's units at note edges.
fn units<'s>(
    notes: &[std::ops::Range<u32>],
    side: &TotalText<'_>,
    span: &std::ops::Range<u32>,
    slice: &'s str,
    mode: TextDiffMode,
) -> Vec<&'s str> {
    let note_of = |at: u32| {
        let after = notes.partition_point(|note| note.start <= at);
        after
            .checked_sub(1)
            .filter(|&note| notes[note].contains(&at))
    };
    let mut cuts = vec![0usize];
    let pieces: Vec<_> = side.pieces(span).collect();
    for pair in pieces.windows(2) {
        if note_of(pair[0].0.start) != note_of(pair[1].0.start) {
            cuts.push(pair[1].1 as usize);
        }
    }
    cuts.push(slice.len());
    cuts.windows(2)
        .flat_map(|cut| {
            let segment = &slice[cut[0]..cut[1]];
            match mode {
                TextDiffMode::Words => segment.split_word_bounds().collect::<Vec<_>>(),
                TextDiffMode::Chars => segment.graphemes(true).collect(),
                TextDiffMode::None => unreachable!("the caller never asks"),
            }
        })
        .collect()
}

fn old_runs(baseline: &[&str], current: &[&str]) -> (Reduced, Reduced) {
    let diff = TextDiff::configure().diff_slices(baseline, current);
    let (mut old_baseline, mut old_current) = (Vec::new(), Vec::new());
    let push = |runs: &mut Reduced, text: &str, kind: RunKind| match runs.last_mut() {
        Some(last) if last.1 == kind => last.0.push_str(text),
        _ => runs.push((text.to_string(), kind)),
    };
    for change in diff.iter_all_changes() {
        let text = change.as_str().unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        match change.tag() {
            ChangeTag::Equal => {
                push(&mut old_baseline, text, RunKind::Unchanged);
                push(&mut old_current, text, RunKind::Unchanged);
            }
            ChangeTag::Delete => push(&mut old_baseline, text, RunKind::Removed),
            ChangeTag::Insert => push(&mut old_current, text, RunKind::Added),
        }
    }
    (old_baseline, old_current)
}

/// The new runs reduced the way L5 says: markup dropped, same-kind neighbours
/// joined.
fn reduced(runs: &[TextDiffRun], source: &[u8]) -> Reduced {
    let mut out: Reduced = Vec::new();
    for run in runs.iter().filter(|run| run.what != RunWhat::Markup) {
        let text = run_text(source, run);
        match out.last_mut() {
            Some(last) if last.1 == run.kind => last.0.push_str(text),
            _ => out.push((text.to_string(), run.kind)),
        }
    }
    out
}

// ------------------------------------------------------------- the harness

/// Every law over one pair, at both grains. Returns how many units carried
/// runs, so a caller can refuse a vacuous sweep.
fn check_pair(label: &str, baseline: &str, current: &str) -> usize {
    let mut with_runs = 0usize;
    let baseline_tokens = usfm_onion::lex(baseline);
    let current_tokens = usfm_onion::lex(current);
    let baseline_total = TotalText::new(baseline.as_bytes(), &baseline_tokens);
    let current_total = TotalText::new(current.as_bytes(), &current_tokens);
    let (baseline_notes, current_notes) = (notes(&baseline_tokens), notes(&current_tokens));
    for mode in [TextDiffMode::Words, TextDiffMode::Chars] {
        let (skeleton, texts) = diff_with_text(baseline, current, mode);
        for (unit, text) in skeleton.units.iter().zip(&texts) {
            let Some(text) = text else { continue };
            let at = format!("{label} {} {mode:?}", unit.id);
            if !text.baseline.is_empty() {
                tiling(&text.baseline, &unit.baseline, &format!("{at} baseline"));
                text_view(
                    &text.baseline,
                    &baseline_total,
                    &unit.baseline,
                    &format!("{at} baseline"),
                );
            }
            if !text.current.is_empty() {
                tiling(&text.current, &unit.current, &format!("{at} current"));
                text_view(
                    &text.current,
                    &current_total,
                    &unit.current,
                    &format!("{at} current"),
                );
            }
            flags_agree(unit, text, &baseline_total, &current_total, &at);
            unchanged_means_equal(text, &baseline_total, &current_total, &at);

            if unit.status == Status::Modified {
                let (baseline_slice, current_slice) = (
                    baseline_total.slice(&unit.baseline),
                    current_total.slice(&unit.current),
                );
                let (old_baseline, old_current) = old_runs(
                    &units(
                        &baseline_notes,
                        &baseline_total,
                        &unit.baseline,
                        &baseline_slice,
                        mode,
                    ),
                    &units(
                        &current_notes,
                        &current_total,
                        &unit.current,
                        &current_slice,
                        mode,
                    ),
                );
                assert_eq!(
                    reduced(&text.baseline, baseline_total.source),
                    old_baseline,
                    "{at}: L5 baseline"
                );
                assert_eq!(
                    reduced(&text.current, current_total.source),
                    old_current,
                    "{at}: L5 current"
                );
            }
            with_runs += 1;
        }
    }
    with_runs
}

// ----------------------------------------------------------------- the cases

/// The five edits §5 names, each a real shape a translator makes.
fn edits(source: &str) -> Vec<(&'static str, String)> {
    vec![
        ("one word changed", replace_once(source, "the ", "thé ")),
        ("a poetry level moved", replace_once(source, "\\q1", "\\q2")),
        (
            "a note body changed",
            replace_once(source, "\\ft ", "\\ft edited "),
        ),
        ("a verse deleted", drop_line(source, "\\v 3 ")),
        ("a verse added", add_verse(source)),
    ]
}

fn replace_once(source: &str, from: &str, to: &str) -> String {
    match source.find(from) {
        Some(at) => format!("{}{to}{}", &source[..at], &source[at + from.len()..]),
        None => source.to_string(),
    }
}

fn drop_line(source: &str, opener: &str) -> String {
    let mut done = false;
    source
        .split_inclusive('\n')
        .filter(|line| {
            let hit = !done && line.trim_start().starts_with(opener);
            done |= hit;
            !hit
        })
        .collect()
}

fn add_verse(source: &str) -> String {
    let mut done = false;
    source
        .split_inclusive('\n')
        .flat_map(|line| {
            if !done && line.trim_start().starts_with("\\v 2 ") {
                done = true;
                return vec![
                    line.to_string(),
                    "\\v 2b an inserted half verse\n".to_string(),
                ];
            }
            vec![line.to_string()]
        })
        .collect()
}

const SHAPES: &[(&str, &str, &str)] = &[
    (
        "a word inside a character marker",
        "\\id GEN\n\\c 1\n\\v 1 the \\w quick\\w* fox\n",
        "\\id GEN\n\\c 1\n\\v 1 the \\w slow\\w* fox\n",
    ),
    (
        "markup changed while a word also changed",
        "\\id GEN\n\\c 1\n\\v 1 a plain word\n\\q1 and more\n",
        "\\id GEN\n\\c 1\n\\v 1 a plain phrase\n\\q2 and more\n",
    ),
    (
        "markup changed and nothing else",
        "\\id GEN\n\\c 1\n\\v 1 a plain word\n",
        "\\id GEN\n\\c 1\n\\v 1 a plain \\add word\\add*\n",
    ),
    (
        "whitespace only",
        "\\id GEN\n\\c 1\n\\p\n\\v 1 one\n\\v 2 two\n",
        "\\id GEN\n\\c 1\n\\p \\v 1 one   \\v 2 two\n",
    ),
    (
        "a note body changed",
        "\\id GEN\n\\c 1\n\\v 1 Jesus\\f + \\ft why\\f* wept.\n",
        "\\id GEN\n\\c 1\n\\v 1 Jesus\\f + \\ft because\\f* wept.\n",
    ),
    (
        "a verse deleted and one added",
        "\\id GEN\n\\c 1\n\\v 1 one\n\\v 2 two\n",
        "\\id GEN\n\\c 1\n\\v 1 one\n\\v 3 three\n",
    ),
    (
        "an attribute list changed",
        "\\id GEN\n\\c 1\n\\v 1 the \\w earth|lemma=\"earth\"\\w*\n",
        "\\id GEN\n\\c 1\n\\v 1 the \\w earth|lemma=\"land\"\\w*\n",
    ),
    (
        "a heading appears between two verses",
        "\\id GEN\n\\c 1\n\\v 1 one\n\\v 2 two\n",
        "\\id GEN\n\\c 1\n\\v 1 one\n\\s A heading\n\\p\n\\v 2 two\n",
    ),
];

#[test]
fn the_laws_hold_on_every_hand_shape() {
    let mut units = 0usize;
    for (label, baseline, current) in SHAPES {
        units += check_pair(label, baseline, current);
    }
    assert!(units > 10, "the hand cases produced only {units} run lists");
}

/// A markup change inside a unit whose words also changed is visible, which is
/// what P3 was: `is_usfm_structure_change` is false there, and the runs say it
/// anyway.
#[test]
fn a_markup_change_shows_even_when_the_words_moved_too() {
    // Both changes inside ONE unit: a unit is cut at its `\v`, so the marker
    // has to follow the anchor to share the verse's runs.
    let baseline = "\\id GEN\n\\c 1\n\\v 1 a plain word\n\\q1 and more\n";
    let current = "\\id GEN\n\\c 1\n\\v 1 a plain phrase\n\\q2 and more\n";
    let (skeleton, texts) = diff_with_text(baseline, current, TextDiffMode::Words);
    let at = skeleton
        .units
        .iter()
        .position(|unit| unit.status == Status::Modified)
        .expect("one modified unit");
    assert!(
        !skeleton.units[at].is_usfm_structure_change,
        "the words changed too, so the flag is false"
    );
    let text = texts[at].as_ref().expect("a modified unit has runs");
    assert!(
        both(text).any(|run| run.what == RunWhat::Markup && run.kind != RunKind::Unchanged),
        "the `\\q1` → `\\q2` must show as a moved markup run"
    );
}

// ------------------------------------------------------------- the tier sweep

fn collect_usfm(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|err| panic!("{}: {err}", dir.display())) {
        let path = entry.expect("a readable entry").path();
        if path.is_dir() {
            collect_usfm(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "usfm") {
            out.push(path);
        }
    }
}

#[test]
fn the_laws_hold_over_the_whole_test_tier() {
    let dir = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../testData/exampleCorpora"
    ));
    let mut paths = Vec::new();
    collect_usfm(dir, &mut paths);
    paths.sort();
    assert!(!paths.is_empty(), "{} holds no books", dir.display());

    let (mut books, mut units) = (0usize, 0usize);
    for path in &paths {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let label = path
            .file_name()
            .expect("a name")
            .to_string_lossy()
            .into_owned();
        for (what, edited) in edits(&source) {
            units += check_pair(&format!("{label}: {what}"), &source, &edited);
        }
        books += 1;
    }
    assert!(books > 100, "expected the whole tier, saw {books} books");
    assert!(
        units > 1000,
        "expected real coverage, saw {units} run lists"
    );
}

/// A footnote inserted after a word leaves the word unchanged, and every run
/// inside the note says so.
#[test]
fn a_note_is_its_own_reading() {
    let baseline = "\\id PHM\n\\c 1\n\\p\n\\v 1 Paul a servant of Christ\n";
    let current =
        "\\id PHM\n\\c 1\n\\p\n\\v 1 Paul a servant\\f + \\fr 1:1 \\ft Or slave\\f* of Christ\n";
    let (skeleton, texts) = diff_with_text(baseline, current, TextDiffMode::Words);
    let at = skeleton
        .units
        .iter()
        .position(|unit| unit.status == Status::Modified)
        .expect("the verse changed");
    let text = texts[at].as_ref().expect("a modified unit has runs");

    assert!(
        text.baseline
            .iter()
            .all(|run| run.kind == RunKind::Unchanged),
        "nothing was removed: {:?}",
        text.baseline
    );
    let added: Vec<(&str, bool)> = text
        .current
        .iter()
        .filter(|run| run.kind == RunKind::Added)
        .map(|run| (run_text(current.as_bytes(), run), run.note))
        .collect();
    assert!(
        added.iter().all(|(_, note)| *note),
        "only the note is new: {added:?}"
    );

    let reading = |note: bool| -> String {
        text.current
            .iter()
            .filter(|run| run.what != RunWhat::Markup && run.note == note)
            .map(|run| run_text(current.as_bytes(), run))
            .collect()
    };
    assert_eq!(
        reading(false),
        "Paul a servant of Christ\n",
        "the verse reads past it"
    );
    assert_eq!(
        reading(true),
        "1:1 Or slave",
        "and the note reads on its own"
    );
}
