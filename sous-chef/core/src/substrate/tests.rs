//! Coverage for the walk (walk.rs) and fold (fold.rs) behind the public
//! `ChapterRow`/`BookAggregate` surface, plus the hygiene sites the walk fills.

use super::*;
use crate::pass::{ChapterKey, analyze};
use crate::{BookKey, Chapter, Corpus, ProjectedBook, TextRange, Verse, VerseKey};

fn row(text: &str) -> ChapterRow {
    Substrate.map(ChapterInput {
        text,
        verses: &[],
        key: ChapterKey::new(BookKey::new(*b"MRK"), 1),
    })
}

fn aggregate(chapters: &[&str]) -> BookAggregate {
    let rows: Vec<ChapterRow> = chapters.iter().map(|text| row(text)).collect();
    let view: Vec<ChapterObs<&ChapterRow>> = rows
        .iter()
        .map(|obs| ChapterObs { start: 0, obs })
        .collect();
    fold_book(&view, &mut Edge::default())
}

fn count(row: &ChapterRow, c: char) -> u32 {
    row.scalars()
        .iter()
        .find(|entry| entry.0 == ScalarKey::of(c))
        .map_or(0, |entry| entry.1)
}

fn pair(row: &ChapterRow, c: char, prev: OuterClass, next: OuterClass) -> u32 {
    row.pairs()
        .iter()
        .find(|entry| entry.0 == PairKey::new(ScalarKey::of(c), prev, next))
        .map_or(0, |entry| entry.1)
}

#[test]
fn every_lane_is_sorted_by_key() {
    let row = row("He said, \u{201C}Go.\u{201D} 12,345 \u{967}\u{968}");
    assert!(row.scalars().windows(2).all(|w| w[0].0 < w[1].0));
    assert!(row.pairs().windows(2).all(|w| w[0].0 < w[1].0));
    assert!(row.follows().windows(2).all(|w| w[0].0 < w[1].0));
    let runs: Vec<Vec<ScalarKey>> = row.runs().map(|(atoms, _)| atoms.to_vec()).collect();
    assert!(runs.windows(2).all(|w| w[0] < w[1]));
}

/// Charter invariant 7: one lane, whatever the number system.
#[test]
fn every_decimal_digit_pools_under_one_key() {
    let row = row("7 \u{967}");
    assert_eq!(count(&row, '7'), 0);
    assert_eq!(count(&row, '\u{967}'), 0);
    assert_eq!(
        row.scalars()
            .iter()
            .find(|entry| entry.0 == ScalarKey::DIGITS)
            .map(|entry| entry.1),
        Some(2)
    );
}

/// Charter invariant 8: glue rides its base and never enters the inventory.
#[test]
fn glue_never_appears_as_a_scalar_a_pair_member_or_a_run_member() {
    let row = row("e\u{301} \u{915}\u{94d}\u{937} \u{915}\u{200d}\u{915}");
    for glue in ['\u{301}', '\u{94d}', '\u{200d}'] {
        assert_eq!(count(&row, glue), 0, "{glue:?} is in the inventory");
        assert!(
            row.pairs()
                .iter()
                .all(|entry| entry.0.scalar() != ScalarKey::of(glue)),
            "{glue:?} is a pair member"
        );
        assert!(
            row.runs()
                .all(|(atoms, _)| !atoms.contains(&ScalarKey::of(glue))),
            "{glue:?} is a run member"
        );
    }
}

/// The Swahili convention: an apostrophe wholly inside a word.
#[test]
fn a_word_medial_apostrophe_reads_letter_to_letter() {
    let row = row("ng'ombe ng'ombe");
    assert_eq!(pair(&row, '\'', OuterClass::Letter, OuterClass::Letter), 2);
    // A word is letters, glue, and digits, so the apostrophe splits one.
    assert_eq!(row.word_count(), 4);
}

#[test]
fn a_run_of_the_same_glyph_lands_in_its_own_length_bucket() {
    let lengths = row("a,b,,c,,,d,,,,,,,e").run_lengths();
    let commas = lengths
        .iter()
        .find(|entry| entry.0 == ScalarKey::of(','))
        .expect("commas");
    assert_eq!(commas.1, [1, 1, 1, 0, 0, 1]);
}

