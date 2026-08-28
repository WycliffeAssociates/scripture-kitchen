//! THE FOLD ORACLE (chunk-fold.md law 1): a book chopped at `\c ` chunk
//! starts, built and linted per chunk, fused where a boundary is dirty, and
//! folded back together must equal the fresh whole-book products —
//! `cst::concat(build_chunk each) == cst::build(lex(book))` node for node,
//! and `merge_reports(lint_chunk each, lint_carried) ≡ lint(book)` finding
//! for finding, offered repair for offered repair.
//!
//! The fast half runs the adversarial straddles (a sidebar across `\c`, an
//! unclosed `\f` across `\c`, runs ending at a seam, sequence anomalies
//! ACROSS the seam, the version carry) on snippets; the `#[ignore]`d half is
//! the same law over every corpus book. The corpus is committed under testData/, so that
//! half skips loudly when `testData/exampleCorpora/` is not mounted.

use std::path::{Path, PathBuf};

use usfm_onion::Token;
use usfm_onion::chunk::pre_scan;
use usfm_onion::cst::{self, Cst};
use usfm_onion::lex;
use usfm_onion::lint::{self, LintReport};

/// One fold unit: a chunk, or the fusion of a dirty-boundary run of chunks.
struct Unit {
    byte_base: u32,
    tokens: Vec<Token>,
    cst: Cst,
}

/// The fusion loop the galley pipeline runs, uncached: chop at the pre-scan
/// starts, build each chunk, and while a boundary reports open, widen the
/// unit over the next chunk and rebuild.
fn fold_units(text: &str) -> Vec<Unit> {
    let starts = pre_scan(text.as_bytes()).starts;
    let mut units = Vec::new();
    let mut at = 0usize;
    while at < starts.len() {
        let start = starts[at] as usize;
        let mut next = at + 1;
        loop {
            let end = starts.get(next).map_or(text.len(), |&s| s as usize);
            let tokens = lex(&text[start..end]);
            let chunk = cst::build_chunk(&tokens, end == text.len());
            if chunk.open_at_end && next < starts.len() {
                next += 1;
                continue;
            }
            units.push(Unit {
                byte_base: start as u32,
                tokens,
                cst: chunk.cst,
            });
            break;
        }
        at = next;
    }
    units
}

fn folded_cst(units: &[Unit]) -> Cst {
    let parts: Vec<&Cst> = units.iter().map(|unit| &unit.cst).collect();
    let counts: Vec<u32> = units.iter().map(|unit| unit.tokens.len() as u32).collect();
    cst::concat(&parts, &counts)
}

fn folded_report(text: &str, units: &[Unit]) -> LintReport {
    let mut locals = Vec::new();
    let mut summaries = Vec::new();
    let mut token_bases = Vec::new();
    let mut byte_bases = Vec::new();
    let mut token_base = 0u32;
    let mut ctx = lint::ChunkContext::default();
    for (index, unit) in units.iter().enumerate() {
        let end = units
            .get(index + 1)
            .map_or(text.len(), |next| next.byte_base as usize);
        let slice = &text.as_bytes()[unit.byte_base as usize..end];
        let (local, carried) = lint::lint_chunk(slice, &unit.tokens, &unit.cst, &ctx);
        if index == 0 {
            // Chunk 0's carry-outs become every later chunk's context.
            ctx = lint::ChunkContext {
                declared_version: carried.declared_version(),
                book_is_scripture: Some(carried.book_is_scripture()),
            };
        }
        token_bases.push(token_base);
        byte_bases.push(unit.byte_base);
        token_base += unit.tokens.len() as u32;
        locals.push(local);
        summaries.push(carried);
    }
    assert_eq!(
        lex(text).len() as u32,
        token_base,
        "chunk lex must tile the whole-book lex"
    );
    let (book, version) = locals
        .first()
        .map(|local| (local.book, local.declared_version))
        .unwrap_or_default();
    let seam = lint::reduce(
        &summaries.iter().collect::<Vec<_>>(),
        &token_bases,
        &byte_bases,
        book,
        version,
    );
    lint::merge_reports(
        &locals.iter().collect::<Vec<_>>(),
        &token_bases,
        &byte_bases,
        seam,
    )
}

