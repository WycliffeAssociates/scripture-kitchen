//! `analyze`: one pass, nine flat u32 reads, every offset in UTF-16.
//!
//! ```text
//! \id GEN
//! \c 1
//! \p \v 1 In the beginning\f + \ft note\f* .
//!
//! let a = analyze(text, wants::ALL, None);
//!
//! a.chapters       [0, MAX, 0, 0, MAX, 0, 8]    chapter 0 (front matter), no `\c`
//!                  [1|SHAPED, 8, 11, 12, 12, 8, 56]  `\c ` at 8, label "1", content at 12
//! a.blocks         [PARA|FRONT|META, 0, 4, 8]   the `\id` line
//!                  [PARA, 13, 16, 56]           `\p ` opens at 13, content at 16
//! a.lines          [PARA|FRONT|META, 0, 4, 7]   line 1 is `\id GEN`
//!                  [CHAPTER_VERSE, 8, 12, 12]   line 2 is `\c 1`
//!                  [PARA, 13, 16, 55]           line 3 opens with `\p `
//! a.verse_anchors  [1|SHAPED, 16, 19, 20, 21]   `\v ` at 16, "1" at 19..20
//! a.note_extents   [FOOTNOTE, 37, 53]           the whole `\f … \f*`
//! a.note_parts     [0, CALLER, 40, 41]          the `+`
//!                  [0, MARKUP, 42, 46]          the `\ft `
//!                  [0, BODY, 46, 50]            "note"
//!                  [0, MARKUP, 50, 53]          the `\f*`
//! a.text_runs      [21, 37]                     reader text, markup removed
//! a.token_spans    [PARA | MARKER<<16, 13, 16]  …one row per token
//! a.diagnostics    [code, from, to, MAX, MAX, aux, fix] …
//! a.usfm_version   MAX                          no `\usfm` line
//! ```
//!
//! The editor holds the document already, so nothing here ships a STRING: a
//! marker name, a chapter label, a diagnostic message are all `sliceString` over
//! spans it is given. The one exception is the fix text, which is bytes the
//! engine invented and the document does not contain.
//!
//! # The LF contract
//!
//! **Emitted UTF-16 offsets assume LF-normalized input.** CodeMirror counts a
//! line break as ONE position (it normalizes to LF on the way in); a literal
//! `\r\n` in the text passed here is TWO UTF-16 code units, so every offset
//! after the first CRLF would be one ahead of the editor's own. The editor
//! vision canonicalizes at ingress (§6.3), so LF-in is the CONTRACT, not
//! something this function repairs — it stays byte-honest and converts what it
//! was given. Native callers reading source bytes are unaffected.
//!
//! # The delimiter is ONE byte
//!
//! The scanner folds a marker's or payload's WHOLE horizontal-whitespace run
//! into that token. The reads do not: every `content_from` is the label's end
//! plus a SINGLE delimiter byte (nothing at all when no run followed), so
//!
//! ```text
//! \v 1  Put     number 11..12   content_from 13   ← byte 13 is CONTENT
//! \p    text    marker 5..7     content_from 8    ← bytes 8..11 are CONTENT
//! ```
//!
//! `[label_end, content_from)` is the delimiter, derivable, one byte or empty.
//! Everything past it is leading content whitespace: visible, editable, and the
//! formatter's to trim. Emitting the whole run instead made a space typed at
//! `content_from` vanish into chrome on the next analysis, and contradicted
//! format's own delimiter rule over the same bytes.
//!
//! # What is computed, and what is copied
//!
//! [`wants`] gates both: an unset bit skips the artifact it needs (no CST when
//! only chapters are asked for, no lint, no mask) and emits an empty read.
//! `clip` bounds the two TOKEN-GRANULARITY reads to a viewport — analysis stays
//! whole-book, because a diagnostic's evidence is not local.

use core::ops::Range;

use crate::cst::{Cst, ROOT_TOKEN};
use crate::lint::{self, LintReport, NO_FIX, NO_TOKEN};
use crate::mask::{Filter, mask};
use crate::tables::generated::{self, MarkerIdx};
use crate::tables::schema::{Category, MarkerKind};
use crate::toc::{Toc, toc};
use crate::utf16::Cursor;
use crate::{Token, TokenKind, lex};

// ---------------------------------------------------------------------------
// The wire schema
// ---------------------------------------------------------------------------

/// One bit per read. Nothing is computed or copied for an unset bit.
///
/// The numbers are WIRE DATA — the TS wrapper mirrors them, and the two are
/// versioned together in the same package.
pub mod wants {
    pub const CHAPTERS: u32 = 1 << 0;
    pub const BLOCKS: u32 = 1 << 1;
    /// `note_extents` AND `note_parts` — the inside of a note is meaningless
    /// without the extent it belongs to, so they share one bit (the same
    /// argument diagnostics and fixes share theirs).
    pub const NOTE_EXTENTS: u32 = 1 << 2;
    pub const TOKEN_SPANS: u32 = 1 << 3;
    pub const TEXT_RUNS: u32 = 1 << 4;
    pub const VERSE_ANCHORS: u32 = 1 << 5;
    /// Findings AND their fixes — a fix is worthless without the finding that
    /// offers it, so they share one bit.
    pub const DIAGNOSTICS: u32 = 1 << 6;
    /// One row per MARKED LINE — the read an editor's line rules read. Needs no
    /// CST: it is a token walk.
    pub const LINES: u32 = 1 << 7;

    pub const ALL: u32 = CHAPTERS
        | BLOCKS
        | NOTE_EXTENTS
        | TOKEN_SPANS
        | TEXT_RUNS
        | VERSE_ANCHORS
        | DIAGNOSTICS
        | LINES;
}

/// u32s per entry, per read. The TS wrapper carries the same numbers.
pub mod stride {
    pub const CHAPTERS: usize = 7;
    pub const BLOCKS: usize = 4;
    pub const LINES: usize = 4;
    pub const NOTE_EXTENTS: usize = 3;
    pub const NOTE_PARTS: usize = 4;
    pub const TOKEN_SPANS: usize = 3;
    pub const TEXT_RUNS: usize = 2;
    pub const VERSE_ANCHORS: usize = 5;
    pub const DIAGNOSTICS: usize = 7;
    pub const FIXES: usize = 2;
    pub const FIX_EDITS: usize = 2;
}

/// "No such offset / no such index" in any read. Same sentinel everywhere so a
/// decoder needs one rule.
pub const NONE: u32 = u32::MAX;

/// The coarse rendering class, three bits, one per shape decision an editor
/// makes without knowing a marker's name.
///
/// A match over the codegen table's [`MarkerKind`] and [`Category`] — never an
/// authored marker list, which is the whole point: `\pi3` is Para because its
/// ROW says Paragraph, and a new spec marker classifies itself.
///
/// # A word, not a byte
///
/// The low BYTE is what it always was — coarse class in bits 0..2, the five
/// shape flags above it — and every constant below bit 8 is unchanged. Bit 8
/// ([`META`]) is the ninth fact, so the class is a `u16` now. It rides in a
/// whole `u32` slot in `blocks`, `lines` and `token_spans`; only
/// `token_spans`' companion field moved (`kind_bits << 16`, not `<< 8`).
pub mod class {
    /// Mask for the 3-bit coarse class.
    pub const MASK: u16 = 0b111;

    pub const OTHER: u16 = 0;
    pub const PARA: u16 = 1;
    pub const CHAR: u16 = 2;
    pub const NOTE: u16 = 3;
    pub const MILESTONE: u16 = 4;
    /// `\c`, `\v`, and their alternates. The `chapters` and `verse_anchors`
    /// reads are how a consumer tells the two apart — this class is the shape
    /// decision ("a numbered marker with a designator slot"), not the identity.
    pub const CHAPTER_VERSE: u16 = 5;
    pub const SIDEBAR: u16 = 6;
    pub const TABLE: u16 = 7;

    // ---- flags, bits 3..8 ----

