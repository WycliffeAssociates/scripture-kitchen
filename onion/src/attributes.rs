//! The attribute k/v INTERPRETER: the one place that reads inside a
//! [`TokenKind::AttrList`](crate::TokenKind::AttrList) span, and the one place
//! that knows how an attribute NAME matches the marker table.
//! [`designator`](crate::designator)'s sibling: borrowed spans, no allocation,
//! no repair — `Malformed` is recorded, never resynchronized. Consumers:
//! lint's attr rules, exports' k/v splatting (the LOSSY step; the token
//! itself stays byte-identical).
//!
//! # By example (each line is one AttrList token; `→` is the event stream)
//!
//! ```text
//! |x-strong="G2532" x-lemma="καί"  →  x-strong=G2532, x-lemma=καί   the corpus majority
//! |in                              →  lemma=in            bare: whole interior → row's default_attribute
//! |Fred Smith                      →  lemma=Fred Smith    bare is GREEDY: free text, ONE event, no invented junk
//! |keyword ␠                       →  lemma=keyword␠      bare keeps its TRAILING HS: no closing pipe, no delimiter
//! |lemma=grace                     →  lemma=grace         unquoted pair value; ends at HS or interior end
//! |lemma="a"strong="G1"            →  lemma=a, strong=G1  a closing quote is its own delimiter
//! |aid="x"| ␠                      →  aid=x               node-initial (U25001): closing pipe + its inner HS trimmed
//! |lemma="grace                    →  Malformed(UnterminatedQuote)       a pair ATTEMPTED and unfinished
//! |lemma="a", strong="G1"          →  lemma=a, Malformed(BareJunk @ ,)   comma is never a separator
//! |="x"                            →  Malformed(EmptyName)
//! ```
//!
//! `Malformed` ENDS iteration — a broken tail is one finding, not many.
//! Pairs-vs-bare is decided ONCE from the first bytes: name-run + `=` opens
//! the pair form, anything else makes the whole interior one bare value.
//!
//! # Grammar
//!
//! ```text
//! interior := HS* (pairs | bare) HS*     interior = span minus delimiter pipe(s)
//! pairs    := pair (HS+ pair)*
//! pair     := name '=' value
//! name     := [A-Za-z0-9_-]+
//! value    := '"' [^"]* '"' | [^ \t]+
//! bare     := the whole interior, verbatim
//! ```
//!
//! HS = space/tab; CR/LF cannot occur (a list never spans a line —
//! `attr_list_end`). Whitespace is the ONLY separator: usfmtc silently DROPS
//! everything after a comma, so there is no comma dialect to honour — we flag
//! where the reference implementation loses data. The name charset enforces it:
//! a pair may only begin where a name byte does, so any stray byte where a pair
//! was due is `BareJunk`.
//!
//! # Bytes we refuse to touch (the spec defines NO escapes; neither do we)
//!
//! ```text
//! |x="a\|b"   →  x=a\|b            \| and \\ pass through VERBATIM (unescaping
//!                                  = allocation + a WRITER's job, never a reader's)
//! |x="a\"b"   →  x=a\  + BareJunk  a quoted value ends at the FIRST '"', whatever precedes
//! |"quoted"   →  lemma="quoted"    bare keeps its quote bytes (delimiters only in a pair)
//! ```
//!
//! Only a quoted PAIR value's quotes are stripped — that one interpretation
//! is the module's point.
//!
//! # vs usfmtc (every divergence deliberate)
//!
//! On ANY pair failure usfmtc silently rereads the WHOLE interior as the
//! default value (`lemma=` → the value `lemma=`). That is a repair, and
//! repairing is not ours: a failed pair attempt here is `Malformed` — the
//! finding lint wants, the data loss exports must not paper over. We also
//! READ `lemma=grace` where usfmtc rejects-then-swallows (accepting it
//! invents no bytes).
//!
//! # Cost
//!
//! One forward pass, no allocation, on demand (never during a scan). Corpus
//! sweep (tests/attr_corpus.rs): 1.25M lists, 4.35M attributes, 226 books,
//! ~0.1s wall lex included — and ZERO malformed, which is the sweep's point.

use core::ops::Range;

use crate::TokenKind;
use crate::tables::generated::{self, MarkerIdx};
use crate::tables::schema::AttrStatus;
use crate::token::Token;

