//! THE HTML SMOKE + THE TEXT-IDENTITY INVARIANT, over every
//! `<validated>pass</validated>` case in `testData/` and every book in
//! `example-corpora/`.
//!
//! ```text
//! origin.usfm --lex--> --cst::build--> --html()--> strip tags  ==  strings of usj()
//! ```
//!
//! HTML has NO ORACLE (sketches/html-export.md's scope ruling: there is no
//! reference implementation, and multiple readings are correct), so this file
//! pins the two things that can be checked WITHOUT one:
//!
//! 1. **SMOKE** — every document renders without panicking, every `<` in the
//!    output opens a tag we wrote (i.e. no content `<` escaped the escaper),
//!    and the tags NEST (a mini-parser with a stack, which also catches a
//!    stray or missing close).
//! 2. **THE TEXT-IDENTITY INVARIANT (RULED 2026-08-21)** — strip the tags and
//!    the remaining text must equal the string content of our own USJ output
//!    for the same input, after whitespace canonicalization. No reference HTML
//!    is needed, and it catches dropped or duplicated content mechanically.
//!    What stays unpinned is only the aesthetic layer, which is legitimately
//!    ours.
//!
//! # The carve-outs, and why each one is exact rather than fudged
//!
//! USJ LIFTS several things out of content and into attributes; the HTML fold
//! splats them into `data-*` too, but ALSO renders them, because a view whose
//! chapter numbers and footnote callers are invisible is not a view. Every such
//! element carries `class="usfm-lifted"`, and this test skips those subtrees —
//! ONE mechanism, declared in `src/html.rs` as the `usfm-lifted` contract, not
//! a per-case normalizer:
//!
//! | rendered text | USJ's home for it | why HTML renders it anyway |
//! |---|---|---|
//! | `\c`/`\v` number | `number` on the chapter/verse element | a reader needs to see the number |
//! | a note's caller (`+` auto or a literal) | `caller` on the note | the caller IS the note's handle in the text |
//! | `\cat`'s text | `category` on the note/sidebar | the audit ruled `\cat` publishable char-shaped content |
//! | `\usfm`'s version | dropped outright | the audit ruled it addressable (`Span`), so it renders + hides via CSS |
//! | `\periph`'s title | `alt` on the periph | a `<section>` with an invisible title is not a view |
//! | `\rb`'s gloss | `gloss` attribute | `<ruby>` without `<rt>` is pointless |
//!
//! The AUTO note-caller number is the one piece of text in the output that no
//! token supplied; it additionally carries `note-caller-generated`, so a
//! consumer (and this test) can tell generated from merely relocated.
//!
//! Everything else is expected to be byte-for-byte the same text, and a
//! divergence is a real dropped/duplicated-content bug — the failure prints a
//! window around the first differing character so it can be told at the bytes.

#![cfg(all(feature = "html", feature = "usj"))]

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde_json::Value;

use usfm_onion_2::{cst, html::html, lex, usj::usj};

/// The class that means "this element's text is not USJ string content" — the
/// contract `src/html.rs` documents. Anything wearing it is skipped whole.
const LIFTED: &str = "usfm-lifted";

/// Elements this fold writes with no closing tag.
const VOID: &[&str] = &["br", "img"];

#[test]
fn html_renders_and_keeps_every_character_of_text() {
    let mut cases: Vec<PathBuf> = Vec::new();
    collect_test_data(Path::new("testData"), &mut cases);
    collect_corpora(Path::new("example-corpora"), &mut cases);
    cases.sort();
    if cases.is_empty() {
        eprintln!("html corpus SKIPPED: no testData/ and no example-corpora/");
        return;
    }

    let results: Vec<Result<(), String>> = cases.par_iter().map(|case| check(case)).collect();
    let failures: Vec<&String> = results.iter().filter_map(|r| r.as_ref().err()).collect();
    eprintln!(
        "html corpus: {}/{} documents render and hold their text",
        results.len() - failures.len(),
        results.len()
    );
    if !failures.is_empty() {
        let mut report = String::new();
        for story in failures.iter().take(10) {
            report.push_str(story);
            report.push('\n');
        }
        panic!(
            "{}/{} documents diverge:\n{report}",
            failures.len(),
            results.len()
        );
    }
}

fn check(case: &Path) -> Result<(), String> {
    let bytes = std::fs::read(case).expect("readable source");
    let Ok(source) = std::str::from_utf8(&bytes) else {
        return Ok(()); // not our test: the lexer's contract is UTF-8 in
    };
    let tokens = lex(source);
    let cst = cst::build(&tokens);

    let ours = html(source.as_bytes(), &tokens, &cst);
    let text = strip(&ours).map_err(|why| format!("{}: {why}", case.display()))?;

    let json: Value = serde_json::from_str(&usj(source.as_bytes(), &tokens, &cst))
        .map_err(|error| format!("{}: OUR usj is not JSON: {error}", case.display()))?;
    let mut expected = String::new();
    strings(&json, &mut expected);

    let ours = canonical(&text);
    let theirs = canonical(&expected);
    if ours == theirs {
        return Ok(());
    }
    Err(format!(
        "{}: {}",
        case.display(),
        divergence(&ours, &theirs)
    ))
}

