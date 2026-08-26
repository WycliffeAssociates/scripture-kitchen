//! THE USX ORACLE: every `<validated>pass</validated>` case in `testData/`
//! with an `origin.xml`, compared STRUCTURALLY (element name, attribute set,
//! child sequence, text) against ours.
//!
//! ```text
//! testData/basic/minimal/origin.usfm  --lex--> --cst::build--> --usx()-->
//!     <usx version="3.0">…</usx>   ==structurally==   origin.xml
//! ```
//!
//! testData is the COMMITTEE'S data, so the comparison is as EXACT as XML
//! permits. Two freedoms are forgiven, both properties of XML not USX:
//!
//! 1. **Attribute ORDER** — an unordered set per the XML spec, and the fixtures
//!    prove it (`<book code style>` but `<char style lemma>`). Compared as a MAP.
//! 2. **Pretty-print INDENTATION** — a whitespace-only text node CONTAINING A
//!    NEWLINE is the formatter's, and is dropped on both sides. A run of SPACES
//!    is KEPT: 7972 of them are real content (`</note> <verse eid…/>`), which is
//!    where the USX whitespace policy parts company with USJ's.
//!
//! Nothing else is normalized: `"the first verse "` keeps its trailing space,
//! `<para />` and `<para></para>` are the same element, and a divergence reports
//! the first structural path that differs (`usx/para[2]/char[0]@style`).
//!
//! The reader below is HAND-ROLLED rather than a dependency: the corpus has no
//! namespaces, no DTD, no comments, no CDATA and only the five predefined
//! entities, so roxmltree would buy generality nothing here needs — and `src/`
//! is a WRITER only, so there is no parser to reuse.

#![cfg(feature = "usx")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use usfm_onion::{cst, lex, usx::usx};

/// The cases this pin does NOT claim, matched by path suffix. A case may only
/// leave the pin if its reason fits on ONE line, in the same three categories
/// `tests/usj_corpus.rs` uses (ERRATUM / PURPOSEFUL / OPEN).
///
/// Thirteen, against USJ's twenty: eight cases the USJ pin cannot claim have an
/// `origin.xml` that agrees with US against their own `origin.json` — the six
/// seam-space fixtures, biblica/CrossRefWithPipe's phantom EOF space, and
/// biblica/PublishingVersesWithFormatting's correct `code="MAT"`.
const EXCLUDED: &[(&str, &str)] = &[
    // ---- ERRATUM: the fixture disagrees with its own origin.usfm ----------
    (
        "advanced/complex",
        "ERRATUM: omits every chapter/verse sid AND eid the rest of the corpus carries",
    ),
    (
        "advanced/footnote-structures",
        "ERRATUM: keeps a literal newline at the `\\f*` / `\\v 2` seam that every other fixture folds to one space",
    ),
    (
        "paratextTests/NoErrorsPartiallyEmptyBook",
        "ERRATUM: swallows `\\h` and `\\mt1` into the preceding `\\rem`'s content, where its OWN origin.json keeps three paras",
    ),
    (
        "paratextTests/WordlistMarkerMissingFromGlossaryCitationForms",
        "ERRATUM: invents a space at the `definition\\v 2` seam, where the source has none (same reading its origin.json takes)",
    ),
    (
        "specExamples/extended/contentCatogories1",
        "ERRATUM: literal newlines inside a note's text where the corpus otherwise collapses them to one space",
    ),
    (
        "specExamples/table",
        "ERRATUM: a cell keeps the raw newline + line indent (`…Zurishaddai\\n        `) that every other fixture folds",
    ),
    (
        "special-cases/empty-attributes",
        "ERRATUM: invents a trailing space on `\\w ആകാശവും|lemma=…` content",
    ),
    (
        "special-cases/figure_with_quotes_in_desc",
        "ERRATUM: unescapes `alt=\"He said: \\\"…\\\"\"` — USFM defines no escapes, so `\\\"` lexes as a marker (src/attributes.rs's stated law)",
    ),
    (
        "usfmjsTests/isa_inline_quotes",
        "ERRATUM: spends the `\\fqa seventy men. \\f*` trailing space TWICE — once inside the char, once as a phantom `\" \"` before `</note>`",
    ),
    (
        "usfmjsTests/usfmBodyTestD",
        "ERRATUM: puts `vid=\"TIT 1:3\"` on the very paragraph that STARTS verse 3, where a vid names the verse open at the block's start",
    ),
    // ---- PURPOSEFUL: the fixtures contradict, we follow the majority ------
    //
    // Unknown-marker pop-all recovery, the same three cases the USJ pin leaves
    // out for the same reason: `\s5` occurs 299 times in 21 validated-pass
    // fixtures and 19 of them read our way.
    (
        "usfmjsTests/luk_quotes",
        "PURPOSEFUL: wants `\\s5` to swallow the following `\\v 17` text; pop-all recovery stands",
    ),
    (
        "usfmjsTests/usfm-body-testF",
        "PURPOSEFUL: wants `\\s5` inside `\\esb` to leave the sidebar open (and NOT swallow, unlike luk_quotes); pop-all recovery stands",
    ),
    (
        "specExamples/milestone",
        "PURPOSEFUL: wants the row-0 milestone `\\zms\\*` to leave `\\q1` open; also carries the literal-newline erratum",
    ),
];

