//! The `analyze` oracle: a DUMB re-derivation of every read, over real books.
//!
//! The reference emitter below rebuilds each of the seven reads from the same
//! artifacts (`lex`, `cst::build`, `lint`, `toc`, `mask`) in the most obvious
//! way possible, and converts every offset through the RANDOM-ACCESS
//! [`Utf16Index`] — one binary search per offset, no cursor, no sorting, no
//! sweep. `analyze` gets the same answers out of one streaming pass, so the
//! optimization the streaming pass IS gets checked against the definition on
//! every book we have.
//!
//! The structural laws ride along, because a reference emitter agreeing with a
//! wrong emitter would still be wrong: chapters tile the document, token spans
//! tile it, text runs are sorted and disjoint, every offset is inside the
//! document, no Form-channel row ever reaches the diagnostics read, and every
//! fix index resolves.
//!
//! Files: every `*.usfm` under `example-corpora/` plus
//! `testData/samples-from-wild/hindi-IRV1/` (dense Devanagari, where byte and
//! UTF-16 offsets drift on nearly every character). Both trees are gitignored;
//! the test skips loudly when neither is mounted. The sweep is ~7s over 226
//! books (rayon, debug), so it is always-on rather than `#[ignore]`d.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use usfm_onion_2::analyze::{Analysis, analyze, anchor, class_word, note_part, stride, wants};
use usfm_onion_2::cst::Cst;
use usfm_onion_2::lint::{LINT_ROWS, LintReport, NO_FIX, NO_TOKEN, Severity, lint};
use usfm_onion_2::mask::{Filter, mask};
use usfm_onion_2::tables::generated;
use usfm_onion_2::tables::schema::MarkerKind;
use usfm_onion_2::toc::{Toc, toc};
use usfm_onion_2::utf16::Utf16Index;
use usfm_onion_2::{Token, TokenKind, lex};

const NONE: u32 = u32::MAX;
/// `Node::token` for the root — the one node with no opening marker.
const ROOT_TOKEN: u32 = u32::MAX;

// ---------------------------------------------------------------------------
// The reference emitter — one binary search per offset, no cleverness
// ---------------------------------------------------------------------------

/// Everything `analyze` returns, re-derived the slow way.
struct Reference {
    chapters: Vec<u32>,
    blocks: Vec<u32>,
    lines: Vec<u32>,
    note_extents: Vec<u32>,
    note_parts: Vec<u32>,
    token_spans: Vec<u32>,
    text_runs: Vec<u32>,
    verse_anchors: Vec<u32>,
    diagnostics: Vec<u32>,
    fixes: Vec<u32>,
    fix_edits: Vec<u32>,
    fix_lens: Vec<u32>,
    fix_text: String,
}

fn reference(text: &str) -> Reference {
    let source = text.as_bytes();
    let tokens = lex(text);
    let cst = usfm_onion_2::cst::build(&tokens);
    let table = toc(source, &tokens);
    let report = lint(source, &tokens, &cst);
    let ix = Utf16Index::new(source);
    let u = |byte: u32| ix.to_utf16(byte);

    Reference {
        chapters: ref_chapters(source, &tokens, &table, &u),
        blocks: ref_blocks(source, &tokens, &cst, &u),
        lines: ref_lines(source, &tokens, &u),
        note_extents: ref_notes(&tokens, &cst, &u),
        note_parts: ref_note_parts(source, &tokens, &cst, &u),
        token_spans: ref_tokens(&tokens, &u),
        text_runs: ref_runs(source, &tokens, &cst, &u),
        verse_anchors: ref_verses(source, &tokens, &table, &u),
        diagnostics: ref_diagnostics(source, &tokens, &report, &u),
        fixes: ref_fixes(&report),
        fix_edits: ref_fix_edits(&report, &u),
        fix_lens: ref_fix_lens(&report),
        fix_text: ref_fix_text(&report),
    }
}

