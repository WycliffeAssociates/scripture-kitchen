//! The dish's RANGE questions, as an oracle for the TypeScript reader.
//!
//! ```text
//! \v 1 Jesus \add wept\add*.
//!            ^^^^^^^^^^^^^^ enclosing(the "wept" span) — the `\add` node
//! ```
//!
//! The reader answers "what covers this range" and "what is this range made
//! of" from `nodes`, `childIds` and the token rows alone. Rust already knows,
//! through `Cst::extent` and `Cst::owners`; this writes that answer to
//! `testData/goldens/dish-queries/`, and
//! `onion-wasm/tests/conformance.mjs` asserts the reader reproduces every
//! entry. Neither half can drift without the other failing.
//!
//! Instrument: SHAPES — one synthetic document carrying notes, milestones,
//! attributes and nesting, plus the committee's own weird shapes, plus one
//! real book for volume. Absent bytes are a loud failure, never a silent skip.
//!
//! On a legitimate change: `UPDATE_GOLDENS=1 cargo test -p usfm_onion --test
//! dish_queries`, then read the diff.

use std::path::{Path, PathBuf};

use usfm_onion::cst::{self, Cst};
use usfm_onion::lex;
use usfm_onion::{Token, TokenKind};

/// Every shape the helpers have to get right, in one small document: a note
/// whose body is not verse text, a nested character marker, a milestone pair,
/// an attribute list, a table row, and a heading between two verses.
const SHAPES: &str = "\\id GEN Genesis\n\
\\usfm 3.0\n\
\\h Genesis\n\
\\c 1\n\
\\s The beginning\n\
\\p\n\
\\v 1 In the beginning\\f + \\ft a \\+nd note\\+nd* here\\f* God created.\n\
\\q1 \\v 2-3 And the \\w earth|lemma=\"earth\"\\w* was \\add without\\add* form.\n\
\\qt-s |who=\"Narrator\"\\*spoken\\qt-e\\*\n\
\\tr \\tc1 one \\tc2 two\n\
\\c 2\n\
\\p \\v 1 Thus the heavens were finished.\n";

/// One golden entry: the range asked about, and what the engine answers.
struct Entry {
    from: u32,
    to: u32,
    node_from: u32,
    node_to: u32,
    marker: u8,
    in_markup: bool,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("onion sits in the workspace")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// The token whose `[start, end)` contains `offset`. Tokens partition the
/// source, so the last start at or before `offset` is it.
fn token_at(tokens: &[Token], offset: u32) -> usize {
    tokens.partition_point(|token| token.start <= offset) - 1
}

/// The smallest node whose extent covers `[from, to)` — the walk the reader
/// does, from the owner of `from`'s token up while the extent falls short.
fn enclosing(
    cst: &Cst,
    tokens: &[Token],
    owners: &[u32],
    parents: &[u32],
    from: u32,
    to: u32,
) -> u32 {
    let end = to.max(from + 1);
    let mut node = owners[token_at(tokens, from)];
    loop {
        let span = cst.extent(node, tokens);
        if span.start <= from && end <= span.end {
            return node;
        }
        let up = parents[node as usize];
        if up == u32::MAX {
            return node;
        }
        node = up;
    }
}

/// No `Text` token bytes inside the range.
fn in_markup(tokens: &[Token], from: u32, to: u32) -> bool {
    if to <= from {
        return true;
    }
    tokens
        .iter()
        .filter(|token| token.start < to && token.end() > from)
        .all(|token| token.kind() != TokenKind::Text)
}

/// Every token's span, every verse-ish anchor's span, and a spread of interior
/// offsets — the three shapes a consumer asks in.
fn ranges(tokens: &[Token], len: u32) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    // At most ~150 token spans, evenly spread, so a 100 KB book and a 500-byte
    // fixture both cost a few hundred entries.
    let stride = (tokens.len() / 150).max(1);
    for token in tokens.iter().step_by(stride) {
        out.push((token.start, token.end()));
    }
    // A range that starts inside one token and ends inside a later one is the
    // case a find hit or a diff run actually produces.
    for pair in tokens.chunks(stride * 3) {
        let (first, last) = (pair[0], pair[pair.len() - 1]);
        let from = first.start + u32::from(first.len) / 2;
        let to = (last.start + u32::from(last.len) / 2).max(from);
        out.push((from, to));
    }
    // And the degenerate one: an empty range at a token boundary.
    for token in tokens.iter().step_by(stride * 5) {
        out.push((token.start, token.start));
    }
    out.retain(|(from, to)| *from < len && *to <= len);
    out.sort_unstable();
    out.dedup();
    out
}