/// One attribute as the bytes say it is. Borrowed spans of the SOURCE — this
/// type never owns text, and the iterator that yields it never allocates.
///
/// `Clone` but not `Copy`, only because [`Range`] is not `Copy`; every field is
/// two words of borrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr<'a> {
    /// The attribute name, or EMPTY for the bare default-value form
    /// (`\w In|in\w*`). An empty name is resolved through the row's
    /// `default_attribute` — [`resolve`] does exactly that.
    pub name: &'a [u8],
    /// The value, with the quotes of a quoted pair value stripped and nothing
    /// else touched (see the module doc on escapes).
    pub value: &'a [u8],
    /// ABSOLUTE source range of `name`. Lint anchors point INSIDE tokens, so
    /// these are the whole reason the interpreter reports ranges at all. Empty
    /// (start == end, at the value) for the bare form.
    pub name_span: Range<u32>,
    /// ABSOLUTE source range of `value`, quotes EXCLUDED — the range covers
    /// exactly the bytes in `value`.
    pub value_span: Range<u32>,
}

/// What one step of the walk found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrEvent<'a> {
    Attr(Attr<'a>),
    /// The bytes stopped making sense at absolute offset `at`. ALWAYS the last
    /// event: a broken tail is one finding, not one per remaining byte, and
    /// resynchronizing would mean guessing where the author's intent resumes.
    Malformed {
        at: u32,
        why: MalformedAttr,
    },
}

/// Why a list stopped parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MalformedAttr {
    /// A value opened with `"` and no second `"` followed. `at` is the OPENING
    /// quote — the byte the author must look at.
    UnterminatedQuote,
    /// An `=` with no name before it (`|="x"`).
    EmptyName,
    /// A `name=` with nothing after the `=` (`|lemma=`).
    MissingValue,
    /// Where a pair should begin, something else does: a comma between pairs,
    /// a bare word after a pair, a stray quote, the tail after a `\"`.
    BareJunk,
}

/// Walks one AttrList token's interior.
///
/// `Copy` state over borrowed bytes — the type is the zero-allocation
/// assertion, not a test.
#[derive(Debug, Clone, Copy)]
pub struct AttrIter<'a> {
    /// The interior, horizontal whitespace and delimiting pipe(s) removed.
    interior: &'a [u8],
    /// Absolute offset of `interior[0]`, so every reported range is absolute.
    base: u32,
    at: usize,
    /// The whole interior is one unnamed default value (decided in [`attrs`]).
    bare: bool,
    done: bool,
}

/// Reads the k/v view of one `AttrList` token, on demand.
///
/// `list` must be an [`TokenKind::AttrList`] token of `source`; the interior is
/// derived per the module doc.
pub fn attrs<'a>(source: &'a [u8], list: &Token) -> AttrIter<'a> {
    debug_assert_eq!(list.kind(), TokenKind::AttrList);
    let span = &source[list.start as usize..list.end() as usize];

    // Leading `|` always. The CLOSING `|` of the node-initial form (U25001)
    // next, with the HS on either side of it — that HS is delimiter, and the
    // closing pipe is what says so. Without a closing pipe there is no
    // delimiter at the tail, so trailing HS is the AUTHOR'S BYTES and stays:
    // `\w word|keyword \w*` reads `keyword ` (testData
    // paratextTests/WordlistMarkerKeywordEndsInSpace and its five siblings).
    let mut from = usize::from(span.first() == Some(&PIPE));
    let mut to = span.len();
    let before_hs = trim_hs(span, from, to);
    if before_hs > from && span[before_hs - 1] == PIPE {
        to = trim_hs(span, from, before_hs - 1);
    }
    while from < to && is_hs(span[from]) {
        from += 1;
    }

    let interior = &span[from..to];
    AttrIter {
        interior,
        base: list.start + from as u32,
        at: 0,
        // A name run followed by `=` is the pair form; anything else (a bare
        // word, a quote, non-ASCII, `=` with no name) is not, and the whole
        // interior is one value. `=` FIRST is the exception: that is a pair
        // attempt with no name, and the loop reports it as such.
        bare: !interior.is_empty() && !starts_pair(interior) && interior[0] != b'=',
        done: interior.is_empty(),
    }
}