/// Where a token's content begins, re-derived by counting the trailing
/// horizontal-whitespace run the scanner folded on and keeping ONE byte of it.
/// Spelled out from the bytes rather than borrowing `analyze`'s helper: the
/// oracle must reach the same offset by its own route.
fn ref_content_from(source: &[u8], token: &Token) -> u32 {
    let span = &source[token.start as usize..token.end() as usize];
    let run = span
        .iter()
        .rev()
        .take_while(|b| **b == b' ' || **b == b'\t')
        .count();
    // A token that is nothing BUT the run keeps its bytes (the trim never
    // empties a span), so there is no delimiter to step over.
    if run == 0 || run == span.len() {
        token.end()
    } else {
        token.end() - run as u32 + 1
    }
}

/// The three offsets a designator slot has, re-derived by stepping the token
/// stream: the marker, an optional front attribute list, then the payload.
fn ref_slot(source: &[u8], tokens: &[Token], marker: usize) -> (u32, u32, u32) {
    match ref_designator(tokens, marker) {
        Some(token) => {
            let span = &source[token.start as usize..token.end() as usize];
            let label = span
                .iter()
                .rposition(|b| !b.is_ascii_whitespace())
                .map_or(0, |at| at + 1) as u32;
            (
                token.start,
                token.start + label,
                ref_content_from(source, &token),
            )
        }
        None => {
            let at = ref_content_from(source, &tokens[marker]);
            (at, at, at)
        }
    }
}

fn ref_designator(tokens: &[Token], marker: usize) -> Option<Token> {
    let mut next = marker + 1;
    if tokens.get(next).map(Token::kind) == Some(TokenKind::AttrList) {
        next += 1;
    }
    match tokens.get(next) {
        Some(token) if token.kind() == TokenKind::Designator => Some(*token),
        _ => None,
    }
}

/// The designator's bytes for a `\c`/`\v`, or `b""` when it has none.
fn ref_designator_bytes<'a>(source: &'a [u8], tokens: &[Token], marker: u32) -> &'a [u8] {
    match ref_designator(tokens, marker as usize) {
        Some(token) => &source[token.start as usize..token.end() as usize],
        None => b"",
    }
}

fn ref_chapters(source: &[u8], tokens: &[Token], table: &Toc, u: &dyn Fn(u32) -> u32) -> Vec<u32> {
    let mut out = Vec::new();
    for row in &table.chapters {
        let (marker_from, label_from, label_to, content) = match row.token {
            NONE => (NONE, u(row.start), u(row.start), NONE),
            token => {
                let (from, to, content) = ref_slot(source, tokens, token as usize);
                (u(tokens[token as usize].start), u(from), u(to), u(content))
            }
        };
        let shaped = row.token != NONE
            && usfm_onion_2::designator::chapter(ref_designator_bytes(source, tokens, row.token))
                .range()
                .is_some();
        out.extend_from_slice(&[
            u32::from(row.number) | if shaped { anchor::NUMBER_SHAPED } else { 0 },
            marker_from,
            label_from,
            label_to,
            content,
            u(row.start),
            u(row.end),
        ]);
    }
    out
}

fn ref_verses(source: &[u8], tokens: &[Token], table: &Toc, u: &dyn Fn(u32) -> u32) -> Vec<u32> {
    let mut out = Vec::new();
    for row in &table.verses {
        let (from, to, content) = ref_slot(source, tokens, row.token as usize);
        let shaped =
            usfm_onion_2::designator::verse(ref_designator_bytes(source, tokens, row.token))
                .range()
                .is_some();
        out.extend_from_slice(&[
            u32::from(row.chapter) | if shaped { anchor::NUMBER_SHAPED } else { 0 },
            u(row.at),
            u(from),
            u(to),
            u(content),
        ]);
    }
    out
}