fn entries(source: &str) -> Vec<Entry> {
    let tokens = lex(source);
    assert!(
        !tokens.is_empty(),
        "a golden document must lex to something"
    );
    let cst = cst::build(&tokens);
    let owners = cst.owners(tokens.len());
    let parents = cst.parents();
    ranges(&tokens, source.len() as u32)
        .into_iter()
        .map(|(from, to)| {
            let node = enclosing(&cst, &tokens, &owners, &parents, from, to);
            let span = cst.extent(node, &tokens);
            let opener = cst.nodes[node as usize].token;
            Entry {
                from,
                to,
                node_from: span.start,
                node_to: span.end,
                marker: tokens
                    .get(opener as usize)
                    .map_or(0, |token| token.marker_idx),
                in_markup: in_markup(&tokens, from, to),
            }
        })
        .collect()
}

/// The golden's text, one entry a line so a diff reads.
fn golden(source_path: &str, entries: &[Entry]) -> String {
    let mut out = format!("{{\n  \"source\": \"{source_path}\",\n  \"entries\": [\n");
    for (n, e) in entries.iter().enumerate() {
        let comma = if n + 1 == entries.len() { "" } else { "," };
        out.push_str(&format!(
            "    {{ \"from\": {}, \"to\": {}, \"enclosing\": {{ \"from\": {}, \"to\": {}, \"marker\": {} }}, \"inMarkup\": {} }}{comma}\n",
            e.from, e.to, e.node_from, e.node_to, e.marker, e.in_markup
        ));
    }
    out.push_str("  ]\n}\n");
    out
}

/// The documents the goldens cover: the golden's name, and the source file
/// under the workspace root. `None` is the synthetic one, written beside it.
const DOCUMENTS: &[(&str, Option<&str>)] = &[
    ("shapes", None),
    (
        "usfmtc-milestones",
        Some("testData/usfmtc/advanced/milestones/origin.usfm"),
    ),
    (
        "usfmtc-nesting",
        Some("testData/usfmtc/advanced/nesting/origin.usfm"),
    ),
    (
        "usfmtc-footnotes",
        Some("testData/usfmtc/advanced/footnote-structures/origin.usfm"),
    ),
    (
        "en_ulb-RUT",
        Some("testData/exampleCorpora/en_ulb/08-RUT.usfm"),
    ),
];

#[test]
fn the_dish_query_goldens_are_fresh() {
    let root = root();
    let dir = root.join("testData/goldens/dish-queries");
    let update = std::env::var_os("UPDATE_GOLDENS").is_some();
    if update {
        std::fs::create_dir_all(&dir).expect("the goldens directory is writable");
    }
    for (name, path) in DOCUMENTS {
        let source = match path {
            // The synthetic document lives in this file; the golden names the
            // fixture the reader has to be handed, and conformance.mjs holds
            // the same bytes under the same name.
            None => SHAPES.to_string(),
            Some(path) => read(&root.join(path)).replace("\r\n", "\n"),
        };
        let fresh = golden(path.unwrap_or("(inline: SHAPES)"), &entries(&source));
        let at = dir.join(format!("{name}.json"));
        if update {
            std::fs::write(&at, &fresh).expect("the golden is writable");
            continue;
        }
        let checked_in = read(&at);
        assert!(
            fresh == checked_in,
            "{}",
            ticket::stale::first_difference(
                &at.display().to_string(),
                "UPDATE_GOLDENS=1 cargo test -p usfm_onion --test dish_queries",
                &fresh,
                &checked_in,
            )
        );
    }
}

/// The synthetic document the goldens are cut from, so the JS half is reading
/// the same bytes rather than a copy that drifted.
#[test]
fn the_shapes_fixture_is_published_for_the_js_half() {
    let at = root().join("testData/goldens/dish-queries/shapes.usfm");
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(at.parent().expect("a parent")).expect("writable");
        std::fs::write(&at, SHAPES).expect("writable");
        return;
    }
    assert_eq!(read(&at), SHAPES, "{} is stale", at.display());
}