impl<'a> Iterator for AttrIter<'a> {
    type Item = AttrEvent<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        if self.bare {
            self.done = true;
            let span = self.base..self.base + self.interior.len() as u32;
            return Some(AttrEvent::Attr(Attr {
                name: &self.interior[..0],
                value: self.interior,
                name_span: self.base..self.base,
                value_span: span,
            }));
        }
        self.next_pair()
    }
}

impl<'a> AttrIter<'a> {
    /// One `name = value` pair, or the one Malformed event that ends the walk.
    fn next_pair(&mut self) -> Option<AttrEvent<'a>> {
        while self.at < self.interior.len() && is_hs(self.interior[self.at]) {
            self.at += 1;
        }
        if self.at == self.interior.len() {
            self.done = true;
            return None;
        }

        let name_from = self.at;
        while self
            .interior
            .get(self.at)
            .is_some_and(|&byte| is_name_byte(byte))
        {
            self.at += 1;
        }
        let name_to = self.at;
        let name = &self.interior[name_from..name_to];

        // Everything that is not `name` `=` is junk, told apart only so the
        // finding can name the author's mistake.
        match self.interior.get(self.at) {
            Some(b'=') if !name.is_empty() => self.at += 1,
            Some(b'=') => return self.malformed(self.at, MalformedAttr::EmptyName),
            _ => return self.malformed(name_from, MalformedAttr::BareJunk),
        }

        let value_from;
        let value_to;
        match self.interior.get(self.at) {
            None => return self.malformed(self.at - 1, MalformedAttr::MissingValue),
            Some(&byte) if is_hs(byte) => {
                return self.malformed(self.at - 1, MalformedAttr::MissingValue);
            }
            Some(b'"') => {
                // No escapes: the first `"` closes the value, whatever precedes it.
                let quote_at = self.at;
                value_from = self.at + 1;
                match next_quote(&self.interior[value_from..]) {
                    Some(offset) => {
                        value_to = value_from + offset;
                        self.at = value_to + 1;
                    }
                    None => return self.malformed(quote_at, MalformedAttr::UnterminatedQuote),
                }
            }
            Some(_) => {
                value_from = self.at;
                while self.interior.get(self.at).is_some_and(|&byte| !is_hs(byte)) {
                    self.at += 1;
                }
                value_to = self.at;
            }
        }

        Some(AttrEvent::Attr(Attr {
            name,
            value: &self.interior[value_from..value_to],
            name_span: self.base + name_from as u32..self.base + name_to as u32,
            value_span: self.base + value_from as u32..self.base + value_to as u32,
        }))
    }

    /// Records the verdict and ENDS the walk.
    fn malformed(&mut self, at: usize, why: MalformedAttr) -> Option<AttrEvent<'a>> {
        self.done = true;
        Some(AttrEvent::Malformed {
            at: self.base + at as u32,
            why,
        })
    }
}

/// What the marker table says about one attribute NAME.
///
/// `Copy`, and it carries the matched DEFINITION rather than just a verdict, so
/// a consumer reads [`AttrStatus`] without a second lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrResolution {
    /// Matched a `defined_attributes` entry. `defined` is the ENTRY's name, so
    /// a wildcard hit reports the pattern (`"a-*"`) and an exact hit reports
    /// itself.
    Defined {
        defined: &'static str,
        status: AttrStatus,
    },
    /// The `x-`/`z-` user namespace: non-canonical by design, never in the
    /// table, and legal wherever attributes are.
    UserNamespace,
    /// Nothing matched — including the bare default value on a row that has no
    /// `default_attribute` (`\fig`), which is a shape that PARSED and a
    /// question for lint, never a [`MalformedAttr`].
    Unknown,
}

