//! What the export folds SHARE — the pieces that are the same fact in USJ and
//! USX.
//!
//! ```text
//! canonical("son of  david")   ->  "son of david"     every ws run is ONE space
//! canonical("a~b")             ->  "a\u{a0}b"          `~` is the NBSP it names
//! marker_name(b"\\+nd ")       ->  "nd"               the AUTHOR's spelling
//! ```
//!
//! Deliberately NOT here: the walker. A JSON array's comma bookkeeping and an
//! XML element's rewind-to-self-closing are not the same state, so `usj.rs` and
//! `usx.rs` each weave their own writer through the fold and a shared driver
//! would be a trait with two implementors. What IS shared is what a fold READS:
//! whitespace canonicalization, marker spelling, the note-peer sets.

use core::ops::Range;
use std::borrow::Cow;

use crate::Token;

/// A FOOTNOTE's own PEER markers — the note-text elements that sit BESIDE each
/// other inside `\f`/`\fe`/`\ef`. `\fv` is absent because it NESTS instead:
/// `\fv ...\fv*` is a verse number inside footnote text (specExamples/footnote).
/// `\fm` is absent as char-like, per its row.
pub(crate) const FOOTNOTE_PEERS: &[&str] =
    &["fdc", "fk", "fl", "fp", "fq", "fqa", "fr", "ft", "fw"];

/// A CROSS-REFERENCE's own peer markers, inside `\x`/`\ex`. `\xt` is one of
/// THESE and not a footnote's, which is the whole of why `\xo 1.1 \xt Ps 135`
/// siblings while `\ft … \xt ref\xt*` nests.
pub(crate) const XREF_PEERS: &[&str] = &["xdc", "xk", "xnt", "xo", "xop", "xot", "xq", "xt", "xta"];

/// Which peer family a note marker belongs to. `\x`/`\ex` are the
/// cross-reference notes; `\f`/`\fe`/`\ef` are the footnotes.
pub(crate) fn note_peers(marker: &str) -> &'static [&'static str] {
    match marker {
        "x" | "ex" => XREF_PEERS,
        _ => FOOTNOTE_PEERS,
    }
}

/// Space, tab, CR, LF — the scanner's structural whitespace, and deliberately
/// not `char::is_whitespace`: a no-break space in the text is CONTENT.
pub(crate) fn is_ws(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

pub(crate) fn trim_start(text: &str) -> &str {
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() && is_ws(bytes[at]) {
        at += 1;
    }
    &text[at..]
}

pub(crate) fn trim_end(text: &str) -> &str {
    let bytes = text.as_bytes();
    let mut to = bytes.len();
    while to > 0 && is_ws(bytes[to - 1]) {
        to -= 1;
    }
    &text[..to]
}

pub(crate) fn trim(text: &str) -> &str {
    trim_end(trim_start(text))
}

/// One text token, canonicalized the way the fixtures are: every run of
/// structural whitespace becomes ONE space, and `~` becomes the non-breaking
/// space it names.
pub(crate) fn canonical(text: &str) -> Cow<'_, str> {
    // Borrow on the common case: allocating per TEXT TOKEN would allocate once
    // per word of scripture.
    let bytes = text.as_bytes();
    let untouched = !bytes.iter().enumerate().any(|(at, &byte)| {
        byte == b'~'
            || (is_ws(byte) && (byte != b' ' || bytes.get(at + 1).is_some_and(|next| is_ws(*next))))
    });
    if untouched {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut in_ws = false;
    for byte in text.chars() {
        match byte {
            ch if ch.is_ascii() && is_ws(ch as u8) => {
                if !in_ws {
                    out.push(' ');
                }
                in_ws = true;
            }
            '~' => {
                out.push('\u{a0}');
                in_ws = false;
            }
            ch => {
                out.push(ch);
                in_ws = false;
            }
        }
    }
    Cow::Owned(out)
}

/// One byte range of the source as `str`. The scanner never splits a UTF-8
/// sequence, so this only ever fails on a source that was not UTF-8.
pub(crate) fn text_at(source: &[u8], range: Range<u32>) -> &str {
    let bytes = &source[range.start as usize..range.end as usize];
    core::str::from_utf8(bytes).unwrap_or("")
}

/// One token's own span.
pub(crate) fn span<'a>(source: &'a [u8], token: &Token) -> &'a str {
    text_at(source, token.start..token.end())
}

/// The marker as the AUTHOR spelled it: `q1`, `qt-s`, `zaln-e` — read off the
/// token's own bytes, not off the row (numbered markers share one row, and
/// milestones share one row with both halves of the pair).
pub(crate) fn marker_name(source: &[u8], token: &Token) -> String {
    let text = span(source, token);
    let text = text.strip_prefix('\\').unwrap_or(text);
    let text = text.strip_prefix('+').unwrap_or(text);
    let text = trim_end(text);
    let text = text.strip_suffix('*').unwrap_or(text);
    trim_end(text).to_string()
}