#[test]
fn a_terminal_records_the_casing_of_the_letter_it_hands_off_to() {
    let row = row("one. Two. three. \u{5d0}");
    let follows = row.follows();
    let dot = follows
        .iter()
        .find(|entry| entry.0 == ScalarKey::of('.'))
        .expect("the terminal");
    assert_eq!(dot.1.get(Case::Upper), 1);
    assert_eq!(dot.1.get(Case::Lower), 1);
    assert_eq!(dot.1.get(Case::Uncased), 1);
}

#[test]
fn an_empty_chapter_is_all_default_edges() {
    let row = row("");
    assert_eq!(row.lead(), Edge::default());
    assert_eq!(row.trail(), Edge::default());
    assert_eq!(row.scalar_count(), 0);
    assert_eq!(row.word_count(), 0);
}

#[test]
fn mapping_the_same_chapter_twice_gives_identical_rows() {
    let text = "He said, \u{201C}Go.\u{201D} 12,345 \u{915}\u{94d}\u{937}a";
    assert_eq!(row(text), row(text));
}

#[test]
fn a_crlf_variant_differs_only_where_the_cr_is() {
    let lf = row("one.\ntwo,\nthree");
    let crlf = row("one.\r\ntwo,\r\nthree");
    assert_eq!(count(&crlf, '\r'), 2);
    assert_eq!(count(&lf, '\r'), 0);
    let without_cr = |row: &ChapterRow| -> Vec<(ScalarKey, u32)> {
        row.scalars()
            .iter()
            .filter(|entry| entry.0 != ScalarKey::of('\r'))
            .copied()
            .collect()
    };
    assert_eq!(without_cr(&lf), without_cr(&crlf));
    assert_eq!(lf.pairs(), crlf.pairs());
    assert_eq!(lf.follows(), crlf.follows());
    assert_eq!(lf.word_count(), crlf.word_count());
    assert_eq!(lf.scalar_count() + 2, crlf.scalar_count());
}

fn assert_seam_agrees(whole: &str, split: &[&str]) {
    let one = aggregate(&[whole]);
    let many = aggregate(split);
    assert_eq!(one.pairs(), many.pairs(), "pairs across {split:?}");
    assert_eq!(one.follows(), many.follows(), "follows across {split:?}");
    assert_eq!(one.scalars(), many.scalars(), "scalars across {split:?}");
    assert_eq!(
        one.word_count(),
        many.word_count(),
        "words across {split:?}"
    );
}

#[test]
fn a_seam_resolves_to_the_counts_of_the_unsplit_text() {
    assert_seam_agrees("one. Two, three", &["one. ", "Two, three"]);
    assert_seam_agrees("one. Two, three", &["one.", " Two, three"]);
    assert_seam_agrees("one.Two", &["one.", "Two"]);
    assert_seam_agrees("a,b", &["a", ",", "b"]);
    assert_seam_agrees("one. \u{201C}Two", &["one. ", "\u{201C}Two"]);
    assert_seam_agrees("word", &["wo", "rd"]);
    assert_seam_agrees("one.  Two", &["one. ", " ", "Two"]);
    assert_seam_agrees("one. Two", &["one.", "", " Two"]);
}

/// The hygiene ruling, kept: a run abutting a masked `\c` is two runs.
#[test]
fn a_run_straddling_a_seam_stays_two_runs() {
    let one = aggregate(&["a,,b"]);
    let split = aggregate(&["a,", ",b"]);
    let shapes = |book: &BookAggregate| -> Vec<(Vec<ScalarKey>, u32)> {
        book.runs()
            .map(|(atoms, count)| (atoms.to_vec(), count))
            .collect()
    };
    assert_eq!(shapes(&one), vec![(vec![ScalarKey::of(','); 2], 1)]);
    assert_eq!(shapes(&split), vec![(vec![ScalarKey::of(',')], 2)]);
    // Everything else still reads as one string.
    assert_eq!(one.pairs(), split.pairs());
}

struct Book {
    key: BookKey,
    text: &'static str,
    chapters: Vec<Chapter>,
    verses: Vec<Verse>,
}

impl ProjectedBook for Book {
    fn key(&self) -> BookKey {
        self.key
    }

    fn text(&self) -> &str {
        self.text
    }

    fn chapters(&self) -> impl Iterator<Item = Chapter> {
        self.chapters.iter().copied()
    }

