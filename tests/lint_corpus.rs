//! What lint actually finds in 226 real books — the pinned numbers.
//!
//! Every nonzero class below is EXPLAINED, not tolerated: a lint rule that
//! fires on clean scripture is a bug, so a moving count here is either real
//! data or a regression, and the comments say which. Regenerate with
//! `cargo run --release --bin playground -- --lint-stats example-corpora`.
//!
//! The corpus is gitignored, so this test skips loudly when it is not mounted.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use usfm_onion_2::cst::build;
use usfm_onion_2::lex;
use usfm_onion_2::lint::{Code, LINT_ROWS, lint};

fn collect_usfm_paths(root: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_usfm_paths(&path, paths);
        } else if path.extension().is_some_and(|ext| ext == "usfm") {
            paths.push(path);
        }
    }
}

/// One row per book: `(path, per-code counts, book code as text)`.
type BookReport = (PathBuf, [u64; LINT_ROWS.len()], Option<String>);

fn lint_corpus() -> Option<Vec<BookReport>> {
    let mut paths = Vec::new();
    collect_usfm_paths(Path::new("example-corpora"), &mut paths);
    if paths.is_empty() {
        eprintln!("lint corpus SKIPPED: no *.usfm under example-corpora/");
        return None;
    }
    paths.sort();

    Some(
        paths
            .par_iter()
            .map(|path| {
                let source = std::fs::read_to_string(path).unwrap();
                let tokens = lex(&source);
                let cst = build(&tokens);
                let report = lint(source.as_bytes(), &tokens, &cst);

                let mut counts = [0u64; LINT_ROWS.len()];
                for obs in &report.observations {
                    counts[obs.code as usize] += 1;
                }
                let book = report.book.map(|idx| {
                    let token = tokens[idx as usize];
                    source[token.start as usize..token.end() as usize].to_string()
                });
                (path.clone(), counts, book)
            })
            .collect(),
    )
}