/// Matches one attribute name against a marker's row. THE one place that
/// learns naming conventions, in this order:
///
/// 1. An EMPTY name is the bare default-value form, and resolves through the
///    row's `default_attribute` (not the first entry — `\fig`'s would be
///    `src`, and it has none at all).
/// 2. Exact match against `defined_attributes`.
/// 3. The `"a-*"` PREFIX WILDCARD sentinel (schema.rs): `\ta` defines no fixed
///    names, only "each attribute should begin with `a-`".
/// 4. The `x-`/`z-` user namespace. NOT restricted to character markers even
///    though the spec words it that way: en_ult's `\zaln-s` milestones carry
///    `x-strong`/`x-lemma` on every aligned word, so a kind test here would
///    flag a million legitimate attributes.
///
/// Exact beats wildcard beats namespace, which matters for nothing today (no
/// defined name starts with `x-`) and is pinned by test so it keeps mattering
/// for nothing.
pub fn resolve(name: &[u8], marker_idx: MarkerIdx) -> AttrResolution {
    let defined = generated::defined_attributes(marker_idx);

    if name.is_empty() {
        let Some(default) = generated::default_attribute(marker_idx) else {
            return AttrResolution::Unknown;
        };
        return match defined.iter().find(|(entry, _)| *entry == default) {
            Some(&(entry, status)) => AttrResolution::Defined {
                defined: entry,
                status,
            },
            // Codegen derives `default_attribute` from the same list, so this
            // is unreachable; it is a table bug if it ever is not.
            None => AttrResolution::Unknown,
        };
    }

    for &(entry, status) in defined {
        if entry.as_bytes() == name {
            return AttrResolution::Defined {
                defined: entry,
                status,
            };
        }
    }
    for &(entry, status) in defined {
        if entry
            .strip_suffix('*')
            .is_some_and(|prefix| name.starts_with(prefix.as_bytes()))
        {
            return AttrResolution::Defined {
                defined: entry,
                status,
            };
        }
    }
    if name.starts_with(b"x-") || name.starts_with(b"z-") {
        return AttrResolution::UserNamespace;
    }
    AttrResolution::Unknown
}

const PIPE: u8 = b'|';