#[test]
fn usx_matches_every_validated_pass_fixture() {
    let root = Path::new("testData");
    if !root.is_dir() {
        eprintln!("usx corpus SKIPPED: no testData/");
        return;
    }

    let mut cases = Vec::new();
    collect(root, &mut cases);
    cases.sort();

    let results: Vec<Result<(), String>> = cases.par_iter().map(|case| check(case)).collect();

    let failures: Vec<&String> = results.iter().filter_map(|r| r.as_ref().err()).collect();
    let passed = results.len() - failures.len();
    eprintln!(
        "usx corpus: {passed}/{} fixtures match ({} excluded)",
        results.len(),
        EXCLUDED.len()
    );
    if !failures.is_empty() {
        let mut report = String::new();
        for story in &failures {
            report.push_str(story);
            report.push('\n');
        }
        panic!(
            "{}/{} fixtures diverge:\n{report}",
            failures.len(),
            results.len()
        );
    }
    // PINNED so the pin cannot shrink quietly. A new fixture or a mistyped
    // exclusion suffix moves this number and must be read.
    assert_eq!(
        cases.len(),
        195,
        "testData/ yielded {} claimable cases, expected 195 (208 validated-pass minus {} excluded)",
        cases.len(),
        EXCLUDED.len()
    );
}

/// Every directory holding `metadata.xml` + `origin.xml` + `origin.usfm` whose
/// metadata CONTAINS `<validated>pass</validated>` — a plain string search,
/// deliberately no XML parser for one flag.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        }
    }
    if !dir.join("origin.xml").is_file() || !dir.join("origin.usfm").is_file() {
        return;
    }
    let Ok(metadata) = std::fs::read_to_string(dir.join("metadata.xml")) else {
        return;
    };
    if !metadata.contains("<validated>pass</validated>") {
        return;
    }
    if EXCLUDED
        .iter()
        .any(|(suffix, _)| dir.to_string_lossy().ends_with(suffix))
    {
        return;
    }
    out.push(dir.to_path_buf());
}

