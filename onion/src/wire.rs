//! One parse, one buffer.
//!
//! ```text
//! onion::parse(text, opts)        -> Parsed   native callers stop here
//! onion::wire::plate(&parsed)     -> Vec<u8>  the dish, for anything crossing
//! ```
//!
//! ```text
//! \id GEN
//! \c 1
//! \p \v 1 In the beginning.
//!
//! let parsed = parse(text, ParseOptions { toc: true, ..Default::default() });
//! let dish   = plate(&parsed);
//!
//! dish[0..4]    "ONWR"          magic
//! dish[4..8]    5               format version
//! dish[8..12]   10              section count
//! dish[12..16]  0               flags (bit 0 = offsets are UTF-16)
//! dish[16..20]  MAX             the declared `\usfm` version, or NONE
//! dish[20..24]  39              source length, in the dish's offset space
//! dish[24..32]  0x…             xxh3-64 of the source BYTES
//! dish[32..]    [off, len] x 10 the directory, then the sections
//! ```
//!
//! Nothing here casts a Rust struct: every field is written little-endian by
//! `wire::generated`, which is emitted from [`schema`] alongside the reader
//! that parses it. The two ends cannot disagree because neither is typed by
//! hand.
//!
//! A native caller has no reason to come here. [`Parsed`] holds the real engine
//! types and borrows the source; plating exists only to cross a boundary.

use crate::cst::Cst;
use crate::lint::{LintReport, NO_FIX, NO_TOKEN};
use crate::toc::Toc;
use crate::{Token, lex};

pub mod emit;
pub mod generated;
pub mod schema;

pub use schema::{FLAG_UTF16, FORMAT_VERSION, MAGIC, NONE, TOKEN_BLANK, TOKEN_DELIMITER_FOLDED};

/// What a parse should compute. Everything defaults OFF except the tree, which
/// is the product.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseOptions {
    /// Run the lint walk. The one expensive optional.
    pub diagnostics: bool,
    /// Build the chapter and verse index. Cheap, and needs no tree.
    pub toc: bool,
    /// Emit every offset as a UTF-16 code-unit offset instead of a byte one.
    ///
    /// Opt-in on purpose: bytes are what the engine holds, what a native
    /// consumer wants, and what sous reads. Only a JS editor counts in UTF-16.
    pub utf16: bool,
}

/// One parsed document, in engine types. Borrows the source it was parsed from.
pub struct Parsed<'a> {
    pub source: &'a str,
    pub tokens: Vec<Token>,
    pub cst: Cst,
    pub lint: Option<LintReport>,
    pub toc: Option<Toc>,
    /// The version the `\usfm` line declares, as a ladder index (0 = 3.0,
    /// 1 = 3.2, 2 = 4.0), or [`NONE`].
    ///
    /// DERIVED — set by [`Parsed::from_parts`], never by hand.
    ///
    /// Always computed: the scan is bounded at the first `\c`, so it is free
    /// even when nothing else is asked for. It has to cross unconditionally
    /// because it GATES diagnostics — `LintRow::severity_at` needs it before a
    /// single finding can be shown — and a consumer that had to find it would
    /// re-walk the header to recover what the engine already knew.
    pub usfm_version: u32,
    pub options: ParseOptions,
}

impl<'a> Parsed<'a> {
    /// Assemble a parse from ingredients a caller already holds — the door
    /// `galley`'s warmer comes through, having served most of them from cache.
    ///
    /// A constructor rather than a struct literal because `usfm_version` is
    /// DERIVED: it must be read from the same tokens, and a caller that filled
    /// it in itself could fill it in wrong. The rest of the coherence — that
    /// the tree was built from these tokens and they were lexed from this
    /// source — is still the caller's to keep; nothing can check it, which is
    /// exactly why the triple travels together.
    pub fn from_parts(
        source: &'a str,
        tokens: Vec<Token>,
        cst: Cst,
        lint: Option<LintReport>,
        toc: Option<Toc>,
        options: ParseOptions,
    ) -> Parsed<'a> {
        let usfm_version = match crate::lint::header_scan(source.as_bytes(), &tokens).1 {
            Some(version) => version as u32,
            None => NONE,
        };
        Parsed {
            source,
            tokens,
            cst,
            lint,
            toc,
            usfm_version,
            options,
        }
    }
}

