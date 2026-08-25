//! Findings over an already-built document: `lex → cst::build → lint`.
//!
//! 43 codes in six families ([`Category`]): STRUCTURE, ORDERING, ATTRIBUTES,
//! PAYLOAD, FORM and VERSION.
//!
//! Three laws shape everything here:
//!
//! - **Lint reads verdicts, it never re-derives them.** [`CloseReason`] is the
//!   walker's judgement about how a frame ended; this module matches on it and
//!   asks the row only "did that row want a closer" — the walker's own predicate
//!   at pop time, never a second notion of it.
//! - **Flag, never repair.** No token is reordered, inserted or dropped (the
//!   editor session's token→span→UTF-16 mapping depends on it). A [`Fix`] is an
//!   *offered* byte edit that nothing here applies.
//! - **No strings, anywhere.** An [`Observation`] is four u32s — two token
//!   indices and an `aux` whose meaning per code is the [`LintRow::aux`] column
//!   — so a report crosses wasm as one flat array.
//!
//! A code emits a fix if and only if its row declares a [`LintRow::fix_label`]
//! (15 of the 43, asserted both ways in tests), computed BESIDE the finding and
//! proved by [`check_fixes`] over all 226 corpus books. Where a repair would be
//! a MOVE or a guess it is not offered — relocating an attribute list, moving an
//! out-of-band marker, guessing which book identifier was meant, or renumbering
//! into a collision with the successor.
//!
//! **lint is ONE in-order walk of the CST** ([`walk`]) feeding four state
//! machines — [`Structure`], [`Ancestry`], [`Ordering`] and [`Flat`] — after
//! [`header_scan`] reads the two whole-file facts they need. All four write
//! through one [`Emit`] sink and the findings are sorted once at the end, so
//! report order is a property of the report and not of the traversal.
//!
//! **Cost** (min-of-8, `--lint-only`): ~9.4 ns/token on unaligned scripture
//! (en_ulb), ~15.5 ns/token on en_ult. The gap is the k/v attribute rules and
//! nothing else — en_ult is 31 MB of `\w` attribute interiors.
//!
//! [`CloseReason`]: crate::cst::CloseReason

pub(crate) mod ancestry;
pub(crate) mod attr_rules;
pub mod catalog;
pub(crate) mod fix;
pub(crate) mod flat;
pub(crate) mod ordering;
pub(crate) mod rows;
pub(crate) mod structure;
pub(crate) mod walk;

#[cfg(test)]
mod tests;

pub use crate::edit::{Edit, FixStr, apply, check_edits};
pub use catalog::diagnostics_json;
pub use fix::{Fix, check_fixes};
pub use rows::{
    AuxKind, Category, Code, LINT_ROWS, LintRow, Severity, UsfmVersion, VERSION_ROWS, VersionRow,
};

pub(crate) use ancestry::Ancestry;
pub(crate) use flat::Flat;
pub(crate) use ordering::Ordering;
pub(crate) use structure::Structure;
pub(crate) use walk::{Doc, Emit};

use crate::Token;
use crate::TokenKind;
use crate::cst::Cst;
use crate::tables::generated;
use crate::tables::schema::{MarkerKind, SpellingShape};
use walk::span_of;
use walk::walk;

/// `Observation::second` when there is no second party.
pub const NO_TOKEN: u32 = u32::MAX;

/// One finding. Four u32s, `Copy`, no allocation: `anchor` and `second` are
/// TOKEN indices, and `second` is [`NO_TOKEN`] when there is only one party.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    pub code: Code,
    pub anchor: u32,
    /// INVARIANT: `second == NO_TOKEN || second < anchor` — the other party (an
    /// opener, an owner, the previous in sequence, a first occurrence) always
    /// PRECEDES the anchor. Pinned over the corpus in tests/lint_corpus.rs.
    pub second: u32,
    pub aux: u32,
}

impl Observation {
    pub(crate) fn one(code: Code, anchor: u32) -> Self {
        Self {
            code,
            anchor,
            second: NO_TOKEN,
            aux: 0,
        }
    }

    fn pair(code: Code, anchor: u32, second: u32) -> Self {
        Self {
            code,
            anchor,
            second,
            aux: 0,
        }
    }
}

/// [`LintReport::fix_of`] when an observation offers no repair.
pub const NO_FIX: u32 = u32::MAX;

