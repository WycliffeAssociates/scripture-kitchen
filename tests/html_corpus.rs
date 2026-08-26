//! THE HTML SMOKE + THE TEXT-IDENTITY INVARIANT, over every
//! `<validated>pass</validated>` case in `testData/` and every book in
//! `example-corpora/`.
//!
//! ```text
//! origin.usfm --lex--> --cst::build--> --html()--> strip tags  ==  strings of usj()
//! ```
//!
//! HTML has NO reference implementation and multiple readings are correct, so
//! this file pins the two things checkable without an oracle:
//!
//! 1. **SMOKE** — every document renders, every `<` in the output opens a tag we
//!    wrote (no content `<` escaped the escaper), and the tags NEST.
//! 2. **TEXT IDENTITY** — strip the tags and the remaining text equals the
//!    string content of our own USJ for the same input, after whitespace
//!    canonicalization. Dropped or duplicated content is then mechanical to
//!    catch, and only the aesthetic layer stays unpinned.
//!
//! The carve-out: USJ LIFTS things out of content into attributes (`\c`/`\v`
//! numbers, note callers, `\cat`, `\usfm`'s version, `\periph`'s title, `\rb`'s
//! gloss); HTML splats them into `data-*` too but ALSO renders them, because a
//! view whose chapter numbers and footnote callers are invisible is not a view.
//! Every such element carries `class="usfm-lifted"` and this test skips those
//! subtrees — ONE mechanism, not a per-case normalizer. The AUTO note-caller
//! number is the one piece of output text no token supplied, so it also carries
//! `note-caller-generated`: generated is distinguishable from relocated.

#![cfg(all(feature = "html", feature = "usj"))]

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde_json::Value;

use usfm_onion::{cst, html::html, lex, usj::usj};

/// "This element's text is not USJ string content" — anything wearing it is
/// skipped whole.
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

/// Strips the tags, skipping every subtree marked [`LIFTED`], and validates on
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
/// are deliberately NOT collected: they are the lifts, skipped on both sides.
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
/// CHARACTERS OF TEXT, and either fold may put a marker's seam space on either
/// side of it, since both spellings serialize back to the same USFM.
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

/// The first differing character, with a window either side — a whole book in
/// one failure line hides the finding.
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

/// Every `<validated>pass</validated>` case's `origin.usfm`. Nothing is
/// excluded: the invariant is against OUR OWN USJ, not the committee's bytes,
/// so the fixture errata the USJ and USX oracles must dodge cannot bite here.
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