    /// A title or section paragraph — `\mt`, `\ms`, `\s`, `\r`, `\d`, `\sp`,
    /// `\cd`, `\cl`. The CM probe's HEADING regex, answered from the table.
    pub const HEADING: u16 = 1 << 3;
    /// Identification, introductions, peripherals, `\id`/`\usfm` — the CM
    /// probe's FRONT regex, answered from the table.
    pub const FRONT: u16 = 1 << 4;
    /// Poetry and lists: the block classes an editor indents.
    pub const POETRY: u16 = 1 << 5;
    /// A closing spelling — `\x*`, `\*`, a `-e` milestone.
    pub const CLOSER: u16 = 1 << 6;
    /// The marker resolved to no row: a `\z` extension, `\s5`, a typo. The
    /// coarse class is [`OTHER`] and the editor must not assume a shape.
    pub const UNKNOWN: u16 = 1 << 7;
    /// MACHINE metadata — `\id`, `\usfm`, `\ide`, `\rem`, `\sts`, `\h`,
    /// `\toc1-3`. Always accompanies [`FRONT`], and splits it: FRONT alone is
    /// introduction prose the reader sees (`\ip`, `\iot`, `\is`, `\imt`) and
    /// FRONT|META is the header strip an editor dims or hides.
    ///
    /// The two categories that decide it — `ParaIdentification` and
    /// `DocumentStructure` — were already in the table; the byte was collapsing
    /// them.
    pub const META: u16 = 1 << 8;
}

/// Flags packed into the `chapter`/`number` field of the two designator reads.
///
/// A designator is POSITIONAL — the scanner hands back whatever follows `\v` —
/// so after a number is deleted the next word becomes the designator (`\v  Then
/// He declared` designates "Then"). An editor that trusts the span styles
/// scripture as a verse number, so the reads carry the verdict the designator
/// interpreter already reached.
pub mod anchor {
    /// The designator matches the spec's pattern for this marker
    /// ([`crate::designator::verse`] / [`crate::designator::chapter`]). CLEAR
    /// is the conservative answer: absent, malformed, or not a number at all.
    pub const NUMBER_SHAPED: u32 = 1 << 31;
    /// The chapter/number the flag rides on.
    pub const NUMBER_MASK: u32 = !NUMBER_SHAPED;
}

/// What one row of the `note_parts` read describes.
///
/// The parts PARTITION a note extent: every token inside one is exactly one
/// part, so an apparatus that renders ORIGIN + BODY and freezes CALLER + MARKUP
/// has accounted for the whole note.
pub mod note_part {
    /// The caller after `\f` — `+`, `-`, `?`, or a custom run. Delimiter
    /// trimmed, the same label span a designator's is.
    pub const CALLER: u32 = 0;
    /// Reader text inside the origin reference (`\fr`, `\xo`) — "1:5".
    pub const ORIGIN: u32 = 1;
    /// Reader text anywhere else in the note — the wording an apparatus edits.
    pub const BODY: u32 = 2;
    /// Every marker token inside the note EXCEPT its own opener: `\fr`, `\ft`,
    /// a nested `\+w`, an attribute list, the `\f*`. The chrome an editor
    /// freezes — the marker plus ONE delimiter byte, never the whole folded
    /// run, so `\ft   note` freezes `\ft ` and the two spaces left are BODY.
    pub const MARKUP: u32 = 3;
}

/// The note families the `note_extents` read names.
pub mod note_family {
    pub const FOOTNOTE: u32 = 0;
    /// `\fe` — an endnote.
    pub const ENDNOTE: u32 = 1;
    /// `\ef` — a study footnote.
    pub const EXTENDED_FOOTNOTE: u32 = 2;
    pub const CROSS_REFERENCE: u32 = 3;
    /// `\ex` — an extended cross reference.
    pub const EXTENDED_CROSS_REFERENCE: u32 = 4;
    /// A note row the five spellings above do not name.
    pub const OTHER: u32 = 5;
}

/// Everything one `analyze` call produced. Every `Vec<u32>` is a flat,
/// fixed-stride record array (see [`stride`]); every offset in one is UTF-16.
///
/// An unwanted read is EMPTY, never absent — a decoder over `chapters` reads
/// zero entries whether the bit was unset or the book has no `\c`, and neither
/// is an error.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Analysis {
    /// The whole document's UTF-16 length — the clamp every other offset is
    /// under, and free (the sweep ends there).
    pub len_utf16: u32,
    /// The version the `\usfm` line declares, as an index into
    /// [`UsfmVersion`](crate::lint::UsfmVersion)'s ladder (`0` = 3.0, `1` = 3.2,
    /// `2` = 4.0). [`NONE`] when the document declares none — the state that
    /// GATES several diagnostics outright, which is why it is a field and not a
    /// consumer's regex over the first 512 bytes.
    ///
    /// Always computed: the scan is bounded at the first `\c`.
    pub usfm_version: u32,
    /// `[number, marker_from, label_from, label_to, content_from, from, to]`.
    /// Row 0 is always chapter 0, the front matter, and the rows TILE the
    /// document. `number` carries [`anchor::NUMBER_SHAPED`]; an absent or
    /// malformed designator gives `number == 0` with the flag clear, and the
    /// raw spelling is still the label span's bytes.
    ///
    /// `marker_from` and `content_from` are [`NONE`] on row 0 alone, which no
    /// `\c` opens. Everywhere else `marker_from..content_from` is the chrome an
    /// editor hides and `content_from` is where the caret goes — one delimiter
    /// byte past the label, or at the label's end when nothing followed it.
    ///
    /// `label_to..content_from` is therefore the DELIMITER, and it is exactly
    /// one byte or empty (see [the emit rule](content_after)).
    pub chapters: Vec<u32>,
    /// `[class, from, content_from, to]` per paragraph-level block —
    /// paragraphs, headers, peripherals, table rows, sidebars. `content_from`
    /// is the opening marker's name plus ONE delimiter byte, so
    /// `from..content_from` is the chrome an editor hides and `\p    text`
    /// leaves three spaces of visible, editable leading whitespace.
    pub blocks: Vec<u32>,
    /// `[class, from, content_from, to]` per MARKED LINE — a line whose first
    /// non-whitespace token is an opening marker. `to` is the line's end, the
    /// newline excluded; `content_from` is past the marker AND its designator,
    /// plus that one delimiter byte.
    ///
    /// Blocks span many lines and a line is what every editing rule reads, so
    /// this is not a slice of `blocks`: `\q1 \v 5 …` is one line inside a
    /// paragraph extent, and a `\s5` opens neither.
    pub lines: Vec<u32>,
    /// `[family, from, to]` per note, the whole `\f … \f*` including its
    /// closer. See [`note_family`].
    pub note_extents: Vec<u32>,
    /// `[note, kind, from, to]` per part of a note's interior — `note` indexes
    /// `note_extents`, `kind` is a [`note_part`]. The parts partition the
    /// extent in document order.
    pub note_parts: Vec<u32>,
    /// `[class | kind_bits << 16, from, to]` per token — the syntax-highlight
    /// read, and the one `clip` exists for. `kind_bits` is
    /// [`TokenKind::to_bits`].
    pub token_spans: Vec<u32>,
    /// `[from, to]` per maximal run of reader-visible text
    /// ([`Filter::reader_text`]), markup removed.
    pub text_runs: Vec<u32>,
    /// `[chapter, marker_from, number_from, number_to, content_from]` per `\v`.
    /// The chapter is the enclosing row's NUMBER, carrying
    /// [`anchor::NUMBER_SHAPED`].
    ///
    /// `marker_from..number_from` and `number_to..content_from` are the two
    /// chrome runs an editor hides; `content_from` is where the caret goes. An
    /// ABSENT designator reports an EMPTY number span AT `content_from` — the
    /// propped-open slot, which is where a retyped number will land, not the
    /// marker's start.
    ///
    /// `number_to..content_from` is the DELIMITER: exactly one byte, or empty
    /// when the line ends right after the number. Never the whole whitespace
    /// run — see [the emit rule](content_after).
    pub verse_anchors: Vec<u32>,
    /// `[code, from, to, second_from, second_to, aux, fix]`. `code` indexes the
    /// codegen'd diagnostics side-table (`onion-wasm/diagnostics.json`); the second
    /// span and the fix index are [`NONE`] when absent.
    pub diagnostics: Vec<u32>,
    /// `[edit_from, edit_to]` per offered fix — a half-open range of
    /// [`Self::fix_edits`] rows. A diagnostic's `fix` field indexes THIS.
    pub fixes: Vec<u32>,
    /// `[from, to]` per fix edit, in apply order. Equal ends is an insertion.
    pub fix_edits: Vec<u32>,
    /// PARALLEL to `fix_edits`: each edit's byte length in
    /// [`Self::fix_text`]. Zero is a pure deletion.
    pub fix_lens: Vec<u32>,
    /// Every fix edit's inserted text, concatenated. Engine-generated ASCII, so
    /// a JS consumer slices it with plain indices and needs no `TextEncoder`.
    pub fix_text: String,
}