/// One row per line whose first non-whitespace token is an opening marker.
fn ref_lines(source: &[u8], tokens: &[Token], u: &dyn Fn(u32) -> u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut from = 0u32;
    let mut looking = true;
    let mut open: Option<(u16, u32, u32)> = None;
    let close = |out: &mut Vec<u32>, open: &mut Option<(u16, u32, u32)>, to: u32| {
        if let Some((cls, start, content)) = open.take() {
            out.extend_from_slice(&[u32::from(cls), u(start), u(content.min(to)), u(to)]);
        }
    };
    for (row, token) in tokens.iter().enumerate() {
        if token.kind() == TokenKind::Newline {
            close(&mut out, &mut open, token.start);
            from = token.end();
            looking = true;
            continue;
        }
        if !looking {
            continue;
        }
        if token.kind() == TokenKind::Text
            && source[token.start as usize..token.end() as usize]
                .iter()
                .all(u8::is_ascii_whitespace)
        {
            continue;
        }
        looking = false;
        if matches!(token.kind(), TokenKind::Marker { .. }) {
            let (_, _, content) = ref_slot(source, tokens, row);
            open = Some((class_word(token), from, content));
        }
    }
    close(&mut out, &mut open, source.len() as u32);
    out
}

fn ref_blocks(source: &[u8], tokens: &[Token], cst: &Cst, u: &dyn Fn(u32) -> u32) -> Vec<u32> {
    let mut out = Vec::new();
    for node in 0..cst.nodes.len() {
        let row = &cst.nodes[node];
        if row.token == ROOT_TOKEN {
            continue;
        }
        let token = &tokens[row.token as usize];
        let block = matches!(
            generated::kind(token.marker_idx),
            MarkerKind::Paragraph
                | MarkerKind::Header
                | MarkerKind::Periph
                | MarkerKind::TableRow
                | MarkerKind::Sidebar
        );
        if !block {
            continue;
        }
        let extent = cst.extent(node as u32, tokens);
        out.extend_from_slice(&[
            u32::from(class_word(token)),
            u(extent.start),
            u(ref_content_from(source, token)),
            u(extent.end),
        ]);
    }
    out
}

fn ref_notes(tokens: &[Token], cst: &Cst, u: &dyn Fn(u32) -> u32) -> Vec<u32> {
    let mut out = Vec::new();
    for node in 0..cst.nodes.len() {
        let row = &cst.nodes[node];
        if row.token == ROOT_TOKEN {
            continue;
        }
        let token = &tokens[row.token as usize];
        if generated::kind(token.marker_idx) != MarkerKind::Note {
            continue;
        }
        // The family, spelled out — the production code resolves the same five
        // names through the table.
        let name = generated::name(token.marker_idx);
        let family = match name {
            "f" => 0,
            "fe" => 1,
            "ef" => 2,
            "x" => 3,
            "ex" => 4,
            _ => 5,
        };
        let extent = cst.extent(node as u32, tokens);
        out.extend_from_slice(&[family, u(extent.start), u(extent.end)]);
    }
    out
}

/// Every note's interior, re-derived by taking the tokens inside the extent BY
/// SPAN — a second way to reach the set the tree walk reaches.
fn ref_note_parts(source: &[u8], tokens: &[Token], cst: &Cst, u: &dyn Fn(u32) -> u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut index = 0u32;
    for node in 0..cst.nodes.len() {
        let row = &cst.nodes[node];
        if row.token == ROOT_TOKEN
            || generated::kind(tokens[row.token as usize].marker_idx) != MarkerKind::Note
        {
            continue;
        }
        let extent = cst.extent(node as u32, tokens);
        let mut origin = false;
        let mut run: Option<(u32, u32, u32)> = None;
        for (at, token) in tokens.iter().enumerate() {
            if token.start < extent.start || token.end() > extent.end {
                continue;
            }
            let name = generated::name(token.marker_idx);
            match token.kind() {
                // An origin runs from its own marker to the next marker of any
                // kind — `\fr 1:5 \ft body` is the shape in every corpus book.
                TokenKind::Marker { .. } => origin = name == "fr" || name == "xo",
                TokenKind::ClosingMarker { .. } => origin = false,
                _ => {}
            }
            if at as u32 == row.token {
                continue;
            }
            let kind = match token.kind() {
                TokenKind::NoteCaller => note_part::CALLER,
                TokenKind::Text | TokenKind::Newline | TokenKind::OptBreak => {
                    if origin {
                        note_part::ORIGIN
                    } else {
                        note_part::BODY
                    }
                }
                _ => note_part::MARKUP,
            };
            let span = &source[token.start as usize..token.end() as usize];
            // Chrome keeps its label plus one delimiter byte; the rest of the
            // folded run is the text that follows it.
            let after = ref_content_from(source, token);
            let to = match kind {
                note_part::CALLER => {
                    let label = span
                        .iter()
                        .rposition(|b| !b.is_ascii_whitespace())
                        .map_or(span.len(), |at| at + 1);
                    token.start + label as u32
                }
                note_part::ORIGIN | note_part::BODY => token.end(),
                _ => after,
            };
            if token.start >= to {
                continue;
            }
            match kind {
                note_part::ORIGIN | note_part::BODY => match &mut run {
                    Some((open, _, end)) if *open == kind && *end == token.start => *end = to,
                    _ => {
                        if let Some((open, start, end)) = run.replace((kind, token.start, to)) {
                            out.extend_from_slice(&[index, open, u(start), u(end)]);
                        }
                    }
                },
                _ => {
                    if let Some((open, start, end)) = run.take() {
                        out.extend_from_slice(&[index, open, u(start), u(end)]);
                    }
                    out.extend_from_slice(&[index, kind, u(token.start), u(to)]);
                    if after < token.end() {
                        let kind = if origin {
                            note_part::ORIGIN
                        } else {
                            note_part::BODY
                        };
                        run = Some((kind, after, token.end()));
                    }
                }
            }
        }
        if let Some((open, start, end)) = run {
            out.extend_from_slice(&[index, open, u(start), u(end)]);
        }
        index += 1;
    }
    out
}

