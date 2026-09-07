//! What a literal find promises about coordinates.
//!
//! Two instruments in one file, and the module says which is which per test:
//!
//! - SHAPES — hand-written USFM through the real `lex`/`cst`/`mask` pipeline,
//!   for the claims a corpus cannot state cleanly: a needle inside a footnote
//!   is unreachable, a needle across one is `Split`, `the` is not `then`, and
//!   a case-insensitive hit lands on the source bytes a naive `str::find`
//!   over the raw USFM would have found.
//! - VOLUME — the whole test tier, `testData/exampleCorpora` (160 books,
//!   12.8 MB): every hit round-trips source → projected → source, every hit's
//!   source pieces re-read to the needle, and the whole-word rule restated in
//!   `galley::find` agrees with `sous_core::words` word for word.
//!
//! Absent corpus bytes are a loud failure, never a silent skip.

use std::path::PathBuf;

use sous_core::words::for_each_word;
use usfm_galley::find::{Find, Hit, SourceSpan};
use usfm_galley::onion::{Filter, Mask, cst, lex, mask};

// ------------------------------------------------------------------ fixtures

/// A book's verse-text mask, built through the real pipeline.
fn project(source: &str) -> Mask {
    let tokens = lex(source);
    let tree = cst::build(&tokens);
    mask(source.as_bytes(), &tokens, &tree, &Filter::verse_text())
}

/// The concatenated source bytes a hit names, markup skipped.
fn reread(source: &str, hit: &Hit) -> String {
    hit.source
        .pieces()
        .map(|piece| &source[piece.start as usize..piece.end as usize])
        .collect()
}

/// Every `.usfm` in the test tier, path and text.
fn tier() -> Vec<(PathBuf, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testData/exampleCorpora");
    let mut books = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).unwrap_or_else(|error| panic!("{}: {error}", dir.display()));
        for entry in entries {
            let path = entry.expect("readable entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "usfm") {
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                books.push((path, text));
            }
        }
    }
    assert!(
        books.len() > 100,
        "the test tier is 160 books; {} found under {}",
        books.len(),
        root.display()
    );
    books.sort();
    books
}

// ------------------------------------------------------------------- SHAPES

const NOTED: &str = "\\id GEN\n\\c 1\n\\v 1 Jesus wept.\\f + \\ft why he wept\\f* Then he rose.\n";

#[test]
fn a_needle_inside_a_footnote_is_not_in_the_projection() {
    let mask = project(NOTED);
    // The words are in the source…
    assert!(NOTED.contains("why he wept"));
    // …and nowhere in the view a proofreader reads.
    assert_eq!(
        Find::literal("why he wept")
            .in_projection(&mask, NOTED.as_bytes())
            .count(),
        0
    );
}

#[test]
fn a_needle_across_a_footnote_gap_comes_back_split() {
    let mask = project(NOTED);
    let hits: Vec<Hit> = Find::literal("wept. Then")
        .in_projection(&mask, NOTED.as_bytes())
        .collect();
    assert_eq!(hits.len(), 1, "the projection joins the two sides");
    let SourceSpan::Split(pieces) = &hits[0].source else {
        panic!("a hit over a masked gap must be Split, got {:?}", hits[0]);
    };
    assert_eq!(pieces.len(), 2, "one piece per contiguous source run");
    // The pieces skip the note; a single range would have swallowed it.
    assert_eq!(reread(NOTED, &hits[0]), "wept. Then");
    assert!(
        NOTED[pieces[0].end as usize..pieces[1].start as usize].starts_with("\\f "),
        "the gap between the pieces is the note itself"
    );
}

#[test]
fn whole_word_the_is_not_then() {
    let source = "\\id GEN\n\\c 1\n\\v 1 the theme of them, then the end.\n";
    let mask = project(source);
    let projected = mask.text(source.as_bytes());
    let words: Vec<String> = Find::literal("the")
        .whole_word(true)
        .in_projection(&mask, source.as_bytes())
        .map(|hit| reread(source, &hit))
        .collect();
    assert_eq!(words, vec!["the", "the"], "in {projected:?}");
    assert_eq!(
        Find::literal("the")
            .in_projection(&mask, source.as_bytes())
            .count(),
        5,
        "the, theme, them, then, the"
    );
}

#[test]
fn a_case_insensitive_hit_lands_on_the_raw_source_bytes() {
    let source =
        "\\id GEN\n\\c 14\n\\v 18 And MELCHIZEDEK king of Salem\\f + \\ft note\\f* met him.\n";
    let mask = project(source);
    let hits: Vec<Hit> = Find::literal("melchizedek")
        .case_insensitive(true)
        .in_projection(&mask, source.as_bytes())
        .collect();
    assert_eq!(hits.len(), 1);
    // The naive answer over the raw USFM, which is markup-free right there.
    let naive = source.find("MELCHIZEDEK").expect("in the source");
    assert_eq!(
        hits[0].source,
        SourceSpan::Contiguous(naive as u32..(naive + 11) as u32)
    );
    assert_eq!(reread(source, &hits[0]), "MELCHIZEDEK");
}