/// Reports are equal where it matters: every consumer-visible fact. The fix
/// ARENA layout is linkage, so fixes compare through the accessors.
fn assert_reports_match(folded: &LintReport, fresh: &LintReport, label: &str) {
    assert_eq!(folded.book, fresh.book, "{label}: book");
    assert_eq!(
        folded.declared_version, fresh.declared_version,
        "{label}: declared_version"
    );
    assert_eq!(
        folded.observations, fresh.observations,
        "{label}: observations"
    );
    for index in 0..fresh.observations.len() {
        let folded_fix = folded.fix(index).map(|fix| (fix.label, folded.edits(fix)));
        let fresh_fix = fresh.fix(index).map(|fix| (fix.label, fresh.edits(fix)));
        assert_eq!(
            folded_fix, fresh_fix,
            "{label}: fix of observation {index} ({:?})",
            fresh.observations[index].code
        );
    }
}

fn assert_fold_oracle(text: &str, label: &str) {
    let units = fold_units(text);
    let tokens = lex(text);
    let fresh_cst = cst::build(&tokens);
    assert_eq!(folded_cst(&units), fresh_cst, "{label}: CST");
    let fresh = lint::lint(text.as_bytes(), &tokens, &fresh_cst);
    assert_reports_match(&folded_report(text, &units), &fresh, label);
}

#[test]
fn clean_books_fold_without_fusion() {
    for (label, text) in [
        ("empty", ""),
        ("front matter only", "\\id FRT\n\\mt1 Front\n\\p intro\n"),
        ("single chunk", "\\id GEN\nno chapters at all\n"),
        (
            "clean two chapters",
            "\\id GEN\n\\usfm 3.0\n\\h Genesis\n\\mt1 Genesis\n\\c 1\n\\p \\v 1 a \\v 2 b\n\\c 2\n\\q1 \\v 1 c\n\\q2 d\n",
        ),
        (
            "notes and characters inside chapters",
            "\\id GEN\n\\c 1\n\\p \\v 1 a\\f + \\fr 1:1 \\ft note\\f* \\add x\\add*\n\\c 2\n\\p \\v 1 \\w b|lemma=\"c\"\\w*\n",
        ),
        (
            "crlf endings",
            "\\id GEN\r\n\\c 1\r\n\\p \\v 1 a\r\n\\c 2\r\n\\p \\v 1 b\r\n",
        ),
        (
            "mid-line chapter is not a boundary",
            "\\id GEN\n\\c 1\n\\p \\v 1 a \\c 9 glued\n\\c 2\n\\p \\v 1 b\n",
        ),
    ] {
        let units = fold_units(text);
        assert!(
            units.len() == pre_scan(text.as_bytes()).starts.len(),
            "{label}: clean boundaries must not fuse"
        );
        assert_fold_oracle(text, label);
    }
}

#[test]
fn dirty_boundaries_fuse_and_converge() {
    for (label, text, expected_units) in [
        (
            "sidebar straddles a chapter",
            "\\id GEN\n\\c 1\n\\p \\v 1 a\n\\esb \\p in\n\\c 2\n\\p more\n\\esbe\n\\p \\v 1 b\n",
            2, // chunk 0 | fused(1,2)
        ),
        (
            // NOT a dirty boundary after all: the whole-book `\c` DISPLACES an
            // unclosed footnote (Recovery), and the boundary simulation runs
            // that same pop — so the chunks stay separate and still converge.
            // Dirty is only what `\c` cannot pop: barriers and mask-allowed
            // frames.
            "unclosed footnote runs past a chapter",
            "\\id GEN\n\\c 1\n\\p \\v 1 a\\f + \\ft note\n\\c 2\n\\p \\v 1 b\n",
            3,
        ),
        (
            "sidebar spans two seams",
            "\\id GEN\n\\c 1\n\\p \\v 1 a\n\\esb \\p in\n\\c 2\n\\p mid\n\\c 3\n\\p out\n\\esbe\n\\p \\v 1 b\n",
            2, // chunk 0 | fused(1,2,3)
        ),
    ] {
        let units = fold_units(text);
        assert_eq!(units.len(), expected_units, "{label}: fusion shape");
        assert_fold_oracle(text, label);
    }
}

