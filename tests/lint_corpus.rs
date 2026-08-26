//! What lint finds in 226 real books — the pinned numbers.
//!
//! Every nonzero class below is EXPLAINED, not tolerated: a rule that fires on
//! clean scripture is a bug, so a moving count is either real data or a
//! regression. Regenerate with
//! `cargo run --release --bin playground -- --lint-stats example-corpora`.
//!
//! The corpus is gitignored, so this test skips loudly when it is not mounted.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use usfm_onion::cst::build;
use usfm_onion::lex;
use usfm_onion::lint::{Code, LINT_ROWS, NO_TOKEN, check_fixes, lint};

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
                    // THE ANCHOR INVARIANT: the other party always PRECEDES the
                    // anchor. `second` is always an opener, an owner, the
                    // previous in sequence or a first occurrence — all behind
                    // the reported token — and consumers (the report's sort,
                    // span highlighting) are built on that.
                    assert!(
                        obs.second == NO_TOKEN || obs.second < obs.anchor,
                        "{}: {} put its second party at or after its anchor ({obs:?})",
                        path.display(),
                        obs.code.row().name,
                    );
                    counts[obs.code as usize] += 1;
                }
                let book = report.book.map(|idx| {
                    let token = tokens[idx as usize];
                    // The code alone: the span carries the folded delimiter.
                    source[token.start as usize..token.end() as usize]
                        .trim_end()
                        .to_string()
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

    // Two genuinely truncated `\f`, in en_ulb ISA and MRK — lint's only real
    // structural findings, and the reason the Recovery verdict exists.
    assert_eq!(total(Code::UnclosedNote), 2);

    // examples.bsb 1SA 16:9 writes `\+xt 2 Samuel 13:3, \+xt 2 Samuel
    // 21:21\+xt* and \+xt* …` — one more `\+xt*` than there are opens. A real
    // authoring slip in the BSB, and the only orphan closer in 226 books.
    assert_eq!(total(Code::OrphanCloser), 1);

    // `\s5` — the unfoldingWord chunk marker, not a spec marker — in every
    // en_ulb book. It resolves to row 0, so it is BOTH a finding and the
    // walker's pop-all recovery event. The count goes away when custom-marker
    // configuration lands; until then it is the honest reading of an
    // unconfigured extension.
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

    // Verses with no paragraph above them, ONE per paragraph-less run. A run
    // ends wherever the repairing `\p` could not survive: at `\c`, and at the
    // pop-all recovery of a row-0 marker. Two real sources: en_ulb's `\s5` pops
    // the open `\p` and the following verses land at root (5401), plus 33 places
    // (31 en_ult, 2 bsb) where a chapter opens straight into `\v`. usfmtc
    // repairs the latter by fabricating a `\p`; we flag it.
    //
    // The counter now asks whether the paragraph above is VERSE-BEARING
    // (`V_FORBIDDEN_IN_PARAGRAPHS`), and this number does NOT move — which is
    // the finding, not an absence of one. Of the 18 v-forbidden rows only `\qa`
    // and `\lit` actually HOLD a verse under today's context masks; the other
    // sixteen (`\s`, `\ip`, `\r`, `\cl`, `\sp`, `\d`'s neighbours…) are
    // DISPLACED by the `\v`, so the verse was already at the root and already
    // counted. The corpus's 88 `\qa` are all Psalm 119 acrostic headings with a
    // `\q1` between the heading and the verse, and `\lit` appears nowhere.
    assert_eq!(total(Code::MissingParagraph), 5_434);
    let outside_ulb: u64 = books
        .iter()
        .filter(|(path, _, _)| !path.to_string_lossy().contains("en_ulb"))
        .map(|(_, counts, _)| counts[Code::MissingParagraph as usize])
        .sum();
    assert_eq!(outside_ulb, 33);

    let by_corpus = |code: Code, corpus: &str| -> u64 {
        books
            .iter()
            .filter(|(path, _, _)| path.to_string_lossy().contains(corpus))
            .map(|(_, counts, _)| counts[code as usize])
            .sum()
    };

    // ---- Ordering + payload ----------------------------------------------

    // ONE designator fails its pattern: en_ulb ZEC 12:7 writes `\v 7"` with no
    // space before the quote, so the carved payload is `7"`. It still TOKENIZES
    // under the designator gate — a leading digit is the gate's whole test — and
    // the interpreter rejects it. This is why a malformed designator RESYNCS the
    // sequence rather than being skipped: leaving `prev_verse` at 6 makes the
    // good `\v 8` next to it read as a gap. One typo, one finding.
    assert_eq!(total(Code::DesignatorMalformed), 1);

    // The gate MOVED the corpus's other typo into this lane, one for one:
    // bdf_reg ACT 8:17 is `\v +` — a bare note caller where the verse number
    // belongs (the `\v 18` after it is correct). `+` is not a digit, so no
    // designator is carved at all and the `\v` names no verse, which is the
    // same fact `\v \p` and a bare `\v` state. Before the gate this counted as
    // designator-malformed; the total across both codes is unchanged at 2.
    assert_eq!(total(Code::VerseWithoutDesignator), 1);
    assert_eq!(by_corpus(Code::VerseWithoutDesignator, "bdf_reg"), 1);
    // …and it offers NO fix, which is the whole point of the empty gate: `\v +`
    // holds a caller, so the verse's identity is the translator's to write. The
    // deletion fix is for the EMPTY spelling (`\v \v \v 1 text`), of which 226
    // books hold none.

    // bdf_reg ROM 3 carries `\v 10` twice — the same verse translated twice,
    // the second copy left in. Real duplication, not a range overlap.
    assert_eq!(total(Code::VerseDuplicate), 1);

    // Verses whose marker is absent — two real kinds. examples.bsb x17 is the
    // classic "omitted verses" the BSB deliberately moves into a footnote (MAT
    // 17:21 … ROM 16:24), plus PSA 106:42 where the marker really is missing;
    // bdf_reg x11 merges verses without writing the merge as a range (`\v 7`
    // then `\v 9`). The first group is the future customer for a per-rule off
    // switch: no versification schemes go inside the linter.
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
    // above its own text. Reported ONLY as missing-verse-one, never also as a
    // gap — the two rules are exclusive.
    assert_eq!(total(Code::MissingVerseOne), 1);

    // BSB Ecclesiastes: the one book with no `\id` at all, raised as an
    // observation at token 0 beside `LintReport::book == None`.
    assert_eq!(total(Code::MissingId), 1);

    // ---- Token-walk rules ------------------------------------------------

    // One book mixing bare and numbered spellings of one family, said once per
    // family per book. All 52 are the poetry ladder: en_ulb x44 mostly
    // `\q1`/`\q2` with stray bare `\q`, bdf_reg x6 the mirror image, en_ult x2.
    // examples.bsb reads 0 because it uses the two spellings in DIFFERENT
    // books — which is what per-book aggregation is for.
    assert_eq!(total(Code::NumberingMix), 52);
    assert_eq!(by_corpus(Code::NumberingMix, "en_ulb"), 44);
    assert_eq!(by_corpus(Code::NumberingMix, "bdf_reg"), 6);
    assert_eq!(by_corpus(Code::NumberingMix, "en_ult"), 2);
    assert_eq!(by_corpus(Code::NumberingMix, "examples.bsb"), 0);

    // ONE marker in 113 MB is followed by something that is not structural
    // whitespace: en_ulb REV writes `\m(for fine linen is the righteous
    // acts…)`. A genuine typo, and the only one — the evidence that this rule
    // is narrow enough.
    assert_eq!(total(Code::DelimiterShape), 1);

    // Paragraphs with nothing in them, info-tier, all harmless authoring
    // artifacts: en_ulb x726 (the `\s5` chunk idiom writes `\m` then `\p` on
    // the next line), en_ult x59 (`\p` then `\s1`, empty `\d` psalm titles),
    // examples.bsb x2 (an empty `\d` and a `\q1` used as a spacer). `\b`, the
    // paragraph empty BY DESIGN, is excluded by the rule and appears in all
    // four corpora — so a zero here would be the bug.
    assert_eq!(total(Code::EmptyParagraph), 787);
    assert_eq!(by_corpus(Code::EmptyParagraph, "en_ulb"), 726);
    assert_eq!(by_corpus(Code::EmptyParagraph, "en_ult"), 59);
    assert_eq!(by_corpus(Code::EmptyParagraph, "examples.bsb"), 2);
    assert_eq!(by_corpus(Code::EmptyParagraph, "bdf_reg"), 0);

    // Everything else is CLEAN across 226 books and must stay that way. What
    // the zeros PROVE — several of these are the rules most likely to cry wolf:
    //
    //   * ids/chapters — every `\id` is one of the spec's 116 identifiers, in
    //     uppercase; every `\c` owns a number; no book lacks a chapter or puts
    //     verses before its first; no chapter number repeats, reverses, skips.
    //   * marker-not-ws-preceded — NARROWED to paragraph rows; unnarrowed it
    //     reports most of en_ult's 6.5M aligned tokens.
    //   * attr-trailing-form-deprecated — en_ult declares `\usfm 3.0` and its
    //     792,414 trailing `\w` lists are correct for that version, so the rule
    //     must read the declaration rather than the spelling.
    //   * attr-both-lists / attr-terminator-mismatch / attr-pipe-hint /
    //     caller-shape — no book writes two lists on one marker, every
    //     terminator belongs to its owner (`\zaln-s |…\*` alone is a million of
    //     them), no stray pipe survives inside an attrs-capable marker, and
    //     every caller is `+` (5661) or a mark of at most three bytes.
    //   * attr-unknown-name — all 4,352,929 attributes resolve, and every one
    //     is an `x-` name on a `\w` or `\zaln-s`: `UserNamespace` every time.
    //     Mishandle that namespace and this becomes a million findings.
    //   * attr-malformed — reconciles EXACTLY with the interpreter's own pin in
    //     tests/attr_corpus.rs over the same 1,253,766 lists. Two sweeps, one
    //     number: if either moves alone, one of them is wrong.
    //   * attr-required-if — no `sid=` and no `\ta` anywhere in the corpus, so
    //     the milestone pairing this rule watches does not occur here yet.
    //   * deprecated-attribute / deprecated-marker — the four deprecated names
    //     (`\xt`'s `link-href`, `\jmp`'s `link-` trio) and the five deprecated
    //     markers occur nowhere in 113 MB; the version GATE is not doing the
    //     silencing, since 67 books do declare `\usfm 3.0`.
    //   * marker-out-of-band — the loudest zero: 65 positional-mask rows,
    //     111,000-odd occurrences, 679 band transitions, and not one marker
    //     looks backwards. The corpora write front matter in spec order, which
    //     is what the mask's positional half claims.
    // The FORM CHANNEL is structurally absent, not merely clean: `lint` does not
    // evaluate a `Severity::Form` row at all, so none can reach a report.
    for row in LINT_ROWS.iter().filter(|row| row.is_form()) {
        assert_eq!(
            total(row.code),
            0,
            "{} escaped into a lint report",
            row.name
        );
    }

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
        // A note DISPLACED by a nested char marker is not a truncated one, and
        // does not belong here (examples.bsb/GEN is the case that taught it).
        BTreeMap::from([("en_ulb/ISA".to_string(), 1), ("en_ulb/MRK".to_string(), 1),])
    );
}