// ---------------------------------------------------------------------------
// The pass
// ---------------------------------------------------------------------------

/// Lexes, builds, lints and indexes `text`, then writes the wanted reads.
///
/// `clip` is a UTF-16 range and bounds ONLY `token_spans` and `text_runs` (a
/// token or run is kept when it overlaps the clip at all). Everything else is
/// whole-book: a chapter table with a hole in it is not a chapter table, and a
/// finding's evidence is regularly outside the viewport that shows it.
///
/// Borrows stay inside: the tokens, the CST, the mask and the report are all
/// dropped here, and what comes back owns only numbers.
pub fn analyze(text: &str, wants: u32, clip: Option<Range<u32>>) -> Analysis {
    let source = text.as_bytes();
    let mut out = Analysis::default();
    let mut cursor = Cursor::new(source);
    out.len_utf16 = cursor.len_utf16();

    let tokens = lex(text);

    // Bounded at the first `\c`, so this is free even at wants == 0 — and it
    // is the gate several diagnostics are silent under, which no read could
    // carry per finding.
    out.usfm_version = match lint::header_scan(source, &tokens).1 {
        Some(version) => version as u32,
        None => NONE,
    };

    // The CST is the expensive shared artifact: three reads need it, and the
    // mask walks it.
    let needs_cst =
        wants & (wants::BLOCKS | wants::NOTE_EXTENTS | wants::TEXT_RUNS | wants::DIAGNOSTICS) != 0;
    let cst = needs_cst.then(|| crate::cst::build(&tokens));
    let toc = (wants & (wants::CHAPTERS | wants::VERSE_ANCHORS) != 0).then(|| toc(source, &tokens));

    let clip = clip.map(|range| {
        cursor.reset();
        let from = cursor.to_byte(range.start);
        let to = cursor.to_byte(range.end);
        from..to
    });

    if let Some(toc) = &toc {
        if wants & wants::CHAPTERS != 0 {
            out.chapters = chapters(source, &tokens, toc);
        }
        if wants & wants::VERSE_ANCHORS != 0 {
            out.verse_anchors = verse_anchors(source, &tokens, toc);
        }
    }
    if let Some(cst) = &cst {
        if wants & wants::BLOCKS != 0 {
            out.blocks = blocks(source, &tokens, cst);
        }
        if wants & wants::NOTE_EXTENTS != 0 {
            let (extents, parts) = notes(source, &tokens, cst);
            out.note_extents = extents;
            out.note_parts = parts;
        }
        if wants & wants::TEXT_RUNS != 0 {
            let mask = mask(source, &tokens, cst, &Filter::reader_text());
            out.text_runs = text_runs(&mask.ranges, clip.as_ref());
        }
        if wants & wants::DIAGNOSTICS != 0 {
            diagnostics(source, &tokens, &lint::lint(source, &tokens, cst), &mut out);
        }
    }
    if wants & wants::TOKEN_SPANS != 0 {
        out.token_spans = token_spans(&tokens, clip.as_ref());
    }
    if wants & wants::LINES != 0 {
        out.lines = lines(source, &tokens);
    }

    // The one wall: every emitted byte offset becomes a UTF-16 offset, one
    // sweep per read.
    let convert = |values: &mut Vec<u32>, stride: usize, fields: &[usize]| {
        to_utf16(source, values, stride, fields);
    };
    convert(&mut out.chapters, stride::CHAPTERS, &[1, 2, 3, 4, 5, 6]);
    convert(&mut out.blocks, stride::BLOCKS, &[1, 2, 3]);
    convert(&mut out.lines, stride::LINES, &[1, 2, 3]);
    convert(&mut out.note_extents, stride::NOTE_EXTENTS, &[1, 2]);
    convert(&mut out.note_parts, stride::NOTE_PARTS, &[2, 3]);
    convert(&mut out.token_spans, stride::TOKEN_SPANS, &[1, 2]);
    convert(&mut out.text_runs, stride::TEXT_RUNS, &[0, 1]);
    convert(&mut out.verse_anchors, stride::VERSE_ANCHORS, &[1, 2, 3, 4]);
    convert(&mut out.diagnostics, stride::DIAGNOSTICS, &[1, 2, 3, 4]);
    convert(&mut out.fix_edits, stride::FIX_EDITS, &[0, 1]);
    out
}

// ---------------------------------------------------------------------------
// The reads, in source bytes
// ---------------------------------------------------------------------------

/// Where a token's CONTENT begins: its payload label's end plus ONE delimiter
/// byte when the scanner folded a horizontal-whitespace run onto it, the label's
/// end when it folded nothing.
///
/// The scanner's fold gives a marker or payload token its WHOLE run — that is
/// the token contract and it is not touched. This is the EMIT rule, and it is
/// the reason `[label_end, content_from)` is exactly one byte or empty: bytes
/// past that one delimiter are leading CONTENT whitespace, visible and editable,
/// which is what keeps a space the author types at `content_from` from being
/// absorbed into chrome by the next analysis.
fn content_after(source: &[u8], token: &Token) -> u32 {
    let label = token.start + trimmed(source, token);
    if label < token.end() {
        label + 1
    } else {
        label
    }
}

/// The three offsets a designator slot has, from the marker's token row.
///
/// ```text
/// \v 1 In the…    marker 16..19   label 19..20   content 21
/// \v 1  In the…   marker 16..19   label 19..20   content 21 — the 2nd space is content
/// \v 1\ttext      marker …        label …        content past the TAB, one byte
/// \v 1\n          marker …        label 19..20   content 20 — no delimiter exists
/// \v \n           marker 48..51   label 51..51   content 51  ← the propped-open slot
/// ```
///
/// The label is the designator MINUS the whole folded run; `content` is the
/// label's end plus the single delimiter byte ([`content_after`]). With no
/// designator both collapse onto the MARKER's own content boundary, so an absent
/// slot is placeable instead of reported at the marker's start.
struct Slot {
    label: Range<u32>,
    content: u32,
    shaped: bool,
}

fn slot(
    source: &[u8],
    tokens: &[Token],
    marker_row: u32,
    read: fn(&[u8]) -> crate::designator::Designator,
) -> Slot {
    let Some(row) = crate::toc::designator_row(tokens, marker_row as usize) else {
        let at = content_after(source, &tokens[marker_row as usize]);
        return Slot {
            label: at..at,
            content: at,
            shaped: false,
        };
    };
    let token = tokens[row];
    let span = &source[token.start as usize..token.end() as usize];
    let label = crate::designator::label(span).len() as u32;
    Slot {
        label: token.start..token.start + label,
        content: content_after(source, &token),
        shaped: read(span).range().is_some(),
    }
}

/// `number | NUMBER_SHAPED` — the flag rides the number's high bit, which no
/// real chapter or verse can reach (both saturate into a u16).
fn numbered(number: u16, shaped: bool) -> u32 {
    u32::from(number) | if shaped { anchor::NUMBER_SHAPED } else { 0 }
}