#[test]
fn a_folded_needle_that_changes_length_still_maps_back() {
    // `İ` is two source bytes and folds to one, so every folded offset after
    // it is short by one; the map is what puts the hit back.
    let source = "\\id GEN\n\\c 1\n\\v 1 İsa and MELCHIZEDEK.\n";
    let mask = project(source);
    let hits: Vec<Hit> = Find::literal("melchizedek")
        .case_insensitive(true)
        .in_projection(&mask, source.as_bytes())
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(reread(source, &hits[0]), "MELCHIZEDEK");
}

// ------------------------------------------------------------------- VOLUME

/// A needle common enough to hit in every Latin book of the tier and short
/// enough to cross a masked gap often.
const COMMON: &str = "the";

/// Masked gaps per book the Split claim is proved on, and the scalars taken
/// from either side of one to make a needle out of it.
const SEAMS: usize = 8;
const CONTEXT: usize = 6;

#[test]
fn every_hit_round_trips_source_to_projected_to_source() {
    let mut total = 0u64;
    let mut split = 0u64;
    for (path, source) in tier() {
        let mask = project(&source);
        let projected = mask.text(source.as_bytes());
        for hit in Find::literal(COMMON).in_projection(&mask, source.as_bytes()) {
            total += 1;
            let where_ = || format!("{} at {:?}", path.display(), hit.projected);
            // The projected slice is the needle.
            assert_eq!(
                &projected[hit.projected.start as usize..hit.projected.end as usize],
                COMMON,
                "{}",
                where_()
            );
            // The source pieces re-read to the needle, markup skipped.
            assert_eq!(reread(&source, &hit), COMMON, "{}", where_());
            // projected -> source -> projected, on both edges.
            assert_eq!(
                mask.to_source(hit.projected.start),
                hit.source.start(),
                "{}",
                where_()
            );
            assert_eq!(
                mask.from_source(hit.source.start()),
                Some(hit.projected.start),
                "{}",
                where_()
            );
            let last = hit
                .source
                .pieces()
                .last()
                .expect("a hit has a piece")
                .clone();
            assert_eq!(
                mask.from_source(last.end - 1),
                Some(hit.projected.end - 1),
                "{}",
                where_()
            );
            // Every piece is inside a kept range and holds no markup.
            let bytes: u32 = hit.source.pieces().map(|p| p.end - p.start).sum();
            assert_eq!(bytes as usize, COMMON.len(), "{}", where_());
            if hit.source.is_split() {
                split += 1;
            }
        }

        // `the` never straddles a gap — markup does not fall inside a word —
        // so the Split half of the claim is proved where the gaps actually
        // are: a needle spanning a seam, taken from the projection itself and
        // used only when it is unambiguous there.
        for &start in mask.starts.iter().skip(1).take(SEAMS) {
            let seam = start as usize;
            let mut left = seam;
            for _ in 0..CONTEXT {
                match projected[..left].chars().next_back() {
                    Some(scalar) => left -= scalar.len_utf8(),
                    None => break,
                }
            }
            let mut right = seam;
            for _ in 0..CONTEXT {
                match projected[right..].chars().next() {
                    Some(scalar) => right += scalar.len_utf8(),
                    None => break,
                }
            }
            let needle = &projected[left..right];
            if left == seam || right == seam || projected.find(needle) != Some(left) {
                // The window ran off an end, or these bytes repeat earlier and
                // the leftmost hit is somebody else's. Nothing to prove here.
                continue;
            }
            let hit = Find::literal(needle)
                .in_projection(&mask, source.as_bytes())
                .next()
                .expect("the needle came out of this projection");
            assert!(
                hit.source.is_split(),
                "{}: {needle:?} spans the masked gap at {seam} and came back whole",
                path.display()
            );
            assert_eq!(reread(&source, &hit), needle, "{}", path.display());
            split += 1;
        }
    }
    assert!(total > 100_000, "only {total} hits over the tier");
    assert!(
        split > 200,
        "only {split} seams in 12.8 MB produced a Split hit"
    );
}

/// The one place the two rules are both in scope, so the one place their
/// agreement can be asserted rather than asserted about.
#[test]
fn the_restated_word_rule_agrees_with_sous_core_words() {
    let mut checked = 0u64;
    for (path, source) in tier() {
        let mask = project(&source);
        let projected = mask.text(source.as_bytes());

        // Sous's answer: every word occurrence whose folded text is the needle.
        let mut theirs: Vec<(u32, u32)> = Vec::new();
        for_each_word(&projected, &[], |word| {
            let text = &projected[word.from as usize..word.to as usize];
            if text.len() == COMMON.len() && text.eq_ignore_ascii_case(COMMON) {
                theirs.push((word.from, word.to));
            }
        });

        // Galley's answer: every whole-word, case-insensitive hit.
        let ours: Vec<(u32, u32)> = Find::literal(COMMON)
            .case_insensitive(true)
            .whole_word(true)
            .in_projection(&mask, source.as_bytes())
            .map(|hit| (hit.projected.start, hit.projected.end))
            .collect();

        assert_eq!(ours, theirs, "{}", path.display());
        checked += theirs.len() as u64;
    }
    assert!(
        checked > 50_000,
        "only {checked} words compared over the tier"
    );
}