    fn verses(&self) -> impl Iterator<Item = Verse> {
        self.verses.iter().copied()
    }
}

fn book(key: &[u8; 3], text: &'static str, len: u32) -> Book {
    Book {
        key: BookKey::new(*key),
        text,
        chapters: vec![Chapter::new(1, TextRange::new(0, len).unwrap()).unwrap()],
        verses: vec![Verse::new(
            VerseKey::new(1, 1, 1).unwrap(),
            TextRange::new(0, len).unwrap(),
        )],
    }
}

/// Charter invariant 2: no state crosses a book, so the second book's
/// first chapter sees `Edge` on its left, not the first book's last scalar.
#[test]
fn every_book_folds_from_a_fresh_edge() {
    let books = vec![book(b"GEN", "a,", 2), book(b"MRK", ",b", 2)];
    let corpus = Corpus::try_new(&books).unwrap();
    // These books hold no hygiene site, so judging emits nothing; the
    // seam claim is the three folds below.
    assert!(analyze(&corpus, &Substrate).is_empty());

    // The same two chapters inside one book resolve their seam; in two
    // books each keeps the `Edge` its own end saw.
    assert_eq!(
        aggregate(&["a,", ",b"]).pairs().to_vec(),
        vec![
            (
                PairKey::new(
                    ScalarKey::of(','),
                    OuterClass::Letter,
                    OuterClass::Nonletter
                ),
                1
            ),
            (
                PairKey::new(
                    ScalarKey::of(','),
                    OuterClass::Nonletter,
                    OuterClass::Letter
                ),
                1
            )
        ]
    );
    assert_eq!(
        aggregate(&["a,"]).pairs().to_vec(),
        vec![(
            PairKey::new(ScalarKey::of(','), OuterClass::Letter, OuterClass::Edge),
            1
        )]
    );
    assert_eq!(
        aggregate(&[",b"]).pairs().to_vec(),
        vec![(
            PairKey::new(ScalarKey::of(','), OuterClass::Edge, OuterClass::Letter),
            1
        )]
    );
}

/// The lanes are what a resident cache pays per chapter, so the inline
/// size is a fact worth pinning: it is 20% of the tier's median row. The
/// hygiene lane is 16 of these bytes and is almost always empty.
#[test]
fn the_row_and_its_edges_are_the_size_the_budget_assumes() {
    assert_eq!(size_of::<ChapterRow>(), 144);
    assert_eq!(size_of::<Edge>(), 20);
}

/// Hygiene's four scalar classes, read off the lane the walk fills.
mod hygiene_sites {
    use crate::HygieneClass;

    fn rows(text: &str) -> Vec<(HygieneClass, u32, u32, u32)> {
        super::row(text)
            .hygiene()
            .iter()
            .map(|f| (f.class(), f.span().from(), f.span().to(), f.run()))
            .collect()
    }

    #[test]
    fn scalar_doc_example_is_exact() {
        let text = "a \u{301} \u{feff}b\u{a0}\u{a0}c\u{fdd0}";
        assert_eq!(
            rows(text),
            vec![
                (HygieneClass::FreeCombiningMark, 1, 4, 1),
                (HygieneClass::MisplacedFormat, 5, 8, 1),
                (HygieneClass::NoBreakSpace, 9, 13, 2),
                (HygieneClass::Noncharacter, 14, 17, 1),
            ]
        );
    }

    #[test]
    fn a_decomposed_graphemes_combining_mark_is_not_a_free_mark() {
        // rules/hygiene.md, required examples.
        assert!(rows("e\u{301}tait").is_empty());
        assert!(rows("\u{3b1}\u{314}\u{301}").is_empty());
        // Marks stacked on a real base stay silent however deep.
        assert!(rows("a\u{301}\u{308}\u{327}").is_empty());
    }

    #[test]
    fn a_bare_combining_mark_after_a_space_or_at_the_start_is_reported() {
        assert_eq!(
            rows("word \u{301}\u{308} next"),
            vec![(HygieneClass::FreeCombiningMark, 4, 9, 2)]
        );
        assert_eq!(
            rows("\u{301}word"),
            vec![(HygieneClass::FreeCombiningMark, 0, 2, 1)]
        );
        // A mark behind a control has no base either; the control run is
        // `HygieneBytes`' own row, not the walk's.
        assert_eq!(
            rows("a\0\u{301}b"),
            vec![(HygieneClass::FreeCombiningMark, 2, 4, 1)]
        );
    }