fn chapters(source: &[u8], tokens: &[Token], toc: &Toc) -> Vec<u32> {
    let mut out = Vec::with_capacity(toc.chapters.len() * stride::CHAPTERS);
    for row in &toc.chapters {
        // Row 0 is the front matter: no `\c` opens it, so it has no marker and
        // no content boundary, and NONE says exactly that.
        let (marker_from, label, content, shaped) = match row.token {
            u32::MAX => (NONE, row.start..row.start, NONE, false),
            token => {
                let slot = slot(source, tokens, token, crate::designator::chapter);
                (
                    tokens[token as usize].start,
                    slot.label,
                    slot.content,
                    slot.shaped,
                )
            }
        };
        out.extend_from_slice(&[
            numbered(row.number, shaped),
            marker_from,
            label.start,
            label.end,
            content,
            row.start,
            row.end,
        ]);
    }
    out
}

fn verse_anchors(source: &[u8], tokens: &[Token], toc: &Toc) -> Vec<u32> {
    let mut out = Vec::with_capacity(toc.verses.len() * stride::VERSE_ANCHORS);
    for row in &toc.verses {
        let slot = slot(source, tokens, row.token, crate::designator::verse);
        out.extend_from_slice(&[
            numbered(row.chapter, slot.shaped),
            row.at,
            slot.label.start,
            slot.label.end,
            slot.content,
        ]);
    }
    out
}

/// One row per MARKED LINE: a line whose first non-whitespace token is an
/// OPENING marker.
///
/// ```text
/// \p \v 1 text        marked — `\p `, content at the `\v`
///   \q1 poetry        marked — an indented marker is still the line's opener
/// text continued      not marked — no row
/// \w word\w*          marked — CHAR class; a consumer decides that is content
/// \f* orphan          not marked — a CLOSER opens nothing
/// ```
///
/// A token walk, not a CST walk: blocks span many lines, and the thing every
/// editing rule reads is the line.
fn lines(source: &[u8], tokens: &[Token]) -> Vec<u32> {
    let mut out = Vec::new();
    // The marked line being accumulated: its class, start and content boundary.
    // Its `to` is only known at the newline, so the row is finished there.
    let mut open: Option<(u16, u32, u32)> = None;
    let mut from = 0u32;
    let mut looking = true;

    let finish = |out: &mut Vec<u32>, open: &mut Option<(u16, u32, u32)>, to: u32| {
        if let Some((class, start, content)) = open.take() {
            out.extend_from_slice(&[u32::from(class), start, content.min(to), to]);
        }
    };

    for (row, token) in tokens.iter().enumerate() {
        if token.kind() == TokenKind::Newline {
            finish(&mut out, &mut open, token.start);
            from = token.end();
            looking = true;
            continue;
        }
        if !looking {
            continue;
        }
        // Leading whitespace does not decide a line; the first token with bytes
        // in it does, and then nothing later on the line can change the answer.
        if token.kind() == TokenKind::Text
            && source[token.start as usize..token.end() as usize]
                .iter()
                .all(u8::is_ascii_whitespace)
        {
            continue;
        }
        looking = false;
        if matches!(token.kind(), TokenKind::Marker { .. }) {
            let content = crate::toc::designator_row(tokens, row).map_or_else(
                || content_after(source, token),
                |slot| content_after(source, &tokens[slot]),
            );
            open = Some((class_word(token), from, content));
        }
    }
    finish(&mut out, &mut open, source.len() as u32);
    out
}

/// Node ids ascend with their opening markers, so this is document order by
/// `from` — with NESTING (a `\p` inside an `\esb`), which is why the offset
/// conversion sorts rather than assuming.
fn blocks(source: &[u8], tokens: &[Token], cst: &Cst) -> Vec<u32> {
    let mut out = Vec::new();
    for (node, row) in cst.nodes.iter().enumerate() {
        if row.token == ROOT_TOKEN {
            continue;
        }
        let token = &tokens[row.token as usize];
        if !block_level(generated::kind(token.marker_idx)) {
            continue;
        }
        let extent = cst.extent(node as u32, tokens);
        out.extend_from_slice(&[
            u32::from(class_word(token)),
            extent.start,
            content_after(source, token),
            extent.end,
        ]);
    }
    out
}

/// The kinds that own a LINE of the document rather than a span inside one.
fn block_level(kind: MarkerKind) -> bool {
    matches!(
        kind,
        MarkerKind::Paragraph
            | MarkerKind::Header
            | MarkerKind::Periph
            | MarkerKind::TableRow
            | MarkerKind::Sidebar
    )
}

/// The note extents and, in the same walk, what is INSIDE each one.
///
/// ```text
/// \f + \fr 1:5 \ft Or comprehended\f*
///    ^caller
///      ^^^^ markup   ^^^^ markup    ^^^ markup
///           ^^^ origin    ^^^^^^^^^^^^^^^ body
/// ```
///
/// The parts partition the extent, so an apparatus renders ORIGIN and BODY and
/// freezes CALLER and MARKUP without reading a byte of USFM itself.
fn notes(source: &[u8], tokens: &[Token], cst: &Cst) -> (Vec<u32>, Vec<u32>) {
    let mut extents = Vec::new();
    let mut parts = Vec::new();
    for (node, row) in cst.nodes.iter().enumerate() {
        if row.token == ROOT_TOKEN {
            continue;
        }
        let token = &tokens[row.token as usize];
        if generated::kind(token.marker_idx) != MarkerKind::Note {
            continue;
        }
        let index = (extents.len() / stride::NOTE_EXTENTS) as u32;
        let extent = cst.extent(node as u32, tokens);
        extents.extend_from_slice(&[family(token.marker_idx), extent.start, extent.end]);
        note_parts(source, tokens, cst, node as u32, index, &mut parts);
    }
    (extents, parts)
}

/// `\fr` and `\xo` — the origin reference, the one note-internal marker whose
/// content is a REFERENCE rather than wording.
///
/// Resolved through the table by name, the way [`family`] resolves the five
/// note spellings: no `Category` separates an origin from a `\ft`, so this is
/// the honest place for the fact rather than a regex in every consumer.
fn is_origin(idx: MarkerIdx) -> bool {
    use crate::tables::schema::SpellingShape::Any;
    idx != generated::UNRESOLVED
        && (idx == generated::marker_idx(b"fr", Any) || idx == generated::marker_idx(b"xo", Any))
}

/// One note's interior, in document order, merging adjacent text into runs.
fn note_parts(
    source: &[u8],
    tokens: &[Token],
    cst: &Cst,
    node: u32,
    index: u32,
    out: &mut Vec<u32>,
) {
    let opener = cst.nodes[node as usize].token;
    // The open text run: adjacent text of one kind is ONE part, so a body that
    // wraps a line comes back whole instead of one row per token.
    let mut run: Option<(u32, Range<u32>)> = None;

    for (token_id, origin) in leaves(tokens, cst, node) {
        if token_id == opener {
            continue;
        }
        let token = &tokens[token_id as usize];
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
        // Chrome owns its LABEL plus the single delimiter byte, same rule as
        // `content_from`: a caller is the `+` alone (a `+` rendered with a
        // trailing space is not a caller), a markup token is the marker and one
        // space. Whatever the scanner folded on beyond that is leading content
        // whitespace, and it joins the text run that follows.
        let (span, tail) = match kind {
            note_part::ORIGIN | note_part::BODY => (token.start..token.end(), 0..0),
            _ => {
                let after = content_after(source, token);
                let label = match kind {
                    note_part::CALLER => token.start + trimmed(source, token),
                    _ => after,
                };
                (token.start..label, after..token.end())
            }
        };
        if span.start >= span.end {
            continue;
        }
        match kind {
            note_part::ORIGIN | note_part::BODY => match &mut run {
                Some((open, at)) if *open == kind && at.end == span.start => at.end = span.end,
                _ => {
                    if let Some((open, at)) = run.replace((kind, span)) {
                        out.extend_from_slice(&[index, open, at.start, at.end]);
                    }
                }
            },
            _ => {
                if let Some((open, at)) = run.take() {
                    out.extend_from_slice(&[index, open, at.start, at.end]);
                }
                out.extend_from_slice(&[index, kind, span.start, span.end]);
                if tail.start < tail.end {
                    let kind = if origin {
                        note_part::ORIGIN
                    } else {
                        note_part::BODY
                    };
                    run = Some((kind, tail));
                }
            }
        }
    }
    if let Some((kind, span)) = run {
        out.extend_from_slice(&[index, kind, span.start, span.end]);
    }
}