fn check(case: &Path) -> Result<(), String> {
    let source = std::fs::read(case.join("origin.usfm")).expect("readable origin.usfm");
    let expected_text = std::fs::read_to_string(case.join("origin.xml")).expect("readable xml");

    let tokens = lex(std::str::from_utf8(&source).expect("utf-8 origin.usfm"));
    let cst = cst::build(&tokens);
    let ours = usx(&source, &tokens, &cst);

    let expected = parse(&expected_text)
        .map_err(|error| format!("{}: fixture is not XML: {error}", case.display()))?;
    let actual = parse(&ours)
        .map_err(|error| format!("{}: OUR output is not XML: {error}\n{ours}", case.display()))?;

    match first_divergence("", &actual, &expected) {
        None => Ok(()),
        Some(path) => Err(format!("{}: {path}", case.display())),
    }
}

// ---- The comparison -------------------------------------------------------

fn first_divergence(at: &str, ours: &Element, theirs: &Element) -> Option<String> {
    if ours.name != theirs.name {
        return Some(format!(
            "{at}: <{}> vs fixture <{}>",
            ours.name, theirs.name
        ));
    }
    let at = format!("{at}/{}", ours.name);
    for (key, value) in &theirs.attrs {
        match ours.attrs.get(key) {
            Some(mine) if mine == value => {}
            Some(mine) => {
                return Some(format!("{at}@{key}: {mine:?} vs fixture {value:?}"));
            }
            None => return Some(format!("{at}@{key}: MISSING (fixture has {value:?})")),
        }
    }
    for key in ours.attrs.keys() {
        if !theirs.attrs.contains_key(key) {
            return Some(format!(
                "{at}@{key}: EXTRA (ours has {:?})",
                ours.attrs[key]
            ));
        }
    }
    for (index, (a, b)) in ours.children.iter().zip(&theirs.children).enumerate() {
        match (a, b) {
            (Node::Text(a), Node::Text(b)) if a == b => {}
            (Node::Text(a), Node::Text(b)) => {
                return Some(format!("{at}[{index}] text {a:?} vs fixture {b:?}"));
            }
            (Node::Element(a), Node::Element(b)) => {
                if let Some(found) = first_divergence(&format!("{at}[{index}]"), a, b) {
                    return Some(found);
                }
            }
            _ => {
                return Some(format!(
                    "{at}[{index}]: {} vs fixture {}",
                    brief(a),
                    brief(b)
                ));
            }
        }
    }
    if ours.children.len() != theirs.children.len() {
        return Some(format!(
            "{at}: {} children, fixture has {}\n  ours:    {}\n  fixture: {}",
            ours.children.len(),
            theirs.children.len(),
            ours.children.iter().map(brief).collect::<String>(),
            theirs.children.iter().map(brief).collect::<String>(),
        ));
    }
    None
}

fn brief(node: &Node) -> String {
    let text = match node {
        Node::Text(text) => format!("{text:?}"),
        Node::Element(element) => format!("<{}>", element.name),
    };
    match text.char_indices().nth(80) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text,
    }
}

// ---- The hand-rolled reader -----------------------------------------------

#[derive(Debug)]
struct Element {
    name: String,
    /// A MAP: XML attribute order is not significant, and the fixtures use two
    /// different orders for two different elements.
    attrs: BTreeMap<String, String>,
    children: Vec<Node>,
}

#[derive(Debug)]
enum Node {
    Text(String),
    Element(Element),
}