fn ref_tokens(tokens: &[Token], u: &dyn Fn(u32) -> u32) -> Vec<u32> {
    let mut out = Vec::with_capacity(tokens.len() * stride::TOKEN_SPANS);
    for token in tokens {
        let packed = u32::from(class_word(token)) | u32::from(token.kind_bits) << 16;
        out.extend_from_slice(&[packed, u(token.start), u(token.end())]);
    }
    out
}

fn ref_runs(source: &[u8], tokens: &[Token], cst: &Cst, u: &dyn Fn(u32) -> u32) -> Vec<u32> {
    let mut out = Vec::new();
    for run in &mask(source, tokens, cst, &Filter::reader_text()).ranges {
        out.extend_from_slice(&[u(run.start), u(run.end)]);
    }
    out
}

fn ref_diagnostics(
    source: &[u8],
    tokens: &[Token],
    report: &LintReport,
    u: &dyn Fn(u32) -> u32,
) -> Vec<u32> {
    // The anchor is trimmed of the delimiter the scanner folded onto it.
    let span = |token: u32| match tokens.get(token as usize) {
        Some(token) => {
            let bytes = &source[token.start as usize..token.end() as usize];
            let label = bytes
                .iter()
                .rposition(|b| !b.is_ascii_whitespace())
                .map_or(bytes.len(), |at| at + 1);
            (u(token.start), u(token.start + label as u32))
        }
        None => {
            let end = u(source.len() as u32);
            (end, end)
        }
    };
    let mut out = Vec::new();
    for (index, obs) in report.observations.iter().enumerate() {
        let (from, to) = span(obs.anchor);
        let (second_from, second_to) = if obs.second == NO_TOKEN {
            (NONE, NONE)
        } else {
            span(obs.second)
        };
        let fix = report.fix_of[index];
        out.extend_from_slice(&[
            obs.code as u32,
            from,
            to,
            second_from,
            second_to,
            obs.aux,
            if fix == NO_FIX { NONE } else { fix },
        ]);
    }
    out
}

fn ref_fixes(report: &LintReport) -> Vec<u32> {
    let mut out = Vec::new();
    let mut edits = 0u32;
    for fix in &report.fixes {
        let count = report.edits(fix).len() as u32;
        out.extend_from_slice(&[edits, edits + count]);
        edits += count;
    }
    out
}

fn ref_fix_edits(report: &LintReport, u: &dyn Fn(u32) -> u32) -> Vec<u32> {
    let mut out = Vec::new();
    for fix in &report.fixes {
        for edit in report.edits(fix) {
            out.extend_from_slice(&[u(edit.from), u(edit.to)]);
        }
    }
    out
}