/// Everything one lint run learned about one document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LintReport {
    /// The `\id` line's BookCode token index. `None` IS the missing-`\id` state
    /// (real in the wild: BSB Ecclesiastes) — never a crash. The fact is kept
    /// here as well as reported as `missing-id`.
    pub book: Option<u32>,
    /// The version the `\usfm` line declares, `None` when there is no such line
    /// (the corpus majority) or its payload is not a version.
    ///
    /// A consumer maps severity through [`LintRow::severity_at`] with THIS
    /// value, and two rules (`attr-trailing-form-deprecated`,
    /// `deprecated-marker`) are GATED on it rather than merely escalated:
    /// "deprecated" is a claim about a declared version, false without one.
    pub declared_version: Option<UsfmVersion>,
    /// Sorted by `anchor`, then by code — one document order for consumers,
    /// independent of which internal pass produced a finding.
    pub observations: Vec<Observation>,
    /// PARALLEL to `observations`: the [`Fix`] index each finding offers, or
    /// [`NO_FIX`]. A side table rather than a fifth field on [`Observation`],
    /// whose four-u32 shape is what crosses wasm. Read through [`Self::fix`].
    pub fix_of: Vec<u32>,
    /// Every offered repair, in no particular order — reached through `fix_of`,
    /// never scanned.
    pub fixes: Vec<Fix>,
    /// The shared edit arena every [`Fix::edits`] range indexes. Flat, like
    /// `observations`: a wasm consumer reads `[from, to, len, bytes…]`.
    pub edit_list: Vec<Edit>,
}

impl LintReport {
    /// The repair offered for `observations[index]`, if any.
    pub fn fix(&self, index: usize) -> Option<&Fix> {
        match self.fix_of.get(index).copied() {
            Some(NO_FIX) | None => None,
            Some(fix) => Some(&self.fixes[fix as usize]),
        }
    }

    /// One fix's edits, in apply order (see [`Fix`]).
    pub fn edits(&self, fix: &Fix) -> &[Edit] {
        &self.edit_list[fix.edits.start as usize..fix.edits.end as usize]
    }
}

/// Lints one already-lexed, already-built document.
///
/// INVARIANT: never reorders, inserts or drops tokens, and never touches
/// `source` destructively — the editor session's token→span→UTF-16 mapping is
/// built on that.
///
/// `source` is read through spans the scanner already carved, plus the single
/// byte on either side of an opening marker's span, where the Form family's
/// whole evidence lives.
pub fn lint(source: &[u8], tokens: &[Token], cst: &Cst) -> LintReport {
    let (book, declared_version) = header_scan(source, tokens);
    let mut out = Emit::default();

    walk(
        &Doc {
            source,
            tokens,
            cst,
        },
        declared_version,
        &mut out,
    );

    // Raised here rather than in a pass because the fact is the ABSENCE of a
    // token, which no sweep can see. The markers-only guard: a plain-prose or
    // empty buffer is not a book that owes an `\id`.
    if book.is_none()
        && tokens.iter().any(|token| {
            matches!(
                token.kind(),
                TokenKind::Marker { .. }
                    | TokenKind::ClosingMarker { .. }
                    | TokenKind::Milestone { .. }
            )
        })
    {
        out.push(Observation::one(Code::MissingId, 0));
    }

    out.finish(book, declared_version)
}

/// The two header facts every later pass wants: the `\id` line's BookCode
/// token, and the version the `\usfm` line declares.
///
/// BOUNDED AT THE FIRST `\c`, and that bound is the point: sweeping 6.5M tokens
/// for a `\usfm` line three of the four corpora do not have would cost more than
/// every rule that reads it. A file that writes one below a chapter has a
/// structural finding already, not a header.
pub(crate) fn header_scan(source: &[u8], tokens: &[Token]) -> (Option<u32>, Option<UsfmVersion>) {
    let usfm = generated::marker_idx(b"usfm", SpellingShape::PlainOnly);
    let mut book = None;
    let mut version = None;
    let mut awaiting_version = false;
    for (idx, token) in tokens.iter().enumerate() {
        match token.kind() {
            TokenKind::BookCode => {
                book.get_or_insert(idx as u32);
                if version.is_some() {
                    break;
                }
            }
            // `\usfm` carves no payload — the scanner leaves the version as
            // ordinary Text, isolated by its line ending — so the fact is read
            // off the ADJACENT token, the `ca`/`cp` rules' shape.
            TokenKind::Text if awaiting_version => {
                version = parse_version(span_of(source, token));
                awaiting_version = false;
                if book.is_some() && version.is_some() {
                    break;
                }
            }
            TokenKind::Marker { .. } => {
                if generated::kind(token.marker_idx) == MarkerKind::Chapter {
                    break;
                }
                awaiting_version = token.marker_idx == usfm;
            }
            _ => awaiting_version = false,
        }
    }
    (book, version)
}

/// `3.0`, `3.2`, `4.0` → the ladder rung a rule keys on. Anything else is
/// `None`: an undeclared version is not a declaration of 3.0, and a rule that
/// escalates on one must not fire on the other.
fn parse_version(span: &[u8]) -> Option<UsfmVersion> {
    let mut parts = span.split(|b| *b == b'.');
    let number = |part: Option<&[u8]>| -> Option<u32> {
        let part = part?;
        (!part.is_empty() && part.iter().all(u8::is_ascii_digit))
            .then(|| part.iter().fold(0u32, |n, b| n * 10 + u32::from(b - b'0')))
    };
    let major = number(parts.next())?;
    let minor = number(parts.next()).unwrap_or(0);
    Some(match (major, minor) {
        (4.., _) => UsfmVersion::V4_0,
        (3, 2..) => UsfmVersion::V3_2,
        _ => UsfmVersion::V3_0,
    })
}