/// Strips the tags, skipping every subtree marked [`LIFTED`] (`usfm-lifted`), and validates on
/// the way through: every `<` must open a tag, and every close must match.
fn strip(html: &str) -> Result<String, String> {
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len() / 2);
    let mut stack: Vec<&str> = Vec::new();
    // The stack depth at which the current LIFTED subtree began, if any.
    let mut skipping: Option<usize> = None;
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'<' => {
                let end = html[at..]
                    .find('>')
                    .map(|offset| at + offset)
                    .ok_or_else(|| format!("unterminated tag at byte {at}"))?;
                let inner = &html[at + 1..end];
                if let Some(name) = inner.strip_prefix('/') {
                    match stack.pop() {
                        Some(open) if open == name => {}
                        other => {
                            return Err(format!(
                                "</{name}> closes {other:?} at byte {at} — tags do not nest"
                            ));
                        }
                    }
                    if skipping == Some(stack.len()) {
                        skipping = None;
                    }
                } else {
                    let name = inner.split([' ', '/']).next().unwrap_or("");
                    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric()) {
                        return Err(format!(
                            "an UNESCAPED `<` reached the output at byte {at}: {:?}",
                            &html[at..end.min(at + 40)]
                        ));
                    }
                    if !VOID.contains(&name) {
                        if skipping.is_none() && has_class(inner, LIFTED) {
                            skipping = Some(stack.len());
                        }
                        stack.push(name);
                    }
                }
                at = end + 1;
            }
            b'&' => {
                let (text, len) =
                    entity(&html[at..]).ok_or_else(|| format!("an UNESCAPED `&` at byte {at}"))?;
                if skipping.is_none() {
                    out.push(text);
                }
                at += len;
            }
            byte => {
                let len = utf8_len(byte);
                if skipping.is_none() {
                    out.push_str(&html[at..at + len]);
                }
                at += len;
            }
        }
    }
    if let Some(open) = stack.last() {
        return Err(format!("<{open}> was never closed"));
    }
    Ok(out)
}

/// Does this open tag's `class` attribute hold `class` as a whole token?
fn has_class(inner: &str, class: &str) -> bool {
    let Some(at) = inner.find(" class=\"") else {
        return false;
    };
    let rest = &inner[at + " class=\"".len()..];
    let Some(end) = rest.find('"') else {
        return false;
    };
    rest[..end].split(' ').any(|token| token == class)
}

/// The four entities this fold's escapers emit, and nothing else — an `&` that
/// starts anything else means content leaked past `Out::text`.
fn entity(text: &str) -> Option<(char, usize)> {
    for (spelling, ch) in [
        ("&amp;", '&'),
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&quot;", '"'),
    ] {
        if text.starts_with(spelling) {
            return Some((ch, spelling.len()));
        }
    }
    None
}

fn utf8_len(byte: u8) -> usize {
    match byte {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

/// Every string in USJ's `content` arrays, in document order. Attribute values
/// are deliberately NOT collected: they are the lifts, which the HTML side marks
/// [`LIFTED`] and this test skips on both sides.
fn strings(value: &Value, out: &mut String) {
    match value {
        Value::String(text) => out.push_str(text),
        Value::Object(map) => {
            if let Some(Value::Array(items)) = map.get("content") {
                for item in items {
                    strings(item, out);
                }
            }
        }
        _ => {}
    }
}

/// Every whitespace run to ONE space, then trimmed — the comparison is about
/// CHARACTERS OF TEXT, and the two folds' seam handling is allowed to put a
/// space on either side of a marker (usj-export.md's "either side serializes
/// identically" ruling, in the small).
fn canonical(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_ws = false;
    for ch in text.chars() {
        if ch.is_whitespace() && ch != '\u{a0}' {
            in_ws = true;
            continue;
        }
        if in_ws && !out.is_empty() {
            out.push(' ');
        }
        in_ws = false;
        out.push(ch);
    }
    out
}

/// The first differing character, with a window either side — a whole book in a
/// failure line hides the finding.
fn divergence(ours: &str, theirs: &str) -> String {
    let at = ours
        .char_indices()
        .zip(theirs.char_indices())
        .find(|((_, a), (_, b))| a != b)
        .map(|((at, _), _)| at)
        .unwrap_or_else(|| ours.len().min(theirs.len()));
    let from = at.saturating_sub(60);
    let window = |text: &str| {
        let from = floor_char(text, from);
        let to = floor_char(text, (at + 60).min(text.len()));
        text[from..to].to_string()
    };
    format!(
        "text diverges at char {at} (ours {} chars, usj {} chars)\n  html: …{}…\n  usj:  …{}…",
        ours.chars().count(),
        theirs.chars().count(),
        window(ours),
        window(theirs)
    )
}

fn floor_char(text: &str, mut at: usize) -> usize {
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Every `<validated>pass</validated>` case's `origin.usfm`. Unlike the USJ and
/// USX oracles this needs no fixture of its own, so nothing is excluded: the
/// invariant is against OUR OWN USJ, not against the committee's bytes.
fn collect_test_data(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_test_data(&path, out);
        }
    }
    let source = dir.join("origin.usfm");
    if !source.is_file() {
        return;
    }
    let Ok(metadata) = std::fs::read_to_string(dir.join("metadata.xml")) else {
        return;
    };
    if metadata.contains("<validated>pass</validated>") {
        out.push(source);
    }
}

fn collect_corpora(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_corpora(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "usfm") {
            out.push(path);
        }
    }
}