/// Horizontal whitespace: the only separator, and the only byte U25001 lets a
/// list absorb after its closing pipe. CR/LF never reach here.
fn is_hs(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// The name charset. ASCII only, deliberately: every defined name and every
/// `x-`/`a-` name in 226 books is ASCII, and a non-ASCII byte here is the
/// signal that this is the BARE form (`|καί`), not a misspelled pair.
fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

/// Does the interior open with a pair ATTEMPT — a name run then `=`?
fn starts_pair(interior: &[u8]) -> bool {
    let mut at = 0;
    while interior.get(at).is_some_and(|&byte| is_name_byte(byte)) {
        at += 1;
    }
    at > 0 && interior.get(at) == Some(&b'=')
}

/// Index of the next `"`. Its own function only to keep the value arm short;
/// a list is tens of bytes, so a plain loop beats a vectorized search.
fn next_quote(bytes: &[u8]) -> Option<usize> {
    bytes.iter().position(|&byte| byte == b'"')
}

/// Walks `to` back over horizontal whitespace, never past `from`.
fn trim_hs(span: &[u8], from: usize, mut to: usize) -> usize {
    while to > from && is_hs(span[to - 1]) {
        to -= 1;
    }
    to
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex;

    /// Lexes one source and walks its FIRST attribute list.
    fn walk(source: &str) -> Vec<AttrEvent<'_>> {
        let tokens = lex(source);
        let list = tokens
            .iter()
            .find(|token| token.kind() == TokenKind::AttrList)
            .copied()
            .expect("no attribute list lexed");
        attrs(source.as_bytes(), &list).collect()
    }

    /// `(name, value)` pairs as text, asserting every event parsed.
    fn pairs(source: &str) -> Vec<(&str, &str)> {
        walk(source)
            .into_iter()
            .map(|event| match event {
                AttrEvent::Attr(attr) => (
                    core::str::from_utf8(attr.name).unwrap(),
                    core::str::from_utf8(attr.value).unwrap(),
                ),
                AttrEvent::Malformed { at, why } => panic!("unexpected {why:?} at {at}"),
            })
            .collect()
    }

    fn idx(marker: &str) -> MarkerIdx {
        generated::marker_idx(
            marker.as_bytes(),
            crate::tables::schema::SpellingShape::PlainOnly,
        )
    }

    #[test]
    fn the_real_alignment_shape() {
        // en_ult's every-word milestone: user namespace, many pairs, Greek.
        assert_eq!(
            pairs("\\zaln-s |x-strong=\"G2532\" x-lemma=\"καί\" x-morph=\"Gr,CC,,,,,,,,\"\\*"),
            vec![
                ("x-strong", "G2532"),
                ("x-lemma", "καί"),
                ("x-morph", "Gr,CC,,,,,,,,"),
            ]
        );
        // Commas inside a quoted value are just bytes — only a comma BETWEEN
        // pairs is junk.
    }

    #[test]
    fn the_bare_default_form_is_one_greedy_value() {
        assert_eq!(pairs("\\w In|in\\w*"), vec![("", "in")]);
        assert_eq!(pairs("\\w Jésus|Jesus\\w*"), vec![("", "Jesus")]);
        // Free text: not tokenized, so no false junk on a two-word gloss.
        assert_eq!(pairs("\\w x|Fred Smith\\w*"), vec![("", "Fred Smith")]);
        // Quotes are not delimiters in the bare form (usfmtc agrees).
        assert_eq!(pairs("\\w x|\"quoted\"\\w*"), vec![("", "\"quoted\"")]);
        // Non-ASCII first byte is the bare signal, never a misspelled name.
        assert_eq!(pairs("\\w x|καί\\w*"), vec![("", "καί")]);
    }

    #[test]
    fn mixed_quoted_pairs() {
        assert_eq!(
            pairs("\\w gracious|lemma=\"grace\" strong=\"G5485\"\\w*"),
            vec![("lemma", "grace"), ("strong", "G5485")]
        );
        // An unquoted pair value ends at whitespace (or at the end).
        assert_eq!(
            pairs("\\w x|lemma=grace strong=\"G5485\"\\w*"),
            vec![("lemma", "grace"), ("strong", "G5485")]
        );
        assert_eq!(pairs("\\w x|lemma=grace\\w*"), vec![("lemma", "grace")]);
        // Extra separator whitespace, and a tab.
        assert_eq!(
            pairs("\\w x|lemma=\"a\"  \tstrong=\"b\"\\w*"),
            vec![("lemma", "a"), ("strong", "b")]
        );
        // An empty quoted value is a value.
        assert_eq!(pairs("\\w x|lemma=\"\"\\w*"), vec![("lemma", "")]);
    }

    #[test]
    fn figs_six_attributes_and_its_missing_default() {
        assert_eq!(
            pairs(
                "\\fig |alt=\"a\" src=\"b.png\" size=\"span\" loc=\"x\" copy=\"c\" ref=\"1:1\"\\fig*"
            ),
            vec![
                ("alt", "a"),
                ("src", "b.png"),
                ("size", "span"),
                ("loc", "x"),
                ("copy", "c"),
                ("ref", "1:1"),
            ]
        );
        // The shape PARSES; that fig has no default is resolution's answer.
        assert_eq!(pairs("\\fig |a.png\\fig*"), vec![("", "a.png")]);
        assert_eq!(resolve(b"", idx("fig")), AttrResolution::Unknown);
        assert_eq!(
            resolve(b"src", idx("fig")),
            AttrResolution::Defined {
                defined: "src",
                status: AttrStatus::Required,
            }
        );
    }

    #[test]
    fn resolution_learns_the_four_conventions() {
        // Exact.
        assert_eq!(
            resolve(b"lemma", idx("w")),
            AttrResolution::Defined {
                defined: "lemma",
                status: AttrStatus::Optional,
            }
        );
        // Default, via the empty name — `w`'s default is `lemma`.
        assert_eq!(resolve(b"", idx("w")), resolve(b"lemma", idx("w")));
        // The `a-*` wildcard: a hit reports the PATTERN, a miss is Unknown.
        assert_eq!(
            resolve(b"a-plus", idx("ta")),
            AttrResolution::Defined {
                defined: "a-*",
                status: AttrStatus::Optional,
            }
        );
        assert_eq!(resolve(b"foo", idx("ta")), AttrResolution::Unknown);
        // A wildcard row still resolves nothing for a bare value.
        assert_eq!(resolve(b"", idx("ta")), AttrResolution::Unknown);
        // User namespace, on a character marker AND on a milestone.
        assert_eq!(
            resolve(b"x-strong", idx("w")),
            AttrResolution::UserNamespace
        );
        // en_ult's `\zaln-s` is a custom `\z` marker, so it resolves to row 0
        // (UNRESOLVED) — which has no attributes and must STILL say
        // UserNamespace, or a million aligned words become findings.
        assert_eq!(
            resolve(b"x-strong", generated::UNRESOLVED),
            AttrResolution::UserNamespace
        );
        assert_eq!(resolve(b"z-mine", idx("w")), AttrResolution::UserNamespace);
        // Unknown on a row with attributes, and on a row with none.
        assert_eq!(resolve(b"nope", idx("w")), AttrResolution::Unknown);
        assert_eq!(resolve(b"nope", idx("add")), AttrResolution::Unknown);
        // Deprecated is carried through, not hidden.
        assert_eq!(
            resolve(b"link-href", idx("xt")),
            AttrResolution::Defined {
                defined: "link-href",
                status: AttrStatus::Deprecated,
            }
        );
    }

    #[test]
    fn each_malformed_shape_yields_exactly_one_event_then_stops() {
        let one = |source: &str| -> (u32, MalformedAttr) {
            let events = walk(source);
            assert_eq!(events.len(), 1, "{source}: {events:?}");
            match events[0] {
                AttrEvent::Malformed { at, why } => (at, why),
                ref other => panic!("{source}: expected Malformed, got {other:?}"),
            }
        };
        // Offsets are ABSOLUTE, pointing at the byte to blame.
        // `\w x|` is 5 bytes, so the interior starts at 5.
        assert_eq!(
            one("\\w x|lemma=\"grace\\w*"),
            (11, MalformedAttr::UnterminatedQuote)
        );
        assert_eq!(one("\\w x|=\"x\"\\w*"), (5, MalformedAttr::EmptyName));
        assert_eq!(one("\\w x|lemma=\\w*"), (10, MalformedAttr::MissingValue));
        assert_eq!(one("\\w x|lemma= \\w*"), (10, MalformedAttr::MissingValue));
    }

    #[test]
    fn an_escaped_quote_never_reaches_this_module() {
        // `\"` is neither an escape (`escape_len` knows only `\|` and `\\`) nor
        // a closer, so `attr_list_end` returns NotAList and the bytes stay TEXT.
        let source = "\\w x|lemma=\"a\\\"b\"\\w*";
        assert!(!lex(source).iter().any(|t| t.kind() == TokenKind::AttrList));

        // Fed a synthetic list anyway, the reading is the documented one: the
        // first `"` closes the value at `a\`, and `b"` is junk.
        let list = Token {
            start: 4,
            len: 13,
            kind_bits: TokenKind::AttrList.to_bits(),
            marker_idx: 0,
        };
        let events: Vec<_> = attrs(source.as_bytes(), &list).collect();
        assert_eq!(events.len(), 2);
        let AttrEvent::Attr(attr) = &events[0] else {
            panic!("{events:?}");
        };
        assert_eq!(attr.value, b"a\\");
        assert_eq!(
            events[1],
            AttrEvent::Malformed {
                at: 15,
                why: MalformedAttr::BareJunk
            }
        );
    }

    #[test]
    fn malformed_ends_the_walk_after_the_good_pairs() {
        // The broken tail is ONE finding, and the pairs before it survive.
        let events = walk("\\w x|lemma=\"a\" strong=\"b\" oops\\w*");
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[2],
            AttrEvent::Malformed {
                why: MalformedAttr::BareJunk,
                ..
            }
        ));
    }

    #[test]
    fn a_comma_between_pairs_is_junk_at_the_comma() {
        // There is no comma dialect. usfmtc drops `strong` silently in both of
        // these; we keep the pair before the comma and flag AT the comma.
        let events = walk("\\w x|lemma=\"a\", strong=\"G1\"\\w*");
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[1],
            AttrEvent::Malformed {
                at: 14,
                why: MalformedAttr::BareJunk
            }
        );
        let events = walk("\\w x|lemma=\"a\",strong=\"G1\"\\w*");
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[1],
            AttrEvent::Malformed {
                at: 14,
                why: MalformedAttr::BareJunk
            }
        );
    }

    #[test]
    fn a_value_is_its_own_right_delimiter() {
        // No separator between pairs: accepted, because the closing quote
        // already ended the value and usfmtc reads it the same way. The
        // comma case above is junk for a different reason — `,` cannot start
        // a name.
        assert_eq!(
            pairs("\\w x|lemma=\"a\"strong=\"G1\"\\w*"),
            vec![("lemma", "a"), ("strong", "G1")]
        );
    }

    #[test]
    fn both_list_forms_strip_their_own_delimiters() {
        // Node-initial (U25001): leading pipe, closing pipe, and the HS the
        // token absorbed after it.
        assert_eq!(pairs("\\w|lemma=\"a\"|\\w*"), vec![("lemma", "a")]);
        assert_eq!(
            pairs("\\p|cat=\"emphasised\"| text"),
            vec![("cat", "emphasised")]
        );
        // ... including the space the fold absorbs BEFORE the opening pipe,
        // and any HS on the inside of the closing one.
        assert_eq!(pairs("\\f |aid=\"mynote\"| +"), vec![("aid", "mynote")]);
        assert_eq!(pairs("\\f |aid=\"x\" | +"), vec![("aid", "x")]);
        // Trailing (3.1): no closing pipe to strip.
        assert_eq!(pairs("\\w x|lemma=\"a\"\\w*"), vec![("lemma", "a")]);
        // Node-initial bare value: the pipes are not part of the value.
        assert_eq!(pairs("\\w|Jesus|\\w*"), vec![("", "Jesus")]);
        assert_eq!(pairs("\\w|Jesus\\w*"), vec![("", "Jesus")]);
    }

    #[test]
    fn an_empty_list_yields_nothing() {
        // The pipe alone is a legal token (scanner.rs pins it), and it says
        // nothing — no attribute, and no complaint either.
        assert_eq!(walk("\\w|\\w*"), vec![]);
        assert_eq!(walk("\\w||\\w*"), vec![]);
        assert_eq!(walk("\\p|| x"), vec![]);
        // Whitespace only, both forms.
        assert_eq!(walk("\\p| | x"), vec![]);
        assert_eq!(walk("\\w| \\w*"), vec![]);
    }

    #[test]
    fn escaped_bytes_pass_through_verbatim() {
        // `\|` keeps the scan going (scanner.rs) and stays in the value as
        // written: unescaping is never a reader's job.
        assert_eq!(pairs("\\w x|a\\|b|\\w*"), vec![("", "a\\|b")]);
        assert_eq!(pairs("\\w x|lemma=\"a\\|b\"\\w*"), vec![("lemma", "a\\|b")]);
        assert_eq!(
            pairs("\\w x|lemma=\"a\\\\b\"\\w*"),
            vec![("lemma", "a\\\\b")]
        );
    }

    #[test]
    fn spans_are_absolute_and_cover_exactly_the_bytes() {
        let source = "\\w x|lemma=\"grace\"\\w*";
        let events = walk(source);
        let AttrEvent::Attr(attr) = &events[0] else {
            panic!("{events:?}");
        };
        assert_eq!(attr.name_span, 5..10);
        assert_eq!(attr.value_span, 12..17);
        assert_eq!(&source.as_bytes()[5..10], b"lemma");
        assert_eq!(&source.as_bytes()[12..17], b"grace");

        // The bare form: an EMPTY name range sitting at the value's start.
        let source = "\\w In|in\\w*";
        let events = walk(source);
        let AttrEvent::Attr(attr) = &events[0] else {
            panic!("{events:?}");
        };
        assert_eq!(attr.name_span, 6..6);
        assert_eq!(attr.value_span, 6..8);

        // Unquoted pair value: the range is the value, no quotes to skip.
        let source = "\\w x|lemma=grace\\w*";
        let events = walk(source);
        let AttrEvent::Attr(attr) = &events[0] else {
            panic!("{events:?}");
        };
        assert_eq!(attr.name_span, 5..10);
        assert_eq!(attr.value_span, 11..16);
    }

    #[test]
    fn two_lists_on_one_node_are_two_independent_walks() {
        // "Later definition wins" is the CONSUMER's merge, never ours: each
        // list reads on its own and both are reported.
        let source = "\\w |Fred|Jésus|Jesus\\w*";
        let tokens = lex(source);
        let read: Vec<Vec<(&str, &str)>> = tokens
            .iter()
            .filter(|token| token.kind() == TokenKind::AttrList)
            .map(|list| {
                attrs(source.as_bytes(), list)
                    .map(|event| match event {
                        AttrEvent::Attr(attr) => (
                            core::str::from_utf8(attr.name).unwrap(),
                            core::str::from_utf8(attr.value).unwrap(),
                        ),
                        other => panic!("{other:?}"),
                    })
                    .collect()
            })
            .collect();
        assert_eq!(read, vec![vec![("", "Fred")], vec![("", "Jesus")]]);
    }
}