/// THE FIX ORACLE: apply → re-lex → re-build → re-lint, for EVERY fix this
/// corpus offers, one at a time. Each must repair its own finding and introduce
/// none — the composability rule made executable
/// ([`usfm_onion::lint::check_fixes`] states the four conditions).
#[test]
fn every_corpus_fix_passes_the_oracle() {
    let mut paths = Vec::new();
    collect_usfm_paths(Path::new("example-corpora"), &mut paths);
    if paths.is_empty() {
        eprintln!("fix oracle SKIPPED: no *.usfm under example-corpora/");
        return;
    }
    paths.sort();

    // The oracle is only evidence if it actually ran, so the number of fixes
    // exercised is pinned like every other number here.
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

    // The two truncated footnotes and the one orphan `\+xt*` — every structural
    // finding in 226 books, each with a repair.
    assert_eq!(total(Code::UnclosedNote), 2);
    assert_eq!(total(Code::OrphanCloser), 1);
    // One `\p` per paragraph-less run, all 5434 of them.
    assert_eq!(total(Code::MissingParagraph), 5_434);
    // The FORMATTER half of `empty-paragraph`: 25 of the 787 empty paragraphs
    // are UNAMBIGUOUS duplication — the RUN of identical empties they open is
    // survived by a paragraph spelled exactly the same, holding the content
    // (`\p\n\p\n\v 39 Hem…`, en_ulb ISA's `\q\n\q` pairs). Every qualifying run
    // in this corpus happens to be one marker long, so the chain-aware
    // derivation leaves the number where the pairwise one had it; the other 762
    // are mixed (`\m` then `\p`, the `\s5` chunk idiom) or are runs whose
    // survivor is spelled differently (en_ulb ISA's `\p\n\p\n\q1`), where which
    // one was meant is the author's to say.
    assert_eq!(total(Code::EmptyParagraph), 25);
    // The corpus's ONE duplicate verse offers no fix: bdf_reg ROM 3 writes
    // `\v 10` twice and then `\v 11`, so renumbering the duplicate to 11 would
    // only move the duplicate one verse along. `renumber` declines — which is
    // why this reads 0 where the finding count above reads 1.
    assert_eq!(total(Code::VerseDuplicate), 0);
    // The corpus's one `verse-without-designator` is `\v +`, which holds
    // content: the empty-run deletion is offered nowhere in 226 books.
    assert_eq!(total(Code::VerseWithoutDesignator), 0);
    assert_eq!(totals.iter().sum::<u64>(), 5_462);
}

#[test]
fn exactly_one_corpus_book_has_no_id_line() {
    let Some(books) = lint_corpus() else { return };
    // BSB Ecclesiastes. `LintReport::book == None` is the STATE; the
    // `missing-id` observation is raised beside it (counted above).
    let missing: Vec<&PathBuf> = books
        .iter()
        .filter(|(_, _, book)| book.is_none())
        .map(|(path, _, _)| path)
        .collect();
    assert_eq!(missing.len(), 1, "books without \\id: {missing:?}");
    assert!(missing[0].to_string_lossy().contains("ECC"));
}