/// Lex, build, and whatever `opts` asks for besides.
///
/// The tree is unconditional: it is the product, and every optional read either
/// needs it or is cheap enough not to gate.
pub fn parse(source: &str, opts: ParseOptions) -> Parsed<'_> {
    let tokens = lex(source);
    let cst = crate::cst::build(&tokens);
    let lint = opts
        .diagnostics
        .then(|| crate::lint::lint(source.as_bytes(), &tokens, &cst));
    let toc = opts
        .toc
        .then(|| crate::toc::toc(source.as_bytes(), &tokens));
    Parsed::from_parts(source, tokens, cst, lint, toc, opts)
}

// ---------------------------------------------------------------------------
// The wire-only rows
// ---------------------------------------------------------------------------

/// A finding as it crosses. `lint::Observation` names TOKENS and carries an
/// enum; this names spans and carries the enum's discriminant.
pub struct Diagnostic {
    pub code: u32,
    pub from: u32,
    pub to: u32,
    pub second_from: u32,
    pub second_to: u32,
    pub aux: u32,
    pub fix: u32,
}

/// A chapter as it crosses: the toc's row plus the designator's token index.
///
/// `toc` walks to the designator to read the number and keeps only the number
/// — its own comment calls `token` "the raw designator's way home". Since
/// `\c` opens no node, that walk is the ONLY route to it, and every consumer
/// was otherwise re-implementing `toc::designator_row`.
pub struct Chapter {
    pub start: u32,
    pub end: u32,
    pub token: u32,
    pub designator: u32,
    pub label_start: u32,
    pub label_end: u32,
    pub number: u32,
}

/// A verse anchor as it crosses. Same reason for `designator` as [`Chapter`].
pub struct Verse {
    pub at: u32,
    pub token: u32,
    pub designator: u32,
    pub label_start: u32,
    pub label_end: u32,
    pub members_from: u32,
    pub members_len: u32,
    pub chapter: u32,
    pub first: u32,
    pub last: u32,
}

/// One verse designator's member as it crosses — the Toc's own row.
pub type Member = crate::toc::VerseMember;

/// One repair's range in the edit arena.
pub struct Fix {
    pub edit_from: u32,
    pub edit_to: u32,
}

/// One edit, with its inserted bytes named ABSOLUTELY in the fix-text blob —
/// so no reader reconstructs a prefix sum to find them.
pub struct Edit {
    pub from: u32,
    pub to: u32,
    pub text_from: u32,
    pub text_len: u32,
}

/// The diagnostics half of a dish: findings, their repairs, and the blob.
struct Findings {
    diagnostics: Vec<Diagnostic>,
    fixes: Vec<Fix>,
    edits: Vec<Edit>,
    text: String,
}

impl Findings {
    fn empty() -> Self {
        Findings {
            diagnostics: Vec::new(),
            fixes: Vec::new(),
            edits: Vec::new(),
            text: String::new(),
        }
    }