    #[test]
    fn zwj_and_zwnj_between_letters_are_silent() {
        // ZWJ/ZWNJ in Indic text never enters the inventory.
        assert!(rows("\u{915}\u{94d}\u{200d}\u{937}").is_empty());
        assert!(rows("\u{915}\u{94d}\u{200c}\u{937}").is_empty());
        assert!(rows("\u{62a}\u{200c}\u{62a}").is_empty());
        // The same joiner with nothing to join is reportable.
        assert_eq!(
            rows("\u{200d} a"),
            vec![(HygieneClass::MisplacedFormat, 0, 3, 1)]
        );
    }

    #[test]
    fn a_stray_byte_order_mark_mid_text_is_reported() {
        assert_eq!(
            rows("in the\u{feff} beginning"),
            vec![(HygieneClass::MisplacedFormat, 6, 9, 1)]
        );
        // An Arabic number sign is Prepend: introducing a digit is its job.
        assert!(rows("\u{600}7").is_empty());
        // The span takes the space with it: a Prepend owns what follows.
        assert_eq!(
            rows("\u{600} "),
            vec![(HygieneClass::MisplacedFormat, 0, 3, 1)]
        );
    }

    #[test]
    fn noncharacters_are_reported_as_runs() {
        assert_eq!(
            rows("a\u{fdd0}\u{fdd1}b"),
            vec![(HygieneClass::Noncharacter, 1, 7, 2)]
        );
        assert_eq!(
            rows("a\u{ffff}b"),
            vec![(HygieneClass::Noncharacter, 1, 4, 1)]
        );
        assert_eq!(
            rows("a\u{10fffe}b"),
            vec![(HygieneClass::Noncharacter, 1, 5, 1)]
        );
        // U+FFFD keeps its own class and is a byte sweep's row, not one here.
        assert!(rows("a\u{fffd}b").is_empty());
    }

    #[test]
    fn nbsp_speaks_only_where_the_claim_is_deterministic() {
        // French spacing around punctuation is convention, not damage.
        assert!(rows("J\u{e9}sus\u{a0}: parle").is_empty());
        assert!(rows("\u{ab}\u{a0}mot\u{a0}\u{bb}").is_empty());
        assert_eq!(
            rows("word \u{a0}next"),
            vec![(HygieneClass::NoBreakSpace, 5, 7, 1)]
        );
        assert_eq!(
            rows("word\u{a0} next"),
            vec![(HygieneClass::NoBreakSpace, 4, 6, 1)]
        );
        assert_eq!(
            rows("\u{a0}word"),
            vec![(HygieneClass::NoBreakSpace, 0, 2, 1)]
        );
        assert_eq!(
            rows("word\u{a0}"),
            vec![(HygieneClass::NoBreakSpace, 4, 6, 1)]
        );
    }

    #[test]
    fn every_emitted_span_lies_on_atom_boundaries() {
        let text = "a\u{301}\0\u{301} \u{a0}\u{a0}\u{fdd0}\\\u{feff}\r\n\u{915}\u{94d}\u{937}";
        for finding in super::row(text).hygiene() {
            let span = finding.span();
            assert_eq!(
                crate::unicode::atoms::widen_to_atoms(text, span),
                span,
                "{:?} at {}..{} is not atom-aligned",
                finding.class(),
                span.from(),
                span.to()
            );
        }
    }

    /// A chapter end is an edge of text, so a run abutting a masked `\c`
    /// is one finding per chapter — the hygiene ruling, kept.
    #[test]
    fn a_site_run_stops_at_the_chapter_edge() {
        assert_eq!(
            rows("a \u{301}\u{301}"),
            vec![(HygieneClass::FreeCombiningMark, 1, 6, 2)]
        );
        assert_eq!(
            rows("a \u{301}"),
            vec![(HygieneClass::FreeCombiningMark, 1, 4, 1)]
        );
        assert_eq!(
            rows("\u{301}"),
            vec![(HygieneClass::FreeCombiningMark, 0, 2, 1)]
        );
    }
}

const fn detached<O: Send + 'static>() {}

#[test]
fn a_substrate_observation_is_send_and_borrow_free() {
    detached::<<Substrate as ChapterPass>::Observation>();
}
