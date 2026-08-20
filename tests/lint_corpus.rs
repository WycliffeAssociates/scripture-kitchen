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
use usfm_onion_2::lint::{Code, LINT_ROWS, check_fixes, lint};

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

    // The two genuinely truncated `\f` the CST has been pointing at since
    // `--cst-stats` landed. These are lint's first real findings and the whole
    // reason the Recovery verdict exists.
    //
    // Was 3 until 2026-08-19: bsb GEN 2:4's footnote was never truncated, it
    // was DISPLACED by the `\+nd` inside it, because no character row carried
    // the Footnote context. Will's class-wide curation (see the note above the
    // `add` row in tables::rows) gave every character row Footnote and
    // CrossReference, and that footnote now closes Explicit at its own `\f*`.
    assert_eq!(total(Code::UnclosedNote), 2);

    // examples.bsb 1SA 16:9 writes `\+xt 2 Samuel 13:3, \+xt 2 Samuel
    // 21:21\+xt* and \+xt* …` — one more `\+xt*` than there are opens. A real
    // authoring slip in the BSB, and the only orphan closer in 226 books.
    //
    // Was 2 until 2026-08-19: the other one, bsb GEN @5796, WAS downstream of
    // the displaced note above — once the note frame was gone its `\f*` closed
    // nothing. Fixing the table fixed the `\f*` with it.
    assert_eq!(total(Code::OrphanCloser), 1);

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

    // ---- Token-walk rules (phase 3) --------------------------------------

    // One book mixing bare and numbered spellings of one family, said once per
    // family per book. All 52 are real and all are the poetry ladder:
    //   * en_ulb x44 — books carrying `\q1`/`\q2` throughout and a handful of
    //     bare `\q` lines (8417 bare against 14755 numbered corpus-wide).
    //   * bdf_reg x6 — the mirror image: mostly bare `\q`, a few `\q1`/`\q2`.
    //   * en_ult x2 — PSA has one bare `\q` among 14k numbered ones; FRT is
    //     front matter with a stray second spelling.
    //   * examples.bsb x0 — it uses `\q1`/`\q2` and bare `\q` in DIFFERENT
    //     books, which is exactly what per-book aggregation is for.
    assert_eq!(total(Code::NumberingMix), 52);
    let by_corpus = |code: Code, corpus: &str| -> u64 {
        books
            .iter()
            .filter(|(path, _, _)| path.to_string_lossy().contains(corpus))
            .map(|(_, counts, _)| counts[code as usize])
            .sum()
    };
    assert_eq!(by_corpus(Code::NumberingMix, "en_ulb"), 44);
    assert_eq!(by_corpus(Code::NumberingMix, "bdf_reg"), 6);
    assert_eq!(by_corpus(Code::NumberingMix, "en_ult"), 2);
    assert_eq!(by_corpus(Code::NumberingMix, "examples.bsb"), 0);

    // ONE marker in 113 MB is followed by something that is not structural
    // whitespace: en_ulb REV writes `\m(for fine linen is the righteous
    // acts…)` with no space after the marker name. A genuine typo, and the
    // only one — which is the evidence that this rule is narrow enough.
    assert_eq!(total(Code::DelimiterShape), 1);

    // Paragraphs with nothing in them. Info-tier, and every one inspected is a
    // real (if harmless) authoring artifact:
    //   * en_ulb x726 — the `\s5` chunk idiom writes `\m` and then `\p` on the
    //     next line, so the `\m` never gets any content.
    //   * en_ult x59 — a `\p` immediately followed by `\s1`, and empty `\d`
    //     descriptive titles above a psalm's first `\q1`.
    //   * examples.bsb x2 — an empty `\d` (ZEC 12) and an empty `\q1` used as
    //     a spacer between two `\q2` lines (PSA 106).
    // `\b`, the paragraph that is empty BY DESIGN, is excluded by the rule and
    // appears in all four corpora — so a zero here would be the bug.
    assert_eq!(total(Code::EmptyParagraph), 787);
    assert_eq!(by_corpus(Code::EmptyParagraph, "en_ulb"), 726);
    assert_eq!(by_corpus(Code::EmptyParagraph, "en_ult"), 59);
    assert_eq!(by_corpus(Code::EmptyParagraph, "examples.bsb"), 2);
    assert_eq!(by_corpus(Code::EmptyParagraph, "bdf_reg"), 0);

    // Everything else is CLEAN across 226 books, and must stay that way: each
    // of these codes fires only on damage the corpus does not contain.
    //
    // Worth naming what the zeros PROVE, because several were the rules most
    // likely to cry wolf: every `\id` in the corpus is one of the spec's 116
    // identifiers, in uppercase; every `\c` owns a number; no book has verses
    // before its first chapter or no chapter at all; and no chapter number
    // repeats, reverses or skips anywhere in 226 books.
    //
    // Phase 3 adds four zeros that are the same kind of evidence, each one a
    // rule that would otherwise have buried the report:
    //   * marker-not-ws-preceded — NARROWED to paragraph rows. Unnarrowed it
    //     reports every hugging character marker, i.e. most of en_ult's 6.5M
    //     aligned tokens.
    //   * attr-trailing-form-deprecated — en_ult declares `\usfm 3.0` and
    //     contains 792,414 trailing `\w` lists, every one of them the correct
    //     spelling for its declared version. The rule reads that declaration.
    //   * attr-both-lists / attr-terminator-mismatch — no book writes two
    //     lists on one marker, and every list's terminator belongs to its
    //     owner. `\zaln-s |…\*` alone accounts for a million of the latter.
    //   * attr-pipe-hint / caller-shape — no stray pipe survives inside an
    //     attrs-capable marker, and every note caller in the corpus is `+`
    //     (5661 of them) or a mark of at most three bytes.
    //
    // The closeout window adds five more zeros, and each one is a DIFFERENT
    // kind of evidence — three of them ride sweeps that do a great deal of
    // work to say nothing:
    //   * attr-unknown-name — the k/v interpreter resolves all 4,352,929
    //     attributes in the corpus (tests/attr_corpus.rs pins that total and
    //     reconciles it against a grep), and every single one is an `x-` name
    //     on a `\w` or a `\zaln-s`. Not one canonical name, and not one bare
    //     default value, in 226 books: `AttrResolution::UserNamespace` every
    //     time. The rule is nevertheless the one most likely to cry wolf if
    //     that namespace were ever mishandled — a million findings, instantly.
    //   * attr-malformed — reconciles EXACTLY with the interpreter's own
    //     corpus pin (`total.malformed == 0` over the same 1,253,766 lists).
    //     Two sweeps, one number: if either moves alone, one of them is wrong.
    //   * attr-required-if — no `sid=` and no `\ta` anywhere in the corpus
    //     (checked at the bytes). en_ult's alignment is `x-`-namespaced on
    //     `\zaln-s`, which is a custom `\z` row, so the milestone pairing this
    //     rule watches simply does not occur here yet.
    //   * deprecated-attribute — `AttrStatus::Deprecated` exists on exactly
    //     four names (`\xt`'s `link-href` and `\jmp`'s `link-` trio) and not
    //     one of them appears in the corpus.
    //   * deprecated-marker — two independent reasons, and both are worth
    //     knowing: 67 books declare `\usfm 3.0` (the rest declare nothing, so
    //     the GATE alone would silence them), and not one occurrence of
    //     `\addpn`, `\fdc`, `\ph`, `\pro` or `\xdc` exists in 113 MB.
    //   * marker-out-of-band — the loudest zero. 65 rows have purely
    //     positional masks and take part (`\id`, `\h`, `\toc#`, `\mt#`, the
    //     whole introduction ladder, `\c`, every paragraph row): 111,000-odd
    //     occurrences drive 679 band transitions across the 226 books, and not
    //     one marker looks backwards. The corpora write their front matter in
    //     spec order, which is exactly what the mask's positional half has
    //     always claimed and nothing had checked.
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
        Code::CaCpPlacement,
        Code::VaVpPlacement,
        Code::CallerShape,
        Code::MarkerNotWsPreceded,
        Code::AttrTrailingFormDeprecated,
        Code::AttrBothLists,
        Code::AttrTerminatorMismatch,
        Code::AttrPipeHint,
        Code::AttrUnknownName,
        Code::AttrMalformed,
        Code::AttrRequiredIf,
        Code::DeprecatedMarker,
        Code::DeprecatedAttribute,
        Code::MarkerOutOfBand,
    ] {
        assert_eq!(total(code), 0, "{} fired on clean data", code.row().name);
    }
}