fn ref_fix_lens(report: &LintReport) -> Vec<u32> {
    let mut out = Vec::new();
    for fix in &report.fixes {
        for edit in report.edits(fix) {
            out.push(edit.insert.as_bytes().len() as u32);
        }
    }
    out
}

fn ref_fix_text(report: &LintReport) -> String {
    let mut out = String::new();
    for fix in &report.fixes {
        for edit in report.edits(fix) {
            out.push_str(edit.insert.as_str());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The laws every book's analysis obeys
// ---------------------------------------------------------------------------

fn laws(where_: &str, a: &Analysis) {
    let end = a.len_utf16;
    let spans = |read: &[u32], stride: usize, fields: &[usize], name: &str| {
        for row in read.chunks_exact(stride) {
            for &field in fields {
                assert!(
                    row[field] == NONE || row[field] <= end,
                    "{where_}: {name} offset {} is past the document ({end})",
                    row[field]
                );
            }
        }
    };
    spans(
        &a.chapters,
        stride::CHAPTERS,
        &[1, 2, 3, 4, 5, 6],
        "chapters",
    );
    spans(&a.blocks, stride::BLOCKS, &[1, 2, 3], "blocks");
    spans(&a.lines, stride::LINES, &[1, 2, 3], "lines");
    spans(&a.note_extents, stride::NOTE_EXTENTS, &[1, 2], "notes");
    spans(&a.note_parts, stride::NOTE_PARTS, &[2, 3], "note parts");
    spans(&a.token_spans, stride::TOKEN_SPANS, &[1, 2], "tokens");
    spans(&a.text_runs, stride::TEXT_RUNS, &[0, 1], "runs");
    spans(
        &a.verse_anchors,
        stride::VERSE_ANCHORS,
        &[1, 2, 3, 4],
        "verses",
    );
    spans(
        &a.diagnostics,
        stride::DIAGNOSTICS,
        &[1, 2, 3, 4],
        "diagnostics",
    );
    spans(&a.fix_edits, stride::FIX_EDITS, &[0, 1], "fix edits");

    // Chapters tile the whole document, front matter first.
    let mut at = 0;
    for row in a.chapters.chunks_exact(stride::CHAPTERS) {
        assert_eq!(row[5], at, "{where_}: chapter rows do not tile");
        assert!(
            row[2] >= row[5] && row[3] <= row[6],
            "{where_}: label inside"
        );
        // The chrome runs marker -> label -> content, and only the synthetic
        // front-matter row is allowed to have no marker at all.
        assert_eq!(
            row[1] == NONE,
            row[4] == NONE,
            "{where_}: half a chapter marker"
        );
        if row[1] != NONE {
            assert!(
                row[1] <= row[2] && row[3] <= row[4],
                "{where_}: chapter chrome is out of order"
            );
        }
        at = row[6];
    }
    assert_eq!(at, end, "{where_}: the last chapter must reach the end");

    // A verse anchor's four offsets ascend: marker, number, content.
    for row in a.verse_anchors.chunks_exact(stride::VERSE_ANCHORS) {
        assert!(
            row[1] <= row[2] && row[2] <= row[3] && row[3] <= row[4],
            "{where_}: verse chrome is out of order"
        );
    }

    // Marked lines are sorted, disjoint, and their chrome is inside them.
    let mut previous = 0;
    for row in a.lines.chunks_exact(stride::LINES) {
        assert!(row[1] >= previous, "{where_}: lines are not sorted");
        assert!(
            row[1] <= row[2] && row[2] <= row[3],
            "{where_}: a line's chrome is not inside it"
        );
        previous = row[3];
    }

    // Note parts stay inside the extent they name, in order.
    for row in a.note_parts.chunks_exact(stride::NOTE_PARTS) {
        let note = row[0] as usize * stride::NOTE_EXTENTS;
        assert!(
            note + stride::NOTE_EXTENTS <= a.note_extents.len(),
            "{where_}: a note part names no note"
        );
        assert!(
            row[2] >= a.note_extents[note + 1]
                && row[3] <= a.note_extents[note + 2]
                && row[2] < row[3],
            "{where_}: a note part escapes its extent"
        );
    }

    // Token spans tile it too — the scanner is lossless, so every byte is in
    // exactly one token.
    let mut at = 0;
    for row in a.token_spans.chunks_exact(stride::TOKEN_SPANS) {
        assert_eq!(row[1], at, "{where_}: token spans do not tile");
        at = row[2];
    }
    assert!(
        a.token_spans.is_empty() || at == end,
        "{where_}: token spans stop at {at}, not {end}"
    );

    // Text runs are sorted, disjoint and non-empty (the mask's own contract,
    // preserved across the conversion).
    let mut previous = 0;
    for row in a.text_runs.chunks_exact(stride::TEXT_RUNS) {
        assert!(
            row[0] >= previous && row[1] > row[0],
            "{where_}: text runs are not sorted-disjoint at {}",
            row[0]
        );
        previous = row[1];
    }

    for row in a.blocks.chunks_exact(stride::BLOCKS) {
        assert!(
            row[1] <= row[2] && row[2] <= row[3],
            "{where_}: a block's chrome is not inside it"
        );
    }

    for row in a.diagnostics.chunks_exact(stride::DIAGNOSTICS) {
        let lint_row = &LINT_ROWS[row[0] as usize];
        assert_ne!(
            lint_row.severity,
            Some(Severity::Form),
            "{where_}: {} is a Form row and must never be a diagnostic",
            lint_row.name
        );
        assert!(row[1] <= row[2], "{where_}: {} span", lint_row.name);
        assert!(
            (row[3] == NONE) == (row[4] == NONE),
            "{where_}: half a second span"
        );
        if row[6] != NONE {
            let fix = row[6] as usize * stride::FIXES;
            assert!(
                fix + stride::FIXES <= a.fixes.len(),
                "{where_}: fix index {} is out of range",
                row[6]
            );
            let (from, to) = (a.fixes[fix], a.fixes[fix + 1]);
            assert!(
                from <= to && to as usize * stride::FIX_EDITS <= a.fix_edits.len(),
                "{where_}: fix edit range is out of range"
            );
        }
    }

    assert_eq!(
        a.fix_lens.len() * stride::FIX_EDITS,
        a.fix_edits.len(),
        "{where_}: one length per fix edit"
    );
    assert_eq!(
        a.fix_lens.iter().sum::<u32>() as usize,
        a.fix_text.len(),
        "{where_}: the lengths must partition the fix blob"
    );
}

/// One book: the reference emitter, the laws, and the clip.
fn check_file(path: &Path) -> usize {
    let text = std::fs::read_to_string(path).unwrap();
    let where_ = path.display().to_string();
    let a = analyze(&text, wants::ALL, None);
    let r = reference(&text);

    assert_eq!(a.chapters, r.chapters, "{where_}: chapters");
    assert_eq!(a.blocks, r.blocks, "{where_}: blocks");
    assert_eq!(a.lines, r.lines, "{where_}: lines");
    assert_eq!(a.note_parts, r.note_parts, "{where_}: note parts");
    assert_eq!(a.note_extents, r.note_extents, "{where_}: note extents");
    assert_eq!(a.token_spans, r.token_spans, "{where_}: token spans");
    assert_eq!(a.text_runs, r.text_runs, "{where_}: text runs");
    assert_eq!(a.verse_anchors, r.verse_anchors, "{where_}: verse anchors");
    assert_eq!(a.diagnostics, r.diagnostics, "{where_}: diagnostics");
    assert_eq!(a.fixes, r.fixes, "{where_}: fixes");
    assert_eq!(a.fix_edits, r.fix_edits, "{where_}: fix edits");
    assert_eq!(a.fix_lens, r.fix_lens, "{where_}: fix lengths");
    assert_eq!(a.fix_text, r.fix_text, "{where_}: fix text");
    assert_eq!(
        a.len_utf16,
        Utf16Index::new(text.as_bytes()).len_utf16(),
        "{where_}: length"
    );
    laws(&where_, &a);

    // A clip never changes an offset — it only drops rows. Take the middle
    // third of the document and check the survivors against the whole-book
    // rows verbatim.
    let clip = a.len_utf16 / 3..a.len_utf16 * 2 / 3;
    let clipped = analyze(
        &text,
        wants::TOKEN_SPANS | wants::TEXT_RUNS,
        Some(clip.clone()),
    );
    let kept: Vec<&[u32]> = a
        .token_spans
        .chunks_exact(stride::TOKEN_SPANS)
        .filter(|row| row[2] >= clip.start && row[1] <= clip.end)
        .collect();
    assert_eq!(
        clipped
            .token_spans
            .chunks_exact(stride::TOKEN_SPANS)
            .collect::<Vec<_>>(),
        kept,
        "{where_}: a clip must be a subsequence of the whole-book read"
    );
    assert!(
        clipped.chapters.is_empty() && clipped.diagnostics.is_empty(),
        "{where_}: an unwanted read must stay empty under a clip"
    );

    a.token_spans.len() / stride::TOKEN_SPANS
}

// ---------------------------------------------------------------------------
// The corpus
// ---------------------------------------------------------------------------

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

fn corpus() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    collect_usfm_paths(Path::new("example-corpora"), &mut paths);
    collect_usfm_paths(
        Path::new("testData/samples-from-wild/hindi-IRV1"),
        &mut paths,
    );
    collect_usfm_paths(
        Path::new("testData/samples-from-wild/hindi-IRV2"),
        &mut paths,
    );
    paths.sort();
    paths
}

/// One offset, hand-checked on Devanagari: the UTF-16 offset of a verse
/// designator is NOT its byte offset, and it is what the random-access index
/// says.
#[test]
fn a_hindi_verse_anchor_is_utf16_not_bytes() {
    let path = Path::new("testData/samples-from-wild/hindi-IRV1/origin.usfm");
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!(
            "hindi spot check SKIPPED: {} is not mounted",
            path.display()
        );
        return;
    };
    let a = analyze(&text, wants::VERSE_ANCHORS | wants::CHAPTERS, None);
    let ix = Utf16Index::new(text.as_bytes());
    let source = lex(&text);
    let table = toc(text.as_bytes(), &source);

    // The LAST verse of the book: the furthest into the drift, so a wrong
    // conversion cannot hide.
    let last = table.verses.last().expect("hindi has verses");
    let (at, len) = last
        .designator_span(text.as_bytes(), &source)
        .expect("its designator");
    let row = a
        .verse_anchors
        .chunks_exact(stride::VERSE_ANCHORS)
        .last()
        .unwrap();
    assert_eq!(row[2], ix.to_utf16(at), "the designator's start");
    assert_eq!(row[3], ix.to_utf16(at + u32::from(len)), "its end");
    assert_eq!(row[1], ix.to_utf16(last.at), "the marker's start");
    assert!(
        row[2] < at,
        "3-byte script: the UTF-16 offset ({}) must be well under the byte offset ({at})",
        row[2]
    );
    // …and it round-trips back to the same bytes the editor would slice.
    assert_eq!(
        &text[ix.to_byte(row[2]) as usize..ix.to_byte(row[3]) as usize],
        &text[at as usize..(at + u32::from(len)) as usize]
    );
    println!(
        "hindi: last verse designator at byte {at} is UTF-16 {} ({} units total)",
        row[2], a.len_utf16
    );
}

/// Every mounted book, all seven reads, against the reference emitter. Kept
/// always-on rather than `#[ignore]`d: the sweep is ~7s over 226 books, which
/// is not the minutes the ignore convention is for.
#[test]
fn every_corpus_book_agrees_with_the_reference_emitter() {
    let paths = corpus();
    if paths.is_empty() {
        eprintln!("analyze oracle SKIPPED: no *.usfm under example-corpora/ or testData/");
        return;
    }
    let tokens: usize = paths.par_iter().map(|path| check_file(path)).sum();
    println!(
        "analyze oracle: {} files, {tokens} token spans, all seven reads",
        paths.len()
    );
}