/// One document's root element. Handles exactly what the corpus contains: an
/// optional `<?xml …?>` declaration, elements, attributes in either quote style,
/// self-closing tags, text, the five predefined entities, numeric refs.
fn parse(text: &str) -> Result<Element, String> {
    // A BOM is an encoding signature, not document content.
    let bytes: Vec<char> = text.trim_start_matches('\u{feff}').chars().collect();
    let mut at = 0usize;
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;

    while at < bytes.len() {
        if bytes[at] == '<' {
            if bytes.get(at + 1) == Some(&'?') {
                at = find(&bytes, at, "?>")? + 2;
                continue;
            }
            if bytes.get(at + 1) == Some(&'/') {
                let end = find(&bytes, at, ">")?;
                let done = stack.pop().ok_or("closing tag with nothing open")?;
                let name: String = bytes[at + 2..end].iter().collect();
                if name.trim() != done.name {
                    return Err(format!("</{}> closes <{}>", name.trim(), done.name));
                }
                push(&mut stack, &mut root, done)?;
                at = end + 1;
                continue;
            }
            let mut cursor = at + 1;
            while cursor < bytes.len() && !" \t\r\n/>".contains(bytes[cursor]) {
                cursor += 1;
            }
            let mut element = Element {
                name: bytes[at + 1..cursor].iter().collect(),
                attrs: BTreeMap::new(),
                children: Vec::new(),
            };
            loop {
                while cursor < bytes.len() && bytes[cursor].is_whitespace() {
                    cursor += 1;
                }
                match bytes.get(cursor) {
                    Some('>') => {
                        stack.push(element);
                        at = cursor + 1;
                        break;
                    }
                    Some('/') => {
                        push(&mut stack, &mut root, element)?;
                        at = find(&bytes, cursor, ">")? + 1;
                        break;
                    }
                    Some(_) => {
                        let start = cursor;
                        while cursor < bytes.len() && bytes[cursor] != '=' {
                            cursor += 1;
                        }
                        let name: String = bytes[start..cursor].iter().collect();
                        let quote = *bytes.get(cursor + 1).ok_or("attribute has no value")?;
                        let value_start = cursor + 2;
                        cursor = value_start;
                        while cursor < bytes.len() && bytes[cursor] != quote {
                            cursor += 1;
                        }
                        let value: String = bytes[value_start..cursor].iter().collect();
                        element
                            .attrs
                            .insert(name.trim().to_string(), unescape(&value));
                        cursor += 1;
                    }
                    None => return Err("unterminated tag".to_string()),
                }
            }
            continue;
        }
        let start = at;
        while at < bytes.len() && bytes[at] != '<' {
            at += 1;
        }
        let raw: String = bytes[start..at].iter().collect();
        // The ONE normalization: a whitespace-only run containing a NEWLINE is
        // the formatter's indentation. A run of spaces IS content in USX.
        if raw.trim().is_empty() && raw.contains('\n') {
            continue;
        }
        let text = unescape(&raw);
        match stack.last_mut() {
            Some(open) => open.children.push(Node::Text(text)),
            None if text.trim().is_empty() => {}
            None => return Err(format!("text {text:?} outside the root element")),
        }
    }
    if !stack.is_empty() {
        return Err(format!("unclosed <{}>", stack[stack.len() - 1].name));
    }
    root.ok_or_else(|| "no root element".to_string())
}

fn push(stack: &mut [Element], root: &mut Option<Element>, element: Element) -> Result<(), String> {
    match stack.last_mut() {
        Some(parent) => parent.children.push(Node::Element(element)),
        None if root.is_some() => return Err("second root element".to_string()),
        None => *root = Some(element),
    }
    Ok(())
}

fn find(bytes: &[char], from: usize, needle: &str) -> Result<usize, String> {
    let needle: Vec<char> = needle.chars().collect();
    (from..bytes.len().saturating_sub(needle.len() - 1))
        .find(|at| bytes[*at..*at + needle.len()] == needle[..])
        .ok_or_else(|| {
            format!(
                "no {:?} after byte {from}",
                needle.iter().collect::<String>()
            )
        })
}

fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let Some(end) = rest[at..].find(';').map(|offset| at + offset) else {
            out.push('&');
            rest = &rest[at + 1..];
            continue;
        };
        match &rest[at + 1..end] {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            entity if entity.starts_with("#x") || entity.starts_with("#X") => {
                let code = u32::from_str_radix(&entity[2..], 16).unwrap_or(0xfffd);
                out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
            }
            entity if entity.starts_with('#') => {
                let code: u32 = entity[1..].parse().unwrap_or(0xfffd);
                out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
            }
            other => {
                out.push('&');
                out.push_str(other);
                out.push(';');
            }
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}