#[test]
fn the_two_unclosed_notes_are_isa_and_mrk() {
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
        // examples.bsb/GEN dropped out on 2026-08-19 — it was a displaced
        // note, not a truncated one. See the unclosed-note pin above.
        BTreeMap::from([("en_ulb/ISA".to_string(), 1), ("en_ulb/MRK".to_string(), 1),])
    );
}

/// THE FIX ORACLE over the whole corpus: apply → re-lex → re-build → re-lint,
/// for EVERY fix this corpus offers, one at a time.
///
/// Each one must repair its own finding and introduce none — the composability
/// rule, executable ([`usfm_onion_2::lint::check_fixes`] states the four
/// conditions). It is the load-bearing test of phase 4: a fix that is merely
/// plausible on a snippet meets 226 real books here.
#[test]
fn every_corpus_fix_passes_the_oracle() {
    let mut paths = Vec::new();
    collect_usfm_paths(Path::new("example-corpora"), &mut paths);
    if paths.is_empty() {
        eprintln!("fix oracle SKIPPED: no *.usfm under example-corpora/");
        return;
    }
    paths.sort();

    // (fixes exercised, per-code counts) — the oracle is only evidence if it
    // actually ran, so the count is pinned like every other number here.
    let counted: Vec<[u64; LINT_ROWS.len()]> = paths
        .par_iter()
        .map(|path| {
            let source = std::fs::read_to_string(path).unwrap();
            let tokens = lex(&source);
            let cst = build(&tokens);
            let report = lint(source.as_bytes(), &tokens, &cst);

            let mut counts = [0u64; LINT_ROWS.len()];
            for (index, obs) in report.observations.iter().enumerate() {
                let Some(fix) = report.fix(index) else {
                    continue;
                };
                assert_eq!(
                    Some(fix.label),
                    obs.code.row().fix_label,
                    "{}: {} emitted a fix its row does not declare",
                    path.display(),
                    obs.code.row().name
                );
                check_fixes(&source, &tokens, &report, &[index as u32]).unwrap_or_else(|error| {
                    panic!("{}: {} — {error}", path.display(), obs.code.row().name)
                });
                counts[obs.code as usize] += 1;
            }
            counts
        })
        .collect();

    let mut totals = [0u64; LINT_ROWS.len()];
    for counts in &counted {
        for (slot, count) in counts.iter().enumerate() {
            totals[slot] += count;
        }
    }
    let total = |code: Code| totals[code as usize];

    // The two truncated footnotes and the one orphan `\+xt*` — every
    // structural finding in 226 books, each with a repair. (Both counts fell
    // by one on 2026-08-19 with the character-in-note curation; see the pins
    // in `the_corpus_yields_exactly_the_known_findings`.)
    assert_eq!(total(Code::UnclosedNote), 2);
    assert_eq!(total(Code::OrphanCloser), 1);
    // One `\p` per paragraph-less run, all 2865 of them.
    assert_eq!(total(Code::MissingParagraph), 2_865);
    // The corpus's ONE duplicate verse is deliberately NOT among these: bdf_reg
    // ROM 3 writes `\v 10` twice and then `\v 11`, so renumbering the duplicate
    // to 11 would only move the duplicate one verse along. The fix declines
    // (see `renumber`), which is why this reads 0 while the finding count above
    // reads 1 — and it is exactly the case that taught the guard.
    assert_eq!(total(Code::VerseDuplicate), 0);
    assert_eq!(totals.iter().sum::<u64>(), 2_868);
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