/// One subtree's leaf tokens in document order, each tagged with whether it
/// sits inside an origin reference ([`is_origin`]).
///
/// [`Cst::in_order_of`] would give the same tokens and lose that flag, which is
/// the one fact a note's interior needs and the tree carries.
fn leaves(tokens: &[Token], cst: &Cst, node: u32) -> Vec<(u32, bool)> {
    let mut out = Vec::new();
    let mut stack = vec![(cst.nodes[node as usize].children.clone(), false)];
    while let Some((range, origin)) = stack.last_mut() {
        if range.start == range.end {
            stack.pop();
            continue;
        }
        let child = cst.child_ids[range.start as usize];
        range.start += 1;
        let origin = *origin;
        if child & crate::cst::NODE_ID_BIT == 0 {
            out.push((child, origin));
            continue;
        }
        let child = child & !crate::cst::NODE_ID_BIT;
        let row = &cst.nodes[child as usize];
        let inside =
            origin || (row.token != ROOT_TOKEN && is_origin(tokens[row.token as usize].marker_idx));
        stack.push((row.children.clone(), inside));
    }
    out
}

/// A token's span minus the trailing whitespace the scanner folded onto it.
/// Never empties the span — a token that is only whitespace keeps its bytes.
fn trimmed(source: &[u8], token: &Token) -> u32 {
    let span = &source[token.start as usize..token.end() as usize];
    let label = crate::scanner::payload_label(span);
    if label.is_empty() {
        token.len as u32
    } else {
        label.len() as u32
    }
}

fn token_spans(tokens: &[Token], clip: Option<&Range<u32>>) -> Vec<u32> {
    let mut out = Vec::with_capacity(tokens.len() * stride::TOKEN_SPANS);
    for token in tokens {
        if !overlaps(token.start, token.end(), clip) {
            continue;
        }
        let packed = u32::from(class_word(token)) | u32::from(token.kind_bits) << 16;
        out.extend_from_slice(&[packed, token.start, token.end()]);
    }
    out
}

fn text_runs(ranges: &[Range<u32>], clip: Option<&Range<u32>>) -> Vec<u32> {
    let mut out = Vec::with_capacity(ranges.len() * stride::TEXT_RUNS);
    for run in ranges {
        if overlaps(run.start, run.end, clip) {
            out.extend_from_slice(&[run.start, run.end]);
        }
    }
    out
}

/// A span survives a clip if it OVERLAPS it — a token straddling the viewport
/// edge is half-drawn otherwise. An empty span at the edge counts as inside.
fn overlaps(from: u32, to: u32, clip: Option<&Range<u32>>) -> bool {
    match clip {
        None => true,
        Some(clip) => to >= clip.start && from <= clip.end,
    }
}

/// The findings, their fixes, and the fixes' text — one walk, because the fix
/// arrays are indexed by what the finding walk emits.
fn diagnostics(source: &[u8], tokens: &[Token], report: &LintReport, out: &mut Analysis) {
    // A finding's span is a TOKEN span, and a marker token carries the
    // delimiter the scanner folded onto it — so `{anchor}` rendered "\v 41 "
    // and the squiggle covered the space after the marker. Trimmed here, at
    // emit, because every consumer wanted the same trim.
    let span = |token: u32| -> (u32, u32) {
        match tokens.get(token as usize) {
            Some(token) => (token.start, token.start + trimmed(source, token)),
            // The one anchor that can name no token: `missing-id` on a
            // document whose tokens are all text.
            None => (source.len() as u32, source.len() as u32),
        }
    };

    out.diagnostics
        .reserve(report.observations.len() * stride::DIAGNOSTICS);
    for (index, obs) in report.observations.iter().enumerate() {
        let (from, to) = span(obs.anchor);
        let (second_from, second_to) = if obs.second == NO_TOKEN {
            (NONE, NONE)
        } else {
            span(obs.second)
        };
        let fix = report.fix_of.get(index).copied().unwrap_or(NO_FIX);
        out.diagnostics.extend_from_slice(&[
            obs.code as u32,
            from,
            to,
            second_from,
            second_to,
            obs.aux,
            if fix == NO_FIX { NONE } else { fix },
        ]);
    }

    // Fixes cross EAGERLY: the shape format_edits proved at a thousand times
    // the volume — spans, one concatenated ASCII blob, one length per edit.
    for fix in &report.fixes {
        let start = (out.fix_edits.len() / stride::FIX_EDITS) as u32;
        for edit in report.edits(fix) {
            out.fix_edits.extend_from_slice(&[edit.from, edit.to]);
            out.fix_lens.push(edit.insert.as_bytes().len() as u32);
            out.fix_text.push_str(edit.insert.as_str());
        }
        let end = (out.fix_edits.len() / stride::FIX_EDITS) as u32;
        out.fixes.extend_from_slice(&[start, end]);
    }
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// One token's packed class word — the coarse class in the low three bits, the
/// shape flags above it (see [`class`]).
pub fn class_word(token: &Token) -> u16 {
    let idx = token.marker_idx;
    let mut byte = match generated::kind(idx) {
        MarkerKind::Paragraph | MarkerKind::Header => class::PARA,
        MarkerKind::Character | MarkerKind::Figure => class::CHAR,
        MarkerKind::Note => class::NOTE,
        MarkerKind::Milestone => class::MILESTONE,
        MarkerKind::Chapter | MarkerKind::Verse => class::CHAPTER_VERSE,
        MarkerKind::Sidebar => class::SIDEBAR,
        // A cell is a character marker structurally and a table cell visually;
        // the visual answer is what this byte is for.
        MarkerKind::TableRow | MarkerKind::TableCell => class::TABLE,
        MarkerKind::Periph | MarkerKind::Meta | MarkerKind::Unknown => class::OTHER,
    };
    byte |= match generated::category(idx) {
        Category::ParaTitlesSections => class::HEADING,
        // The two halves of FRONT. Identification and DocumentStructure are the
        // machine header an editor dims (`\id`, `\usfm`, `\h`, `\toc1`);
        // Introductions and Peripheral are prose the reader is meant to read.
        Category::ParaIdentification | Category::DocumentStructure => class::FRONT | class::META,
        Category::ParaIntroductions | Category::ParaPeripheral => class::FRONT,
        Category::ParaPoetry | Category::ParaLists => class::POETRY,
        _ => 0,
    };
    if matches!(
        token.kind(),
        TokenKind::ClosingMarker { .. }
            | TokenKind::MilestoneTerminator
            | TokenKind::Milestone { end: true }
    ) {
        byte |= class::CLOSER;
    }
    // Only a token that CLAIMS to be a marker can be an unknown one: text and
    // designators carry marker_idx 0 too, and they are not markers at all.
    if idx == generated::UNRESOLVED && is_marker(token.kind()) {
        byte |= class::UNKNOWN;
    }
    byte
}

fn is_marker(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Marker { .. } | TokenKind::ClosingMarker { .. } | TokenKind::Milestone { .. }
    )
}

/// Which of the five note spellings a row is — resolved THROUGH the table, so a
/// renamed row degrades to `OTHER` instead of mislabelling.
fn family(idx: MarkerIdx) -> u32 {
    use crate::tables::schema::SpellingShape::Any;
    let named = |name: &[u8]| generated::marker_idx(name, Any);
    match idx {
        _ if idx == named(b"f") => note_family::FOOTNOTE,
        _ if idx == named(b"fe") => note_family::ENDNOTE,
        _ if idx == named(b"ef") => note_family::EXTENDED_FOOTNOTE,
        _ if idx == named(b"x") => note_family::CROSS_REFERENCE,
        _ if idx == named(b"ex") => note_family::EXTENDED_CROSS_REFERENCE,
        _ => note_family::OTHER,
    }
}