#[test]
fn the_corpus_yields_exactly_the_known_findings() {
    let Some(books) = lint_corpus() else { return };
    assert_eq!(
        books.len(),
        226,
        "corpus size changed — re-read the numbers"
    );

    let mut totals = [0u64; LINT_ROWS.len()];
    for (_, counts, _) in &books {
        for (slot, count) in counts.iter().enumerate() {
            totals[slot] += count;
        }
    }
    let total = |code: Code| totals[code as usize];

    // The three genuinely truncated `\f` the CST has been pointing at since
    // `--cst-stats` landed. These are lint's first real findings and the whole
    // reason the Recovery verdict exists.
    assert_eq!(total(Code::UnclosedNote), 3);

    // Both in examples.bsb, both DOWNSTREAM of an unclosed note: once the note
    // frame is gone its `\f*` closes nothing. GEN's pair sits ~240 bytes
    // apart, which is the signature of exactly this.
    assert_eq!(total(Code::OrphanCloser), 2);

    // `\s5` — the unfoldingWord chunk marker, not a spec marker — in every
    // en_ulb book. It resolves to row 0, so it is BOTH a finding and the
    // walker's pop-all recovery event. This whole count disappears the day
    // custom-marker configuration lands and `\s5` gets a real row; until then
    // it is the honest reading of an unconfigured extension.
    assert_eq!(total(Code::UnknownMarker), 13_636);
    for (path, counts, _) in &books {
        let unknown = counts[Code::UnknownMarker as usize];
        assert_eq!(
            unknown > 0,
            path.to_string_lossy().contains("en_ulb"),
            "{}: unknown markers outside en_ulb",
            path.display()
        );
    }

    // Verses with no paragraph above them, ONE per paragraph-less run — and
    // a run never crosses `\c` (a `\p` inserted in one chapter repairs
    // nothing in the next). Two sources, both real: (a) en_ulb's `\s5` pops
    // the open `\p`, and where no `\p` follows the chunk the verses land at
    // root — 2832, i.e. the same `\s5` story as above; (b) 33 places
    // (31 en_ult, 2 bsb) where a chapter genuinely opens straight into `\v`
    // with no paragraph marker, which is precisely the case usfmtc repairs
    // by fabricating a `\p` and we flag.
    assert_eq!(total(Code::MissingParagraph), 2_865);
    let outside_ulb: u64 = books
        .iter()
        .filter(|(path, _, _)| !path.to_string_lossy().contains("en_ulb"))
        .map(|(_, counts, _)| counts[Code::MissingParagraph as usize])
        .sum();
    assert_eq!(outside_ulb, 33);

    // ---- Ordering + payload (phase 2) -----------------------------------

    // Two designators in 226 books fail their pattern, both genuine typos:
    //   * bdf_reg ACT 8:17 is written `\v +` — a bare note caller where the
    //     verse number belongs (the `\v 18` after it is correct).
    //   * en_ulb ZEC 12:7 is written `\v 7"` with no space before the quote,
    //     so the carved payload is `7"`.
    // The second is why malformed designators RESYNC the sequence instead of
    // merely being skipped: with `prev_verse` left at 6, the perfectly good
    // `\v 8` next to it read as a gap. One typo, one finding.
    assert_eq!(total(Code::DesignatorMalformed), 2);

    // bdf_reg ROM 3 carries `\v 10` twice — the same verse translated twice,
    // the second copy left in. Real duplication, not a range overlap.
    assert_eq!(total(Code::VerseDuplicate), 1);

    // Verses whose marker is simply absent. Every one inspected is real, and
    // they split into two well-known kinds:
    //   * examples.bsb x17 — the classic "omitted verses" (MAT 17:21, 18:11,
    //     23:14; MRK 7:16, 9:44, 9:46, 11:26, 15:28; LUK 17:36, 23:17;
    //     JHN 5:4; ACT 8:37, 15:34, 24:7, 28:29; ROM 16:24), which the BSB
    //     deliberately moves into a footnote, plus PSA 106:42 where the verse
    //     marker is genuinely missing above its own text.
    //   * bdf_reg x11 — a minority-language translation that merges verses
    //     without writing the merge as a range (`\v 7` then `\v 9`).
    // The first group is exactly the future customer for a per-rule off
    // switch (ruled: no versification schemes inside the linter).
    assert_eq!(total(Code::VerseGap), 28);
    let gaps_by_corpus = |corpus: &str| -> u64 {
        books
            .iter()
            .filter(|(path, _, _)| path.to_string_lossy().contains(corpus))
            .map(|(_, counts, _)| counts[Code::VerseGap as usize])
            .sum()
    };
    assert_eq!(gaps_by_corpus("examples.bsb"), 17);
    assert_eq!(gaps_by_corpus("bdf_reg"), 11);

    // examples.bsb LAM 2 opens `\c 2` … `\v 2`: the verse 1 marker is missing
    // above its own text. Reported ONLY as missing-verse-one — never also as
    // a gap, which is the whole point of the two rules being exclusive.
    assert_eq!(total(Code::MissingVerseOne), 1);

    // BSB Ecclesiastes, the same book `LintReport::book == None` has always
    // named. Phase 2 turns that state into an actual observation at token 0.
    assert_eq!(total(Code::MissingId), 1);

    // Everything else is CLEAN across 226 books, and must stay that way: each
    // of these codes fires only on damage the corpus does not contain.
    //
    // Worth naming what the zeros PROVE, because several were the rules most
    // likely to cry wolf: every `\id` in the corpus is one of the spec's 116
    // identifiers, in uppercase; every `\c` owns a number; no book has verses
    // before its first chapter or no chapter at all; and no chapter number
    // repeats, reverses or skips anywhere in 226 books.
    for code in [
        Code::UnclosedChar,
        Code::UnclosedAtEof,
        Code::UnterminatedContainer,
        Code::UnterminatedMilestone,
        Code::OrphanTerminator,
        Code::OrphanContainerEnd,
        Code::ContentOutsideSidebarRule,
        Code::NestedSpellingMisuse,
        Code::ChapterDuplicate,
        Code::ChapterOutOfOrder,
        Code::ChapterGap,
        Code::VerseOutOfOrder,
        Code::VerseBeforeFirstChapter,
        Code::MissingChapter,
        Code::BookCodeUnknown,
        Code::BookCodeNotUppercase,
        Code::ChapterWithoutDesignator,
    ] {
        assert_eq!(total(code), 0, "{} fired on clean data", code.row().name);
    }
}

#[test]
fn the_three_unclosed_notes_are_isa_mrk_and_bsb_gen() {
    let Some(books) = lint_corpus() else { return };

    let mut sites: BTreeMap<String, u64> = BTreeMap::new();
    for (path, counts, book) in &books {
        let count = counts[Code::UnclosedNote as usize];
        if count == 0 {
            continue;
        }
        let corpus = path
            .parent()
            .and_then(|p| p.file_name())
            .unwrap_or_default()
            .to_string_lossy();
        let book = book.clone().unwrap_or_else(|| "?".into());
        *sites.entry(format!("{corpus}/{book}")).or_default() += count;
    }

    assert_eq!(
        sites,
        BTreeMap::from([
            ("en_ulb/ISA".to_string(), 1),
            ("en_ulb/MRK".to_string(), 1),
            ("examples.bsb/GEN".to_string(), 1),
        ])
    );
}

#[test]
fn exactly_one_corpus_book_has_no_id_line() {
    let Some(books) = lint_corpus() else { return };
    // BSB Ecclesiastes. `LintReport::book == None` is the STATE; since phase 2
    // the `missing-id` observation is raised beside it (counted above).
    let missing: Vec<&PathBuf> = books
        .iter()
        .filter(|(_, _, book)| book.is_none())
        .map(|(path, _, _)| path)
        .collect();
    assert_eq!(missing.len(), 1, "books without \\id: {missing:?}");
    assert!(missing[0].to_string_lossy().contains("ECC"));
}
