//! THE ORACLE for the single-pass experiment: the staged path is the
//! definition, and `analyze_fused` must reproduce it exactly.
//!
//! `analyze_fused(src) == (lex(src), build(&tokens), lint(src, &tokens, &cst))`
//! — tokens byte-identical, `Cst` equal, `LintReport` equal in every field
//! (observations, fix links, fixes, edits, book, declared version).
//!
//! Two populations: a snippet zoo of the shapes the unit tests exercise (which
//! runs everywhere), and every `*.usfm` under `example-corpora/` (gitignored —
//! skips loudly when absent), same corpus discipline as the other oracles.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use usfm_onion::cst::build;
use usfm_onion::experiments::fused::{analyze_fused, analyze_fused_cst, lex_noop_sink};
use usfm_onion::lex;
use usfm_onion::lint::lint;

fn assert_identical(label: &str, source: &str) {
    let tokens = lex(source);
    let cst = build(&tokens);
    let report = lint(source.as_bytes(), &tokens, &cst);

    let (fused_tokens, fused_cst, fused_report) = analyze_fused(source);

    assert_eq!(
        fused_tokens.len(),
        tokens.len(),
        "{label}: token count differs"
    );
    for (i, (f, s)) in fused_tokens.iter().zip(&tokens).enumerate() {
        assert_eq!(f, s, "{label}: token {i} differs");
    }
    assert_eq!(fused_cst.nodes, cst.nodes, "{label}: cst nodes differ");
    assert_eq!(
        fused_cst.child_ids, cst.child_ids,
        "{label}: cst child arena differs"
    );
    assert_eq!(fused_report.book, report.book, "{label}: book differs");
    assert_eq!(
        fused_report.declared_version, report.declared_version,
        "{label}: declared version differs"
    );
    assert_eq!(
        fused_report.observations, report.observations,
        "{label}: observations differ"
    );
    assert_eq!(
        fused_report.fix_of, report.fix_of,
        "{label}: fix links differ"
    );
    assert_eq!(fused_report.fixes, report.fixes, "{label}: fixes differ");
    assert_eq!(
        fused_report.edit_list, report.edit_list,
        "{label}: edits differ"
    );

    // The two measurement rungs share the arms, so they must produce the same
    // tokens (and, for the middle rung, the same tree) — otherwise the perf
    // comparison they exist for is measuring another pipeline.
    assert_eq!(lex_noop_sink(source), tokens, "{label}: noop sink differs");
    let (cst_tokens, cst_only) = analyze_fused_cst(source);
    assert_eq!(cst_tokens, tokens, "{label}: fused-cst tokens differ");
    assert_eq!(cst_only, cst, "{label}: fused-cst tree differs");
}