// ---------------------------------------------------------------------------
// The UTF-16 wall
// ---------------------------------------------------------------------------

/// Rewrites the `fields` of every row as UTF-16 offsets, in one sweep.
///
/// The sweep needs its offsets in ASCENDING order, and most reads already are
/// (tokens tile the document, mask ranges are sorted) — so the ordered case is
/// checked in one pass and converted in place with no allocation. The reads
/// that are NOT ordered — a diagnostic's second span precedes its anchor, a
/// chapter's label sits inside its own span, a `\p` nests inside an `\esb` —
/// sort a position list first, which costs a few thousand u32s on the reads
/// small enough to have that shape.
///
/// [`NONE`] is not an offset and is left alone.
fn to_utf16(source: &[u8], values: &mut [u32], stride: usize, fields: &[usize]) {
    if values.is_empty() {
        return;
    }
    let positions =
        || (0..values.len() / stride).flat_map(|row| fields.iter().map(move |f| row * stride + f));

    let mut ascending = true;
    let mut previous = 0u32;
    for at in positions() {
        let value = values[at];
        if value == NONE || value < previous {
            ascending = false;
            break;
        }
        previous = value;
    }

    let mut cursor = Cursor::new(source);
    if ascending {
        for at in positions() {
            values[at] = cursor.to_utf16(values[at]);
        }
        return;
    }

    let mut order: Vec<usize> = positions().filter(|at| values[*at] != NONE).collect();
    order.sort_unstable_by_key(|at| values[*at]);
    for at in order {
        values[at] = cursor.to_utf16(values[at]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::{Code, LINT_ROWS, Severity};

    /// The module doc's book, and the row shapes it prints.
    const DOC: &str = "\\id GEN\n\\c 1\n\\p \\v 1 In the beginning\\f + \\ft note\\f* .\n";

    fn rows(values: &[u32], stride: usize) -> Vec<&[u32]> {
        values.chunks_exact(stride).collect()
    }

    /// `number | NUMBER_SHAPED`, spelled out where a test asserts a row.
    fn shaped(number: u32) -> u32 {
        number | anchor::NUMBER_SHAPED
    }

    #[test]
    fn the_module_doc_example() {
        let a = analyze(DOC, wants::ALL, None);
        let meta = u32::from(class::PARA | class::FRONT | class::META);
        assert_eq!(
            rows(&a.chapters, stride::CHAPTERS),
            vec![
                &[0, NONE, 0, 0, NONE, 0, 8],
                &[shaped(1), 8, 11, 12, 12, 8, 56]
            ]
        );
        assert_eq!(
            rows(&a.blocks, stride::BLOCKS),
            vec![&[meta, 0, 4, 8], &[u32::from(class::PARA), 13, 16, 56]]
        );
        assert_eq!(
            rows(&a.lines, stride::LINES),
            vec![
                &[meta, 0, 4, 7],
                &[u32::from(class::CHAPTER_VERSE), 8, 12, 12],
                &[u32::from(class::PARA), 13, 16, 55],
            ]
        );
        assert_eq!(
            rows(&a.verse_anchors, stride::VERSE_ANCHORS),
            vec![&[shaped(1), 16, 19, 20, 21]]
        );
        assert_eq!(
            rows(&a.note_extents, stride::NOTE_EXTENTS),
            vec![&[note_family::FOOTNOTE, 37, 53]]
        );
        assert_eq!(
            rows(&a.note_parts, stride::NOTE_PARTS),
            vec![
                &[0, note_part::CALLER, 40, 41],
                &[0, note_part::MARKUP, 42, 46],
                &[0, note_part::BODY, 46, 50],
                &[0, note_part::MARKUP, 50, 53],
            ]
        );
        assert_eq!(a.usfm_version, NONE, "no `\\usfm` line");
        // The document's own bytes, sliced by the spans it was handed back.
        assert_eq!(&DOC[11..12], "1");
        assert_eq!(&DOC[16..21], "\\v 1 ");
        assert_eq!(&DOC[37..53], "\\f + \\ft note\\f*");
        assert_eq!(&DOC[46..50], "note");
    }

    /// THE BUG this rule exists for, in the editor's own terms: an author with
    /// the caret at `content_from` types a space, and the byte they just typed
    /// must still be theirs afterwards.
    ///
    /// The scanner's fold gives the designator token the whole run, so emitting
    /// the token's end walked `content_from` forward with every keystroke and
    /// the editor — which hides `[number_to, content_from)` as chrome — hid the
    /// space. Silent, invisible document growth.
    #[test]
    fn typing_a_space_at_content_from_does_not_move_content_from() {
        let before = "\\c 1\n\\p \\v 1 Put\n";
        let a = analyze(before, wants::ALL, None);
        let row = rows(&a.verse_anchors, stride::VERSE_ANCHORS)[0];
        let (number_to, content_from) = (row[3], row[4]);
        assert_eq!(&before[row[2] as usize..number_to as usize], "1");
        assert_eq!(content_from, number_to + 1, "one delimiter byte");

        // The keystroke, applied exactly where the caret was.
        let mut after = String::from(&before[..content_from as usize]);
        after.push(' ');
        after.push_str(&before[content_from as usize..]);
        assert_eq!(after, "\\c 1\n\\p \\v 1  Put\n");

        let b = analyze(&after, wants::ALL, None);
        let moved = rows(&b.verse_anchors, stride::VERSE_ANCHORS)[0];
        assert_eq!(moved[4], content_from, "the chrome must not have grown");
        assert_eq!(&after[moved[4] as usize..moved[4] as usize + 1], " ");
        // …and a second keystroke lands next to the first, still in content.
        assert_eq!(&after[content_from as usize..], " Put\n");
    }

    /// The three cases the single-byte rule has to answer, all of them falling
    /// out of one rule rather than three.
    #[test]
    fn the_delimiter_is_one_byte_whatever_the_run_is() {
        let cases = [
            // A TAB is the delimiter byte; which whitespace it is does not
            // change the count. (format's delimiter-single normalizes it.)
            ("\\c 1\n\\p \\v 1\tPut\n", 12, 13, "\t"),
            // End of line: no horizontal delimiter EXISTS, so there is nothing
            // to step over and the slot is propped open at the number's end.
            ("\\c 1\n\\p \\v 1\nPut\n", 12, 12, ""),
            // Trailing whitespace and no content: one byte is the delimiter,
            // the rest is visible trailing whitespace format deletes.
            ("\\c 1\n\\p \\v 1   \n", 12, 13, " "),
            // The plain case, and the two-space case that is the bug.
            ("\\c 1\n\\p \\v 1 Put\n", 12, 13, " "),
            ("\\c 1\n\\p \\v 1  Put\n", 12, 13, " "),
        ];
        for (source, number_to, content_from, delimiter) in cases {
            let a = analyze(source, wants::VERSE_ANCHORS, None);
            let row = rows(&a.verse_anchors, stride::VERSE_ANCHORS)[0];
            assert_eq!((row[3], row[4]), (number_to, content_from), "{source:?}");
            assert_eq!(
                &source[row[3] as usize..row[4] as usize],
                delimiter,
                "{source:?}: the delimiter is derivable and at most one byte"
            );
        }
    }

    /// A block marker's own run, the `\p    text` case: three of those four
    /// spaces are the author's leading whitespace, not the editor's chrome.
    #[test]
    fn a_block_marker_owns_its_name_and_one_delimiter() {
        let source = "\\c 1\n\\p    text\n";
        let a = analyze(source, wants::BLOCKS | wants::LINES, None);
        let block = rows(&a.blocks, stride::BLOCKS)[0];
        assert_eq!(&source[block[1] as usize..block[2] as usize], "\\p ");
        assert_eq!(&source[block[2] as usize..block[3] as usize], "   text\n");
        // The line read agrees with the block read on the same bytes.
        let line = rows(&a.lines, stride::LINES)[1];
        assert_eq!(line[2], block[2]);
        // No designator at all, and the marker still yields exactly one byte.
        let bare = analyze("\\c 1\n\\p   \n\\p x\n", wants::BLOCKS, None);
        let first = rows(&bare.blocks, stride::BLOCKS)[0];
        assert_eq!(first[2], first[1] + 3, "`\\p` plus one space");
    }

    /// The same disease inside a note: MARKUP is chrome an apparatus freezes,
    /// so it may not swallow whitespace the author typed either.
    #[test]
    fn note_chrome_keeps_one_delimiter_and_hands_back_the_rest() {
        let source = "\\c 1\n\\p a\\f +   \\ft   note\\f*\n";
        let a = analyze(source, wants::NOTE_EXTENTS, None);
        let seen: Vec<(u32, &str)> = rows(&a.note_parts, stride::NOTE_PARTS)
            .iter()
            .map(|row| (row[1], &source[row[2] as usize..row[3] as usize]))
            .collect();
        assert_eq!(
            seen,
            vec![
                (note_part::CALLER, "+"),
                // Two of the caller's three spaces are content, not chrome.
                (note_part::BODY, "  "),
                (note_part::MARKUP, "\\ft "),
                (note_part::BODY, "  note"),
                (note_part::MARKUP, "\\f*"),
            ]
        );
    }

    /// Under the designator gate the flag's meaning is sharp: it says the
    /// INTERPRETER accepted a designator the scanner did carve. Prose after
    /// `\v ` carves none at all, so it reports as the absent slot.
    #[test]
    fn a_designator_that_is_not_number_shaped_says_so() {
        let source = "\\c 1\n\\p \\v 1 a\n\\v  Then He declared\n\\v 012 c\n\\v \n";
        let a = analyze(source, wants::VERSE_ANCHORS, None);
        let seen: Vec<(bool, &str)> = rows(&a.verse_anchors, stride::VERSE_ANCHORS)
            .iter()
            .map(|row| {
                (
                    row[0] & anchor::NUMBER_SHAPED != 0,
                    &source[row[2] as usize..row[3] as usize],
                )
            })
            .collect();
        assert_eq!(
            seen,
            vec![
                (true, "1"),
                // "Then" is Text — the same empty slot a bare `\v` reports.
                (false, ""),
                // Digit-start but malformed: the token exists, the flag is clear.
                (false, "012"),
                // Absent: an EMPTY span, and it sits at the propped-open slot.
                (false, ""),
            ]
        );
        let last = rows(&a.verse_anchors, stride::VERSE_ANCHORS)[3];
        assert_eq!(last[2], last[4], "the empty slot is AT content_from");
        assert_eq!(&source[last[1] as usize..last[4] as usize], "\\v ");
    }

    /// A `\c` label is author data, so `12b` keeps its bytes AND loses its flag.
    #[test]
    fn a_chapter_row_carries_its_marker_and_its_content_boundary() {
        let source = "\\c 12b\n\\p a\n\\c\n\\p b\n";
        let a = analyze(source, wants::CHAPTERS, None);
        let table = rows(&a.chapters, stride::CHAPTERS);
        assert_eq!(table[0], &[0, NONE, 0, 0, NONE, 0, 0], "front matter");
        assert_eq!(table[1][0], 0, "`12b` is not a chapter number");
        assert_eq!(&source[table[1][2] as usize..table[1][3] as usize], "12b");
        assert_eq!(
            &source[table[1][1] as usize..table[1][4] as usize],
            "\\c 12b"
        );
        // No designator at all: the label is empty AT the marker's end, which is
        // where a retyped number lands.
        assert_eq!(table[2][2], table[2][3]);
        assert_eq!(table[2][2], table[2][4]);
        assert_eq!(&source[table[2][1] as usize..table[2][4] as usize], "\\c");
    }

    /// The `\usfm` line is a whole-file fact several diagnostics are SILENT
    /// under, so it is a field rather than a consumer's regex.
    #[test]
    fn the_declared_version_is_reported() {
        assert_eq!(analyze("\\id GEN\n", 0, None).usfm_version, NONE);
        assert_eq!(analyze("\\id GEN\n\\usfm 3.0\n", 0, None).usfm_version, 0);
        assert_eq!(analyze("\\id GEN\n\\usfm 3.2\n", 0, None).usfm_version, 1);
        assert_eq!(analyze("\\id GEN\n\\usfm 4.0\n", 0, None).usfm_version, 2);
        // Not a version: undeclared, never a defaulted 3.0.
        assert_eq!(
            analyze("\\id GEN\n\\usfm three\n", 0, None).usfm_version,
            NONE
        );
    }

    #[test]
    fn an_unset_bit_emits_nothing_and_computes_nothing() {
        let a = analyze(DOC, wants::CHAPTERS, None);
        assert_eq!(a.chapters.len(), 2 * stride::CHAPTERS);
        assert!(a.blocks.is_empty());
        assert!(a.lines.is_empty());
        assert!(a.token_spans.is_empty());
        assert!(a.diagnostics.is_empty());
        assert!(a.text_runs.is_empty());
        assert!(a.note_parts.is_empty());
        assert_eq!(analyze(DOC, 0, None).len_utf16, DOC.len() as u32);
    }

    /// The line read is what deletes an editor's whole-book token projection,
    /// so what it does and does NOT call a line is the contract.
    #[test]
    fn a_marked_line_is_one_whose_first_token_is_an_opening_marker() {
        let source = "\\c 1\n\\q1 \\v 5 poetry\ncontinued\n  \\p indented\n\\f* orphan\n\\s5\n\n";
        let a = analyze(source, wants::LINES, None);
        let seen: Vec<(u16, &str, &str)> = rows(&a.lines, stride::LINES)
            .iter()
            .map(|row| {
                (
                    row[0] as u16,
                    &source[row[1] as usize..row[2] as usize],
                    &source[row[2] as usize..row[3] as usize],
                )
            })
            .collect();
        assert_eq!(
            seen,
            vec![
                (class::CHAPTER_VERSE, "\\c 1", ""),
                // The chrome runs past the DESIGNATOR: `\v 5 ` is the verse's
                // own chrome and belongs to the line that opens with `\q1 `.
                (class::PARA | class::POETRY, "\\q1 ", "\\v 5 poetry"),
                // "continued" is not marked, and gets no row at all.
                (class::PARA, "  \\p ", "indented"),
                // `\f*` closes something; it opens nothing, so no row.
                (class::OTHER | class::UNKNOWN, "\\s5", ""),
            ]
        );
    }

    /// The pass-7 ledger's open question, answered from the table: machine
    /// header versus introduction prose.
    #[test]
    fn meta_splits_front_without_an_authored_marker_list() {
        let source = "\\id GEN\n\\usfm 3.0\n\\h Genesis\n\\toc1 The First\n\\rem note\n\
                      \\imt Intro\n\\is Section\n\\ip prose\n\\iot Outline\n\\io1 item\n";
        let a = analyze(source, wants::LINES, None);
        let meta: Vec<bool> = rows(&a.lines, stride::LINES)
            .iter()
            .map(|row| row[0] as u16 & class::META != 0)
            .collect();
        assert_eq!(
            meta,
            vec![
                true, true, true, true, true, false, false, false, false, false
            ]
        );
        // FRONT still covers both halves, so an existing consumer is unmoved.
        for row in rows(&a.lines, stride::LINES) {
            assert!(row[0] as u16 & class::FRONT != 0);
        }
    }

    /// The apparatus's whole need: a note's caller, its origin, its wording,
    /// and the chrome between them.
    #[test]
    fn note_parts_partition_a_notes_interior() {
        let source = "\\c 1\n\\p \\v 1 a\\x - \\xo 1:5 \\xt See \\+nd Lord\\+nd*\\x*\n";
        let a = analyze(source, wants::NOTE_EXTENTS, None);
        let seen: Vec<(u32, &str)> = rows(&a.note_parts, stride::NOTE_PARTS)
            .iter()
            .map(|row| (row[1], &source[row[2] as usize..row[3] as usize]))
            .collect();
        assert_eq!(
            seen,
            vec![
                (note_part::CALLER, "-"),
                (note_part::MARKUP, "\\xo "),
                (note_part::ORIGIN, "1:5 "),
                (note_part::MARKUP, "\\xt "),
                (note_part::BODY, "See "),
                (note_part::MARKUP, "\\+nd "),
                (note_part::BODY, "Lord"),
                (note_part::MARKUP, "\\+nd*"),
                (note_part::MARKUP, "\\x*"),
            ]
        );
        // The parts cover the extent from the opening marker's end onward.
        let extent = rows(&a.note_extents, stride::NOTE_EXTENTS)[0];
        let parts = rows(&a.note_parts, stride::NOTE_PARTS);
        assert!(parts[0][2] >= extent[1] && parts[parts.len() - 1][3] == extent[2]);
    }

    /// A note whose body wraps a line is ONE body part, not one per token.
    #[test]
    fn a_notes_body_is_one_run_across_a_line_break() {
        let source = "\\c 1\n\\p \\v 1 a\\f + \\ft first\nsecond\\f*\n";
        let a = analyze(source, wants::NOTE_EXTENTS, None);
        let body: Vec<&str> = rows(&a.note_parts, stride::NOTE_PARTS)
            .iter()
            .filter(|row| row[1] == note_part::BODY)
            .map(|row| &source[row[2] as usize..row[3] as usize])
            .collect();
        assert_eq!(body, vec!["first\nsecond"]);
    }

    /// The CM probe's three regexes, answered from the table.
    #[test]
    fn the_class_byte_answers_para_heading_and_front() {
        let source = "\\id GEN\n\\h Genesis\n\\toc1 The First Book\n\\mt1 Genesis\n\\s Section\n\
                      \\c 1\n\\p text\n\\q1 poetry\n\\li1 item\n\\s5\n\\esb\n\\p in a sidebar\n\\esbe\n";
        let a = analyze(source, wants::BLOCKS, None);
        let seen: Vec<u16> = rows(&a.blocks, stride::BLOCKS)
            .iter()
            .map(|row| row[0] as u16)
            .collect();
        let heading = class::PARA | class::HEADING;
        let front = class::PARA | class::FRONT | class::META;
        let poetry = class::PARA | class::POETRY;
        assert_eq!(
            seen,
            vec![
                front,   // \id
                front,   // \h
                front,   // \toc1
                heading, // \mt1
                heading, // \s
                class::PARA,
                poetry, // \q1
                poetry, // \li1
                // `\s5` resolves to no row, so it is not block-level and opens
                // no block — which is right: a uW chunk marker sits INSIDE a
                // paragraph. It reaches the editor through `token_spans`,
                // carrying OTHER | UNKNOWN.
                class::SIDEBAR,
                class::PARA, // nested inside the sidebar
            ]
        );
    }

    /// Nesting is the case the ascending sweep cannot serve: the sidebar opens
    /// before its paragraph and closes after it.
    #[test]
    fn a_nested_block_still_gets_exact_offsets() {
        let source = "\\esb\n\\p λόγος inside\n\\esbe\n";
        let a = analyze(source, wants::BLOCKS, None);
        let ix = crate::utf16_index(source.as_bytes());
        let blocks = rows(&a.blocks, stride::BLOCKS);
        assert_eq!(blocks.len(), 2);
        let sidebar = blocks[0];
        let para = blocks[1];
        assert!(sidebar[1] < para[1] && para[3] < sidebar[3], "nested");
        // The paragraph's own span, checked against the random-access index.
        let para_start = source.find("\\p ").unwrap() as u32;
        assert_eq!(para[1], ix.to_utf16(para_start));
        assert_eq!(para[2], ix.to_utf16(para_start + 3));
    }

    #[test]
    fn offsets_are_utf16_not_bytes() {
        // Devanagari: 3 bytes per character, so the two coordinates drift.
        let source = "\\c 1\n\\p \\v 1 अब्राहम की सन्तान\n\\v 2 tail\n";
        let a = analyze(source, wants::ALL, None);
        let ix = crate::utf16_index(source.as_bytes());
        for row in rows(&a.verse_anchors, stride::VERSE_ANCHORS) {
            assert!(row[1] < ix.len_utf16());
        }
        let second = rows(&a.verse_anchors, stride::VERSE_ANCHORS)[1];
        let byte = source.rfind("\\v 2").unwrap() as u32 + 3;
        assert_eq!(second[2], ix.to_utf16(byte));
        assert!(
            (second[2] as usize) < source[..byte as usize].len(),
            "UTF-16 offsets must be SMALLER than byte offsets on 3-byte script"
        );
        assert_eq!(a.len_utf16, ix.len_utf16());
    }

    #[test]
    fn a_clip_bounds_only_the_token_granularity_reads() {
        let source = "\\c 1\n\\p \\v 1 one\n\\c 2\n\\p \\v 1 two\n";
        let whole = analyze(source, wants::ALL, None);
        let clipped = analyze(source, wants::ALL, Some(0..8));
        assert_eq!(whole.chapters, clipped.chapters);
        assert_eq!(whole.verse_anchors, clipped.verse_anchors);
        assert_eq!(whole.blocks, clipped.blocks);
        assert!(clipped.token_spans.len() < whole.token_spans.len());
        assert!(clipped.text_runs.len() < whole.text_runs.len());
        // Everything kept overlaps the clip.
        for row in rows(&clipped.token_spans, stride::TOKEN_SPANS) {
            assert!(
                row[1] <= 8,
                "a kept token starts at or before the clip's end"
            );
        }
    }

    #[test]
    fn a_finding_carries_its_span_its_second_and_its_fix() {
        // An unclosed `\f`: one finding, one fix, one edit inserting `\f*`.
        let a = analyze(
            "\\c 1\n\\p \\v 1 a\\f + \\ft note\n",
            wants::DIAGNOSTICS,
            None,
        );
        let found = rows(&a.diagnostics, stride::DIAGNOSTICS);
        let note = found
            .iter()
            .find(|row| row[0] == Code::UnclosedAtEof as u32)
            .expect("the note left open at eof");
        assert!(note[1] < note[2], "a real span");
        let fix = note[6];
        assert_ne!(fix, NONE);
        let range = &a.fixes[fix as usize * stride::FIXES..][..stride::FIXES];
        assert_eq!(range[1] - range[0], 1, "one edit");
        let edit = range[0] as usize;
        assert_eq!(a.fix_lens[edit], 3);
        assert_eq!(&a.fix_text[..3], "\\f*");
        assert_eq!(
            a.fix_edits[edit * stride::FIX_EDITS],
            a.fix_edits[edit * stride::FIX_EDITS + 1],
            "an insertion"
        );
    }

    /// The Form channel is the formatter's, never a diagnostic — so no code in
    /// the `diagnostics` read may belong to a Form row.
    #[test]
    fn no_form_row_ever_reaches_the_diagnostics_read() {
        let sources = [
            DOC,
            "\\c 1\n\\p\n\\p   \\v 1  a  \r\n\\v 2\\f + x\n",
            "\\id gen\n\\c 2\n\\v 1 a\n\\c 1\n",
        ];
        for source in sources {
            let a = analyze(source, wants::DIAGNOSTICS, None);
            for row in rows(&a.diagnostics, stride::DIAGNOSTICS) {
                let lint_row = &LINT_ROWS[row[0] as usize];
                assert_ne!(
                    lint_row.severity,
                    Some(Severity::Form),
                    "{} is a Form row and must never be a diagnostic",
                    lint_row.name
                );
            }
        }
    }

    #[test]
    fn an_empty_document_answers_every_read() {
        let a = analyze("", wants::ALL, None);
        assert_eq!(a.len_utf16, 0);
        // The Toc's front-matter row always exists.
        assert_eq!(a.chapters, vec![0, NONE, 0, 0, NONE, 0, 0]);
        assert!(a.blocks.is_empty() && a.token_spans.is_empty() && a.diagnostics.is_empty());
    }
}
