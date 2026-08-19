//! Findings over an already-built document: `lex → cst::build → lint`.
//!
//! Phases 1-4 ship the STRUCTURAL, ORDERING, PAYLOAD, FORM and ADJACENCY
//! families, the SHAPE-ONLY half of Attributes, and the FIX model. Still owed,
//! each waiting on a piece that does not exist yet: the two attribute rules that
//! need a k/v interpreter, and the Version family — see
//! planning/lint-sketch.md "Build phases".
//!
//! Three laws shape everything here:
//!
//! - **Lint reads verdicts, it never re-derives them.** [`CloseReason`] is the
//!   walker's judgement about how a frame ended; this module matches on it and
//!   asks the row only "did that row want a closer" — the exact predicate the
//!   walker used at pop time ([`wants_closer`]), never a second notion of it.
//! - **Flag, never repair.** No token is reordered, inserted or dropped
//!   (the editor session's token→span→UTF-16 mapping depends on it), and no
//!   text is rewritten. Phase 4's [`Fix`]es are *offered* byte edits: the rule
//!   governs TOKENS, and a fix is proposed TEXT that nothing here applies. Once
//!   a user accepts one the bytes are real and the next re-lex is honest.
//! - **No strings, anywhere.** An [`Observation`] is four u32s. Everything a
//!   message needs textually is already a span reachable through
//!   `anchor`/`second`; everything else is a small integer in `aux`, whose
//!   meaning per code is the [`LintRow::aux`] column. That is what lets a
//!   report cross wasm as one flat `[code, anchor, second, aux] × n` array.
//!
//! Fixes ride the passes that find things, computed BESIDE the finding and
//! never in a pass of their own: 14 of the 37 codes declare a
//! [`LintRow::fix_label`], and a code emits a fix if and only if its row does
//! (asserted both ways in tests). Every one is a byte splice — insert the ending
//! the author left out, delete an orphan, write the expected number, upper-case
//! three bytes — and every one is proved by [`check_fixes`], the oracle, over
//! all 226 corpus books. Where a repair would be a MOVE or a guess it is not
//! offered: `attr-trailing-form-deprecated` (relocating an attribute list is an
//! interpretation of the content it jumps), `book-code-unknown` (which
//! identifier was meant is not mechanical), the gap codes, and a renumber whose
//! own successor would collide with it.
//!
//! Shape of the pass, since 2026-08-19: **lint is ONE in-order walk of the CST
//! feeding four state machines.** A short bounded prologue over the header
//! ([`header_scan`]) reads the two whole-file facts the machines need, and then
//! [`walk`] visits every node open, every leaf token in document order, and
//! every node close exactly once, handing each event to [`Structure`] (close
//! verdicts, orphan closers), [`Ancestry`] (sidebar containment, paragraph-less
//! verse runs), [`Ordering`] (the chapter/verse sequence) and [`Flat`] (the
//! row-lookup, form, payload, adjacency and attribute rules). All four write
//! through one [`Emit`] sink and the findings are sorted once at the end, so
//! report order is a property of the report and not of the traversal.
//!
//! The four sweeps this replaced — a node sweep, a token sweep, a tree walk and
//! an ordering sweep — each paid the same ~2.5 ns/token of dispatch before any
//! rule ran, and the tree walk is a strict superset of a flat token sweep: the
//! lifted partition oracle says `cst.in_order()` recovers `0..tokens.len()`, so
//! one walk delivers every token in document order PLUS the ancestry its own
//! stack carries PLUS the node boundaries. The machines are plain feedable
//! structs — explicit state, event methods, no assumption that a slice of
//! tokens exists — because the next driver is the CST Builder itself
//! (planning/investigate-later.md, "Single-pass pipeline").
//!
//! PERF (measured 2026-08-19, `playground --lint-only`, min-of-8 with the two
//! binaries run ALTERNATELY in one window, so trust the deltas over the
//! absolutes; en_ulb is small enough that its numbers need `--iters 30`):
//!
//! | corpus | four passes | one walk |
//! |--------|-------------|----------|
//! | en_ult (6.57M tokens, 1.76M nodes) | 12.1 ns/token | **9.1** |
//! | en_ulb (255k tokens)               | 12.8 ns/token | **8.4** |
//!
//! Split by machine on en_ult, by disabling each in turn: the bare walk ~3.6,
//! `Flat` ~3.1, `Structure` ~1.4, `Ordering` ~0.6, `Ancestry` ~0.3. Four things
//! bought the 3 ns, in order of size:
//!
//! - **The node-close fast out** (~0.5). 1.73M of en_ult's 1.76M nodes close
//!   `Explicit` and have no verdict to report, so the shape question — which
//!   reads the row and peeks at the next node — is asked only of the rest.
//! - **The current frame in locals** (~0.8), with only the ancestors in the vec:
//!   `stack.last_mut()` on every iteration was a load and a bounds check on the
//!   hottest line in lint.
//! - **[`Frame::scratch`]** (~1.0): the two ancestry bits computed at open and
//!   handed back at close, instead of re-reading the row.
//! - **Orphan closers judged at node close only** (~0.7): see [`Structure`].
//!   This also deleted the `tokens.len()` consumed-closer bitset and the
//!   `nodes.len()` container-end one, which the staged passes needed because
//!   the fact was produced in one sweep and read in another.
//!
//! `Flat` is now the biggest single line, and it is the same rule bodies as
//! before — the two Form rules read the source byte on either side of every
//! opening marker, the attribute machine carries four fields across the walk,
//! and `numbering-mix` zeroes two 153-entry arrays per document. Cutting it
//! further means changing what the rules DO, which is a different exercise.
//!
//! The module map, one file per piece of that shape: [`rows`] is the authored
//! data (the codes and [`LINT_ROWS`], the sibling of `tables::rows`), [`walk`]
//! is the driver ([`Doc`], the [`Emit`] sink, THE WALK), [`structure`],
//! [`ancestry`], [`ordering`] and [`flat`] are one machine each, and [`fix`]
//! holds lint's half of the fix model — the offered repair, the sequence
//! fixes and [`check_fixes`]. The byte-splice primitives those are built from
//! ([`Edit`], [`FixStr`], [`apply`]) are `crate::edit`, because the formatter
//! and the diff port speak the same vocabulary. This file keeps the layer a
//! caller sees: [`lint`], [`Observation`], [`LintReport`], [`header_scan`] and
//! the missing-`\id` end check.
//!
//! [`CloseReason`]: crate::cst::CloseReason