    fn build(source: &[u8], tokens: &[Token], report: &LintReport) -> Self {
        // A finding's anchor is a TOKEN, and a marker token carries the one
        // delimiter byte the scanner folded onto it — trimmed here so every
        // consumer's squiggle stops at the marker rather than past it.
        let span = |token: u32| -> (u32, u32) {
            match tokens.get(token as usize) {
                Some(t) => (t.start, t.start + t.trimmed_len(source)),
                // The one anchor naming no token: `missing-id` where every
                // token is text.
                None => (source.len() as u32, source.len() as u32),
            }
        };

        let mut out = Findings::empty();
        out.diagnostics.reserve(report.observations.len());
        for (index, obs) in report.observations.iter().enumerate() {
            let (from, to) = span(obs.anchor);
            let (second_from, second_to) = if obs.second == NO_TOKEN {
                (NONE, NONE)
            } else {
                span(obs.second)
            };
            let fix = report.fix_of.get(index).copied().unwrap_or(NO_FIX);
            out.diagnostics.push(Diagnostic {
                code: obs.code as u32,
                from,
                to,
                second_from,
                second_to,
                aux: obs.aux,
                fix: if fix == NO_FIX { NONE } else { fix },
            });
        }

        for fix in &report.fixes {
            let edit_from = out.edits.len() as u32;
            for edit in report.edits(fix) {
                let text_from = out.text.len() as u32;
                out.text.push_str(edit.insert.as_str());
                out.edits.push(Edit {
                    from: edit.from,
                    to: edit.to,
                    text_from,
                    text_len: out.text.len() as u32 - text_from,
                });
            }
            out.fixes.push(Fix {
                edit_from,
                edit_to: out.edits.len() as u32,
            });
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Plating
// ---------------------------------------------------------------------------

/// Bytes of header before the section directory.
///
/// Eight words, all spoken for: magic, format version, section count, flags,
/// the declared `\usfm` version, the source's length, and its xxh3-64 across
/// the last two. The length is in the dish's own offset space; the hash is
/// always over the source BYTES, so two dishes of one document agree on it
/// whichever space they were plated in.
pub const HEADER_BYTES: usize = 32;
/// One directory entry: byte offset, byte length.
pub const DIRECTORY_ENTRY_BYTES: usize = 8;

/// Serialize a parse into one buffer.
///
/// Sections are written in [`schema::SECTIONS`] order and padded to a 4-byte
/// boundary, so a reader may take a `Uint32Array` view over any of them without
/// copying.
pub fn plate(parsed: &Parsed<'_>) -> Vec<u8> {
    let source = parsed.source.as_bytes();
    let findings = match &parsed.lint {
        Some(report) => Findings::build(source, &parsed.tokens, report),
        None => Findings::empty(),
    };
    let members: &[Member] = parsed.toc.as_ref().map_or(&[], |toc| &toc.members);
    let (chapters, verses) = match &parsed.toc {
        Some(toc) => (
            toc.chapters
                .iter()
                .map(|c| Chapter {
                    start: c.start,
                    end: c.end,
                    token: c.token,
                    designator: designator_of(&parsed.tokens, c.token),
                    label_start: c.label_start,
                    label_end: c.label_end,
                    number: u32::from(c.number),
                })
                .collect(),
            toc.verses
                .iter()
                .map(|v| Verse {
                    at: v.at,
                    token: v.token,
                    designator: designator_of(&parsed.tokens, v.token),
                    label_start: v.label_start,
                    label_end: v.label_end,
                    members_from: v.members_from,
                    members_len: u32::from(v.members_len),
                    chapter: u32::from(v.chapter),
                    first: u32::from(v.first),
                    last: u32::from(v.last),
                })
                .collect(),
        ),
        None => (Vec::new(), Vec::new()),
    };

    // Each section's bytes, plus every position in them holding a source
    // offset — gathered as we write so the UTF-16 pass is one sweep over the
    // whole dish rather than one per section.
    let mut sections: Vec<Vec<u8>> = Vec::with_capacity(schema::SECTIONS.len());
    let mut offsets: Vec<(usize, usize)> = Vec::new(); // (section, position)

    macro_rules! rows {
        ($record:expr, $rows:expr, $write:path $(, $context:expr)*) => {{
            let mut bytes = Vec::with_capacity($rows.len() * $record.stride());
            let mut positions = Vec::new();
            $write($rows $(, $context)*, &mut bytes, &mut positions);
            let at = sections.len();
            offsets.extend(positions.into_iter().map(|p| (at, p)));
            sections.push(bytes);
        }};
    }

    rows!(
        schema::TOKEN,
        &parsed.tokens,
        generated::write_tokens,
        source
    );
    rows!(schema::NODE, &parsed.cst.nodes, generated::write_nodes);
    {
        let mut bytes = Vec::with_capacity(parsed.cst.child_ids.len() * 4);
        for id in &parsed.cst.child_ids {
            bytes.extend_from_slice(&id.to_le_bytes());
        }
        sections.push(bytes);
    }
    rows!(
        schema::DIAGNOSTIC,
        &findings.diagnostics,
        generated::write_diagnostics
    );
    rows!(schema::FIX, &findings.fixes, generated::write_fixes);
    rows!(schema::EDIT, &findings.edits, generated::write_edits);
    sections.push(findings.text.into_bytes());
    rows!(schema::CHAPTER, &chapters, generated::write_chapters);
    rows!(schema::VERSE, &verses, generated::write_verses);
    rows!(schema::MEMBER, members, generated::write_members);

    debug_assert_eq!(sections.len(), schema::SECTIONS.len());

    if parsed.options.utf16 {
        to_utf16(source, &mut sections, &offsets);
    }

    let length = if parsed.options.utf16 {
        crate::utf16::utf16_len(source)
    } else {
        source.len() as u32
    };
    assemble(
        &sections,
        parsed.options.utf16,
        parsed.usfm_version,
        length,
        xxhash_rust::xxh3::xxh3_64(source),
    )
}

/// Every source offset in the dish, converted in one ascending sweep.
///
/// The values are lifted out ONCE, alongside where they came from, so the sort
/// key is an inline `u32` rather than a read back through two levels of `Vec`
/// — a comparator that re-reads costs one indirection per comparison, which is
/// `n log n` of them.
///
/// And the sort is skipped when the values already ascend, which is the common
/// case: tokens tile the document and are most of the offsets in a dish. Only
/// a section that walks backwards forces it — a diagnostic's `second` precedes
/// its anchor by construction.
fn to_utf16(source: &[u8], sections: &mut [Vec<u8>], offsets: &[(usize, usize)]) {
    if offsets.is_empty() {
        return;
    }
    let read = |sections: &[Vec<u8>], s: usize, p: usize| -> u32 {
        u32::from_le_bytes(sections[s][p..p + 4].try_into().expect("four bytes"))
    };

    // (value, section, position) — the value inline, so nothing is read twice.
    let mut order: Vec<(u32, u32, u32)> = Vec::with_capacity(offsets.len());
    let mut ascending = true;
    let mut previous = 0u32;
    for &(s, p) in offsets {
        let value = read(sections, s, p);
        if value == NONE {
            continue;
        }
        ascending &= value >= previous;
        previous = value;
        order.push((value, s as u32, p as u32));
    }
    if !ascending {
        order.sort_unstable_by_key(|entry| entry.0);
    }

    let runs = crate::utf16::Runs::new(source);
    macro_rules! convert {
        ($walk:expr) => {{
            let mut walk = $walk;
            for (value, section, at) in order {
                let (section, at) = (section as usize, at as usize);
                let converted = walk.to_utf16(value);
                sections[section][at..at + 4].copy_from_slice(&converted.to_le_bytes());
            }
        }};
    }
    match runs.as_ref() {
        Some(runs) => convert!(runs.walk()),
        None => convert!(crate::utf16::Cursor::new(source)),
    }
}

/// Header, directory, then the sections, each 4-aligned.
fn assemble(
    sections: &[Vec<u8>],
    utf16: bool,
    usfm_version: u32,
    source_length: u32,
    source_hash: u64,
) -> Vec<u8> {
    let directory = HEADER_BYTES + sections.len() * DIRECTORY_ENTRY_BYTES;
    let body: usize = sections.iter().map(|s| align4(s.len())).sum();
    let mut out = Vec::with_capacity(directory + body);

    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&(sections.len() as u32).to_le_bytes());
    out.extend_from_slice(&(if utf16 { FLAG_UTF16 } else { 0 }).to_le_bytes());
    out.extend_from_slice(&usfm_version.to_le_bytes());
    out.extend_from_slice(&source_length.to_le_bytes());
    out.extend_from_slice(&source_hash.to_le_bytes());

    let mut at = directory;
    for section in sections {
        out.extend_from_slice(&(at as u32).to_le_bytes());
        out.extend_from_slice(&(section.len() as u32).to_le_bytes());
        at += align4(section.len());
    }

    for section in sections {
        out.extend_from_slice(section);
        out.resize(align4(out.len()), 0);
    }
    out
}

/// The token index of the designator belonging to the marker at `token`, or
/// [`NONE`]. `u32::MAX` marks the front-matter chapter row, which has neither.
fn designator_of(tokens: &[Token], token: u32) -> u32 {
    if token == NONE {
        return NONE;
    }
    match crate::toc::designator_row(tokens, token as usize) {
        Some(row) => row as u32,
        None => NONE,
    }
}

const fn align4(n: usize) -> usize {
    (n + 3) & !3
}