/// The shapes the cst and lint unit tests pin, plus the ones the fused pass
/// has its own reasons to fear: no `\c` at all (the header warm-up buffers the
/// whole document), a `\usfm` line, attribute lists, renumbering.
const ZOO: &[&str] = &[
    "",
    "hello",
    "\\p text here\n",
    "\\id GEN\n\\usfm 3.2\n\\c 1\n\\p\n\\v 1 In the beginning.\n",
    "\\id GEN\n\\c 1\n\\p\n\\v 1 one\n\\v 2 two\n\\v 3 three\n",
    // No chapter anywhere: the warm-up buffers the whole input.
    "\\id GEN Some description\n\\h Genesis\n\\toc1 The Book\n\\p front matter\n",
    // No `\id` either — the missing-id end check.
    "\\p just a paragraph\n",
    "plain prose with no markers at all\n",
    // Unclosed and orphan structure.
    "\\c 1\n\\p \\f + \\ft note\n\\v 1 text\n",
    "\\c 1\n\\p text \\w word\n",
    "\\c 1\n\\p text \\f*\n",
    "\\c 1\n\\p text \\*\n",
    "\\c 1\n\\p \\add one\\add* \\add two\n",
    // Milestones, containers, sidebars.
    "\\c 1\n\\p \\zaln-s |x-strong=\"G46130\"\\*\\w gracious|lemma=\"x\"\\w*\\zaln-e\\*\n",
    "\\c 1\n\\list-s\\*\n\\li1 one\n\\li1 two\n\\list-e\\*\n",
    "\\c 1\n\\table-s\\*\n\\tr \\tc1 a \\tc2 b\n\\tr \\tc1 c\n\\table-e\\*\n",
    "\\c 1\n\\esb\n\\p sidebar\n\\c 2\n\\esbe\n",
    "\\c 1\n\\ts\\*\n\\p text\n",
    // Attributes: front, trailing, mismatched terminator, two lists.
    "\\id GEN\n\\usfm 3.2\n\\c 1\n\\p \\w gracious|lemma=\"grace\"\\w*\n",
    "\\id GEN\n\\usfm 3.0\n\\c 1\n\\p \\w gracious|lemma=\"grace\"\\w*\n",
    "\\c 1\n\\p \\w a|lemma=\"x\"\\add*\n",
    "\\c 1\n\\p \\w |a=\"1\"|b=\"2\"\\w*\n",
    "\\c 1\n\\p \\fig |src=\"x.png\"\\fig*\n",
    // Ordering: duplicates, gaps, out of order, and the bdf_reg ROM 3 shape
    // that the renumber lookahead exists to refuse.
    "\\id GEN\n\\c 1\n\\p\n\\v 1 a\n\\v 1 b\n\\v 2 c\n",
    "\\id GEN\n\\c 1\n\\p\n\\v 10 a\n\\v 10 b\n\\v 11 c\n",
    "\\id GEN\n\\c 1\n\\p\n\\v 5 a\n\\v 2 b\n\\v 3 c\n",
    "\\id GEN\n\\c 1\n\\p\n\\v 1 a\n\\v 5 b\n",
    "\\id GEN\n\\c 1\n\\p\n\\v 1 a\n\\c 1\n\\p\n\\v 1 b\n",
    "\\id GEN\n\\c 1\n\\p\n\\v 2 a\n",
    "\\id GEN\n\\c\n\\p\n\\v 1 a\n",
    "\\id GEN\n\\v 1 a\n\\c 1\n\\p\n\\v 1 b\n",
    "\\id XYZ\n\\c 1\n\\p\n\\v 1 a\n",
    "\\id gen\n\\c 1\n\\p\n\\v 1 a\n",
    // Adjacency, levels, form.
    "\\id GEN\n\\c 1\n\\cp A\n\\p\n\\v 1 a\\va 1b\\va*\n",
    "\\id GEN\n\\c 1\n\\p\n\\cp A\n",
    "\\id GEN\n\\c 1\n\\q\n\\q1 a\n\\q2 b\n",
    "\\id GEN\n\\c 1\n\\p a\\p b\n",
    "\\id GEN\n\\c 1\n\\p\n\\v 1 a\n\\p\n",
    "\\id GEN\n\\c 1\n\\zznotarow x\n\\p a\n",
    "\\id GEN\n\\c 1\n\\p \\+nd a\\+nd*\n",
    "\\id GEN\n\\c 1\n\\p \\f +++++ \\ft x\\f*\n",
    // Escapes, optional breaks, CRLF, tabs, pipes in content.
    "\\c 1\n\\p a\\~b\\/c\\\\d\\|e\n",
    "\\c 1\n\\p a//b\n",
    "\\c 1\r\n\\p text\r\n\\v 1 one\r\n",
    "\\c 1\n\\p \t text \t more\n",
    "\\c 1\n\\p a|b|c\n",
    "\\c 1\n\\p \\w a|b|c\\w*\n",
    "\\c 1\n\\p \\u0041 and \\U0001F600\n",
    "\\v 1 text with no chapter\n",
    "\\c 1\n\\v\n1\n",
    "\\zaln-s |x=\"1\"\n",
];

#[test]
fn fused_matches_the_staged_pipeline_on_the_zoo() {
    for (i, source) in ZOO.iter().enumerate() {
        assert_identical(&format!("zoo[{i}]"), source);
    }
}

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

#[test]
fn fused_matches_the_staged_pipeline_on_the_corpus() {
    let mut paths = Vec::new();
    collect_usfm_paths(Path::new("example-corpora"), &mut paths);
    if paths.is_empty() {
        eprintln!("fused identity SKIPPED: no *.usfm under example-corpora/");
        return;
    }
    paths.sort();

    paths.par_iter().for_each(|path| {
        let source = std::fs::read_to_string(path).unwrap();
        assert_identical(&path.display().to_string(), &source);
    });

    eprintln!(
        "fused identity: {} books, tokens + cst + lint report identical",
        paths.len()
    );
}