#[test]
fn seam_runs_and_pendings_hold() {
    for (label, text) in [
        (
            "empty paragraph run ends at the seam",
            "\\id GEN\n\\c 1\n\\p a\n\\p\n\\p\n\\c 2\n\\p b\n",
        ),
        (
            "empty designator-less verses end at the seam",
            "\\id GEN\n\\c 1\n\\p \\v 1 a\n\\v \\v \n\\c 2\n\\p \\v 1 b\n",
        ),
        (
            "a closer is the last content before the seam",
            "\\id GEN\n\\c 1\n\\p \\v 1 a\\w*\n\\c 2\n\\p \\v 1 b\n",
        ),
        (
            "paragraph-less verse runs per chapter",
            "\\id GEN\n\\c 1\n\\v 1 a \\v 2 b\n\\c 2\n\\v 1 c\n",
        ),
        (
            "unclosed char at the very end of the book",
            "\\id GEN\n\\c 1\n\\p \\v 1 a\n\\c 2\n\\p \\v 1 \\add open\n",
        ),
    ] {
        assert_fold_oracle(text, label);
    }
}

#[test]
fn carried_facts_cross_the_seam() {
    for (label, text) in [
        (
            "chapter duplicate across the seam",
            "\\id GEN\n\\c 1\n\\p \\v 1 a\n\\c 1\n\\p \\v 1 b\n",
        ),
        (
            "chapter out of order across the seam",
            "\\id GEN\n\\c 2\n\\p \\v 1 a\n\\c 1\n\\p \\v 1 b\n",
        ),
        (
            "chapter gap across the seam",
            "\\id GEN\n\\c 1\n\\p \\v 1 a\n\\c 4\n\\p \\v 1 b\n",
        ),
        (
            "duplicate id in a later chunk",
            "\\id GEN\n\\c 1\n\\p \\v 1 a\n\\c 2\n\\id MAT\n\\p \\v 1 b\n",
        ),
        (
            "duplicate usfm in a later chunk",
            "\\id GEN\n\\usfm 3.0\n\\c 1\n\\p \\v 1 a\n\\c 2\n\\usfm 3.2\n\\p \\v 1 b\n",
        ),
        (
            "numbering mix across chunks",
            "\\id GEN\n\\c 1\n\\q a\n\\c 2\n\\q1 b\n",
        ),
        (
            "band violation in a later chunk",
            "\\id GEN\n\\c 1\n\\p \\v 1 a\n\\c 2\n\\p \\v 1 b\n\\mt1 late title\n",
        ),
        (
            "version declared in chunk 0 gates a later chunk's marker",
            "\\id GEN\n\\usfm 3.0\n\\c 1\n\\p \\v 1 \\pro x\\pro*\n\\c 2\n\\p \\v 1 \\pro y\\pro*\n",
        ),
        (
            "no version declared leaves later chunks ungated",
            "\\id GEN\n\\c 1\n\\p \\v 1 \\pro x\\pro*\n\\c 2\n\\p \\v 1 \\pro y\\pro*\n",
        ),
        (
            "verses before the first chapter",
            "\\id GEN\n\\p \\v 1 early\n\\c 1\n\\p \\v 1 a\n",
        ),
        (
            "paragraph before the first chapter",
            "\\id GEN\n\\p early\n\\c 1\n\\p \\v 1 a\n",
        ),
        (
            "verses and no chapter at all",
            "\\id GEN\n\\p \\v 1 a \\v 2 b\n",
        ),
        (
            "sid opened in one chunk owes its eid in another",
            "\\id GEN\n\\c 1\n\\p \\v 1 \\qt-s |sid=\"q1\"\\* said\n\\c 2\n\\p \\v 1 more \\qt-e\\*\n",
        ),
        (
            "missing id with markers present",
            "\\c 1\n\\p \\v 1 a\n\\c 2\n\\p \\v 1 b\n",
        ),
    ] {
        assert_fold_oracle(text, label);
    }
}

/// The corpus-scale law. `#[ignore]`: minutes-class, part of the pass-end
/// gate (`cargo test -- --include-ignored`), not the inner loop.
#[test]
#[ignore = "corpus-scale oracle; run --include-ignored at pass end"]
fn fold_oracle_over_every_corpus_book() {
    let mut paths: Vec<PathBuf> = Vec::new();
    fn collect(root: &Path, paths: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, paths);
            } else if path.extension().is_some_and(|ext| ext == "usfm") {
                paths.push(path);
            }
        }
    }
    collect(Path::new("../testData/exampleCorpora"), &mut paths);
    if paths.is_empty() {
        eprintln!("fold oracle SKIPPED: no *.usfm under testData/exampleCorpora/");
        return;
    }
    paths.sort();
    for path in &paths {
        let text = std::fs::read_to_string(path).expect("readable book");
        assert_fold_oracle(&text, &path.display().to_string());
    }
    println!("fold oracle held over {} books", paths.len());
}