pub(crate) mod ancestry;
pub(crate) mod fix;
pub(crate) mod flat;
pub(crate) mod ordering;
pub(crate) mod rows;
pub(crate) mod structure;
pub(crate) mod walk;

#[cfg(test)]
mod tests;

pub use crate::edit::{Edit, FixStr, apply};
pub use fix::{Fix, check_fixes};
pub use rows::{AuxKind, Category, Code, LINT_ROWS, LintRow, Severity, UsfmVersion};

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
/// TOKEN indices into the linted slice (per-build, like `marker_idx`), and
/// `second` is [`NO_TOKEN`] when the finding has only one party.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    pub code: Code,
    pub anchor: u32,
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
    /// The `\id` line's BookCode token index. `None` IS the missing-`\id`
    /// state (real in the wild: BSB Ecclesiastes) — never a crash, and in
    /// phase 1 never an [`Observation`] either: the `missing-id` code is
    /// phase 2's, and until then this field carries the fact by itself.
    pub book: Option<u32>,
    /// The version the `\usfm` line declares, `None` when there is no such
    /// line (the corpus majority) or its payload is not a version.
    ///
    /// Phase 3 gives the [`LintRow::escalation`] column the fact it has always
    /// needed: a consumer maps a finding's severity through that column using
    /// THIS value, and one rule (`attr-trailing-form-deprecated`) uses it as a
    /// gate rather than a dial, because "deprecated" is a claim about a
    /// declared version and is false without one.
    pub declared_version: Option<UsfmVersion>,
    /// Sorted by `anchor`, then by code — one document order for consumers,
    /// independent of which internal pass produced a finding.
    pub observations: Vec<Observation>,
    /// PARALLEL to `observations`: the [`Fix`] index each finding offers, or
    /// [`NO_FIX`]. A side table rather than a fifth field on
    /// [`Observation`] — the four-u32 shape is what lets a report cross wasm as
    /// one flat array, most findings offer no fix at all, and every consumer
    /// that only wants to LIST findings never touches this vec. Read it through
    /// [`Self::fix`].
    pub fix_of: Vec<u32>,
    /// Every offered repair, in no particular order — reached through
    /// `fix_of`, never scanned.
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

// ---------------------------------------------------------------------------
// The pass
// ---------------------------------------------------------------------------

/// Lints one already-lexed, already-built document.
///
/// INVARIANT: `lint` never reorders, inserts or drops tokens, and never
/// touches `source` bytes destructively — the caller's `tokens` slice is the
/// same slice afterwards. The editor session's token→span→UTF-16 mapping is
/// built on that.
///
/// `source` is read through spans the scanner already carved — plus, since
/// phase 3, the single byte on either side of an opening marker's span, which
/// is where the Form family's whole evidence lives. The structural rules still
/// ask only the CST and the marker table.
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

    // A file with no `\id` at all. Raised here rather than in a pass because
    // the fact is the ABSENCE of a token, which no sweep can see, and because
    // `book` is the state it reads. Markers-only guard: a plain-prose or empty
    // buffer is not a book that owes an `\id`.
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
/// BOUNDED AT THE FIRST `\c`, and that bound is the point: both markers live in
/// the book header, so sweeping 6.5M tokens for a `\usfm` line that three of
/// the four corpora do not have would cost more than every rule that reads it.
/// Nothing after the first chapter can be an `\id` or a `\usfm` line — and a
/// file that puts one there has a structural finding already, not a header.
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
            // `\usfm` carves no payload (the scanner leaves the version string
            // as ordinary Text, isolated by its line ending), so the fact is
            // read off the ADJACENT token — the same adjacency shape the
            // `ca`/`cp` rules use, and the reason scanner.rs carves nothing.
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
