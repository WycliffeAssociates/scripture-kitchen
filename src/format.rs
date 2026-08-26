//! `format`: the OPT-IN prettifier — one transaction of byte edits, no rewriter.
//!
//! ```text
//! source     \c 1\v 1  In  the beginning.\v 2 The earth…
//!
//! format(source, &FormatOptions::default())
//!            \c 1
//!            \p \v 1 In  the beginning. \v 2 The earth…
//!            ↑ every block marker owns  ↑ interior spacing is CONTENT
//!              its own line               and is never touched
//!
//! format(source, &FormatOptions { verse_breaks: VerseBreaks::Keep, ..d })
//!            \c 1
//!            \p
//!            \v 1 In  the beginning.
//!            \v 2 The earth…
//! ```
//!
//! Two internal passes and no duplicated rule logic: [`lint`] runs as it always
//! does and format keeps the fixes of the rows carrying [`LintRow::formatter`]
//! plus whatever [`FormatOptions::repairs`] names, and a second walk evaluates
//! the FORM CHANNEL — the [`Severity::Form`] rows, which lint never reaches.
//! The two edit sets merge, collide by ROW ORDER (first writer wins, the loser
//! is simply not emitted), and apply as one transaction.
//!
//! [`format_edits_in`] is that same whole-book transaction filtered to a byte
//! range — the scoped "format this chapter" button, with the straddle policy
//! written on the function.
//!
//! Format is the one part of the crate allowed to mutate, delete and invent
//! bytes; the mask's no-normalization law does not bind it. What it invents is
//! whitespace and engine `FixStr`s — never document prose.
//!
//! [`LintRow::formatter`]: crate::lint::LintRow::formatter
//! [`Severity::Form`]: crate::lint::Severity::Form

use crate::cst::Cst;
use crate::designator::{self, Designator};
use crate::edit::{Edit, FixStr, apply};
use crate::lint::{Code, lint};
use crate::scanner::payload_label;
use crate::tables::generated;
use crate::tables::schema::MarkerKind;
use crate::{Token, TokenKind, lex};

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

/// What happens to the line break in front of a `\v`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerseBreaks {
    /// `\v` joins the block-marker list: one verse per line. What a CodeMirror
    /// session rendering line breaks wants.
    Keep,
    /// The break in front of a `\v` inside a paragraph becomes a space, so a
    /// paragraph is one flowing line. The brief's default.
    #[default]
    Remove,
}

/// What happens to a line break sitting on a character-marker boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CharBreaks {
    /// The author's breaks are the author's.
    #[default]
    Keep,
    /// Rewrite them as spaces — the aligned-corpus shape, where every `\w` is
    /// written on its own line (en_ult). A pretty aligned VIEW is `format` with
    /// `Join` and then a mask; the mask itself never normalizes.
    Join,
}

/// The byte form of every line ending in the OUTPUT — inserted breaks use it and
/// existing endings are rewritten to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Newline {
    #[default]
    Lf,
    CrLf,
}

impl Newline {
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Lf => b"\n",
            Self::CrLf => b"\r\n",
        }
    }
}

/// Every knob, flat. No profile type: presets, if they ever exist, are
/// documented constructors over this struct.
///
/// The whitespace canon defaults ON — it is what "format" means — and the two
/// uW-era CONTENT rows default off, because each assumes an editorial intent
/// the bytes do not state.
#[derive(Debug, Clone)]
pub struct FormatOptions<'a> {
    // ---- axes ----
    pub verse_breaks: VerseBreaks,
    pub char_marker_breaks: CharBreaks,
    pub newline: Newline,
    // ---- parameterized rows ----
    /// Marker names (as written, no backslash) whose every occurrence is deleted
    /// outright — `["s5"]` for the unfoldingWord chunk marker. Empty by default,
    /// and an empty list yields zero findings.
    ///
    /// No safety rail: linting the OUTPUT is the rail. Format never promises the
    /// result is cleaner than what was asked for.
    pub remove_markers: &'a [&'a str],
    /// Lint codes whose EXISTING fixes join the transaction. Only codes whose
    /// repair is unambiguous belong here (`unclosed-note`, `unclosed-at-eof`);
    /// renumbering never does, at any tier.
    pub repairs: &'a [Code],
    // ---- per-rule switches ----
    pub block_marker_own_line: bool,
    pub collapse_blank_lines: bool,
    pub normalize_newlines: bool,
    pub trim_text_edges: bool,
    pub delimiter_single: bool,
    pub designator_ws_single: bool,
    pub marker_ws_at_line_start: bool,
    /// `\v 2 2 men went` → `\v 2 men went`. Off by default.
    pub dedupe_verse_number: bool,
    /// `\v 1\v 2\v 3 asdf` → `\v 1-3 asdf`. Off by default.
    pub bridge_empty_verses: bool,
}

impl Default for FormatOptions<'_> {
    fn default() -> Self {
        Self {
            verse_breaks: VerseBreaks::default(),
            char_marker_breaks: CharBreaks::default(),
            newline: Newline::default(),
            remove_markers: &[],
            repairs: &[],
            block_marker_own_line: true,
            collapse_blank_lines: true,
            normalize_newlines: true,
            trim_text_edges: true,
            delimiter_single: true,
            designator_ws_single: true,
            marker_ws_at_line_start: true,
            dedupe_verse_number: false,
            bridge_empty_verses: false,
        }
    }
}

// ---------------------------------------------------------------------------
// The public surface
// ---------------------------------------------------------------------------

/// The EDIT LIST: sorted by `from`, non-overlapping, in SOURCE byte offsets — an
/// editor session applies it through the byte↔UTF-16 shim it already owns.
///
/// Non-UTF-8 input yields no edits at all. Formatting bytes nobody can lex would
/// be guessing, and the contract is "a malformed document formats to a
/// malformed-but-tidier document, never an error".
pub fn format_edits(source: &[u8], opts: &FormatOptions) -> Vec<Edit> {
    let edits: Vec<Edit> = settle(source, opts)
        .into_iter()
        .map(|(_, _, edit)| edit)
        .collect();
    checked(source, edits)
}

/// The same transaction, RANGE-BOUNDED: only the edits whose bytes lie inside
/// `range` (byte offsets into `source`).
///
/// What a chapter-scoped "format this section" button wants. The analysis is
/// still WHOLE-BOOK — lint needs the book, and precedence between two rules that
/// want the same byte is settled over the whole document before anything is
/// filtered — so the ranged list is always a SUBSET of `format_edits`, never a
/// different set of proposals.
///
/// THE STRADDLE POLICY: an edit is kept iff its ENTIRE span lies within the
/// range; one that crosses a boundary is dropped WHOLE, never cut. Half an edit
/// corrupts — a splice that replaces `\n   \n` with one break cannot be honored
/// for the bytes on this side of the line only.
///
/// ```text
/// source   \c 1\n\p \v 1 a\n\n\n\v 2 b\n
///                        byte 14 ^^ 16
///
/// format_edits(source)            .. collapse-blank-lines 14..16, ""
/// format_edits_in(source, 15..24) .. gone: 14..16 reaches out of the window
/// format_edits_in(source, 14..24) .. kept: the span opens ON the edge
/// ```
///
/// ATOMICITY BEATS SCOPE: an edit group that only makes sense whole
/// (`bridge-empty-verses` writes the bridge AND deletes the empty `\v`s; a
/// chained lint fix) is kept iff EVERY edit of the group is inside. A group half
/// in the window is wholly absent.
///
/// BOUNDARY INSERTIONS ARE IN: a pure insertion (`from == to`) at `range.start`
/// or at `range.end` is inside — a caret at the window's edge is in the window.
/// The one rule `range.start <= from && to <= range.end` says both things.
///
/// IDEMPOTENCE IS IN-SCOPE ONLY, and only over a CLEAN boundary: apply the
/// ranged transaction, widen the range by its own byte delta, and a second
/// `format_edits_in` proposes nothing — provided no edit was dropped whose span
/// reached INTO the window. A chapter span is clean that way (its edges are
/// marker boundaries, so a straddler ends where the window begins). A window
/// whose edge cuts through a straddler keeps the bytes that edit would have
/// taken, so the next pass has real work: scoped formatting CONVERGES there
/// rather than settling in one, and never oscillates.
///
/// Either way, a straddling edit is still proposed by WHOLE-BOOK `format_edits`
/// afterward — dropping it did not fix those bytes, and scoped formatting never
/// promised it had.
///
/// No chapter-scoped entry point exists, and none is wanted: the caller already
/// holds the span it means, from [`Toc::chapters`] (`ChapterRow`'s `start..end`)
/// or the `chapters` read of [`analyze`], and hands it in here.
///
/// An inverted range yields nothing; a zero-width one keeps only pure insertions
/// at that point.
///
/// [`Toc::chapters`]: crate::toc::Toc::chapters
/// [`analyze`]: crate::analyze::analyze
pub fn format_edits_in(
    source: &[u8],
    range: core::ops::Range<u32>,
    opts: &FormatOptions,
) -> Vec<Edit> {
    if range.start > range.end {
        return Vec::new();
    }
    let settled = settle(source, opts);
    // Group-wise, and only AFTER precedence: a group is kept whole or not at all.
    let groups = settled.iter().map(|(_, group, _)| *group + 1).max();
    let mut keep = vec![true; groups.unwrap_or(0) as usize];
    for (_, group, edit) in &settled {
        if edit.from < range.start || edit.to > range.end {
            keep[*group as usize] = false;
        }
    }
    let edits: Vec<Edit> = settled
        .into_iter()
        .filter(|(_, group, _)| keep[*group as usize])
        .map(|(_, _, edit)| edit)
        .collect();
    checked(source, edits)
}

/// The same transaction, each edit beside the ROW that won its bytes. The tests
/// read the partition through this; `format_edits` is the same list with the
/// codes dropped.
#[cfg(test)]
pub(crate) fn format_claims(source: &[u8], opts: &FormatOptions) -> Vec<(Code, Edit)> {
    settle(source, opts)
        .into_iter()
        .map(|(code, _, edit)| (code, edit))
        .collect()
}

/// Both passes and the precedence rule, whole-book: `(row, group, edit)` in
/// document order. The group id is what makes an ATOMIC claim filterable
/// downstream without re-running precedence.
fn settle(source: &[u8], opts: &FormatOptions) -> Vec<(Code, u32, Edit)> {
    let Ok(text) = core::str::from_utf8(source) else {
        return Vec::new();
    };
    let tokens = lex(text);
    let cst = crate::cst::build(&tokens);
    let mut claims = Claims::default();
    let paragraphs = harvest(source, &tokens, &cst, opts, &mut claims);
    form_pass(source, &tokens, &cst, opts, &paragraphs, &mut claims);
    claims.resolve()
}

/// The transaction pre-flight, debug builds only: an invalid merge is an engine
/// bug, not something a caller can be handed.
fn checked(#[allow(unused_variables)] source: &[u8], edits: Vec<Edit>) -> Vec<Edit> {
    #[cfg(debug_assertions)]
    if let Err(error) = crate::edit::check_edits(source, &edits) {
        panic!("format built an invalid transaction: {error}");
    }
    edits
}

/// The formatted bytes — `apply(source, &format_edits(source, opts))`.
pub fn format(source: &[u8], opts: &FormatOptions) -> Vec<u8> {
    apply(source, &format_edits(source, opts))
}

// ---------------------------------------------------------------------------
// Pass 1: lint's own fixes, for the rows that format owns
// ---------------------------------------------------------------------------

/// Runs `lint` exactly as a diagnostics consumer would and keeps the fixes of
/// the FORMATTER rows plus the `repairs` allowlist. Nothing is re-implemented
/// here and nothing is reported; format is a consumer of lint's fixes, invoked
/// internally so callers need no "lint first" protocol.
/// Returns the byte offsets where `missing-paragraph` writes a `\p` — the one
/// fact the Form pass cannot derive for itself, because "is a paragraph open
/// here" is an ANCESTRY question and the answer at that verse is about to change.
fn harvest(
    source: &[u8],
    tokens: &[Token],
    cst: &Cst,
    opts: &FormatOptions,
    out: &mut Claims,
) -> Vec<u32> {
    let report = lint(source, tokens, cst);
    let mut paragraphs = Vec::new();
    for (index, observation) in report.observations.iter().enumerate() {
        let code = observation.code;
        if !(code.row().formatter || opts.repairs.contains(&code)) {
            continue;
        }
        let Some(fix) = report.fix(index) else {
            continue;
        };
        if code == Code::MissingParagraph {
            paragraphs.push(tokens[observation.anchor as usize].start);
        }
        let mut edits = Vec::new();
        for edit in report.edits(fix) {
            retarget(*edit, code, opts, &mut edits);
        }
        out.group(code, edits.into_iter());
    }
    // Document order, because the observations are: a binary search is all the
    // Form pass ever does with this.
    paragraphs
}

/// A harvested fix in the caller's newline form.
///
/// Two rewrites, both of them the OPTIONS speaking about bytes lint had no way
/// to know about: every `\n` becomes the configured ending, and the TRAILING
/// break of the `missing-paragraph` fix is a VERSE break — the axis that owns
/// verse breaks decides it, or `\p\n\v` would survive a `Remove` pass and need a
/// second one.
///
/// A rewrite that outgrows one `FixStr` STACKS: the remainder rides adjacent
/// pure insertions at the splice point (legal, and `apply` concatenates them in
/// order) — the options are never silently dropped.
fn retarget(edit: Edit, code: Code, opts: &FormatOptions, out: &mut Vec<Edit>) {
    let bytes = edit.insert.as_bytes();
    if !bytes.contains(&b'\n') {
        out.push(edit);
        return;
    }
    // Worst case doubles every byte (`\n` → `\r\n`).
    let mut buf = [0u8; FixStr::CAP * 2];
    let mut len = 0;
    for (at, byte) in bytes.iter().enumerate() {
        let piece: &[u8] = if *byte != b'\n' {
            core::slice::from_ref(byte)
        } else if at + 1 == bytes.len()
            && code == Code::MissingParagraph
            && opts.verse_breaks == VerseBreaks::Remove
        {
            b" "
        } else {
            opts.newline.bytes()
        };
        buf[len..len + piece.len()].copy_from_slice(piece);
        len += piece.len();
    }
    let mut rest = &buf[..len];
    let mut from = edit.from;
    loop {
        let take = rest.len().min(FixStr::CAP);
        out.push(Edit {
            from,
            to: edit.to,
            insert: FixStr::new(&rest[..take]),
        });
        rest = &rest[take..];
        if rest.is_empty() {
            return;
        }
        from = edit.to;
    }
}

// ---------------------------------------------------------------------------
// Pass 2: the Form channel
// ---------------------------------------------------------------------------

/// Is this marker one that starts its own line?
///
/// Read off the marker table's MACRO category ([`MarkerKind`]) and nothing else
/// — no authored marker list. Poetry is not special-cased: `\q#` is a Paragraph
/// row like `\p`, so the newline in front of every `\q` IS the block-marker
/// newline and the "poetry exemption" dissolves. The match is exhaustive so a
/// new kind cannot arrive unclassified.
fn is_block_like(marker_idx: generated::MarkerIdx, opts: &FormatOptions) -> bool {
    match generated::kind(marker_idx) {
        MarkerKind::Paragraph
        | MarkerKind::Chapter
        | MarkerKind::TableRow
        | MarkerKind::Header
        | MarkerKind::Sidebar
        | MarkerKind::Periph => true,
        MarkerKind::Verse => opts.verse_breaks == VerseBreaks::Keep,
        // Everything that lives INSIDE a line: character markers, notes,
        // milestones, figures, `\cat`, table cells — and row 0, whose row says
        // nothing about anything.
        MarkerKind::Unknown
        | MarkerKind::Character
        | MarkerKind::Note
        | MarkerKind::Milestone
        | MarkerKind::Figure
        | MarkerKind::Meta
        | MarkerKind::TableCell => false,
    }
}

/// Character, note and milestone boundaries — where a separator must NEVER be
/// invented (`\nd Lord\nd*'s Battles` is correct as written) and where a
/// `char_marker_breaks: Join` line break may become a space.
fn is_inline_boundary(marker_idx: generated::MarkerIdx) -> bool {
    matches!(
        generated::kind(marker_idx),
        MarkerKind::Character | MarkerKind::Note | MarkerKind::Milestone
    )
}

fn is_hws(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// The start of the horizontal whitespace run ending at `at`. Every rule that
/// REPLACES a run (a break, a joined line) swallows the run in front of it in
/// the same edit — otherwise the replacement leaves whitespace behind that only
/// a second pass would clean up.
fn ws_run_start(source: &[u8], at: u32) -> u32 {
    let mut start = at as usize;
    while start > 0 && is_hws(source[start - 1]) {
        start -= 1;
    }
    start as u32
}

fn span<'a>(source: &'a [u8], token: &Token) -> &'a [u8] {
    &source[token.start as usize..token.end() as usize]
}

fn is_ws_text(source: &[u8], token: &Token) -> bool {
    token.kind() == TokenKind::Text && span(source, token).iter().all(|byte| is_hws(*byte))
}

/// The marker name as WRITTEN — past the `\` and past a nested `+`, without the
/// folded delimiter. `remove_markers` matches against this, so `\s5` is
/// removable even though it resolves to row 0.
fn written_name<'a>(source: &'a [u8], token: &Token) -> &'a [u8] {
    let name = payload_label(span(source, token));
    let name = name.strip_prefix(b"\\").unwrap_or(name);
    name.strip_prefix(b"+").unwrap_or(name)
}

/// The FORM CHANNEL's walk: one linear pass over the tokens, plus the CST for
/// the two rows that need a subtree (`remove-marker`) or a lookahead
/// (`bridge-empty-verses`).
///
/// The state is three facts a token cannot answer for itself: whether we are at
/// the start of a line, whether a paragraph is open (which is what makes a verse
/// break removable), and where the current run of blank lines began.
fn form_pass(
    source: &[u8],
    tokens: &[Token],
    cst: &Cst,
    opts: &FormatOptions,
    paragraphs: &[u32],
    out: &mut Claims,
) {
    // Removal runs FIRST and marks its tokens invisible, so every rule below
    // sees the document the caller asked for: the line break in front of a
    // removed `\s5` is judged against the `\v` that follows it, not against the
    // marker about to disappear.
    let removed = remove_markers(source, tokens, cst, opts, out);
    let mut at_line_start = true;
    let mut in_paragraph = false;
    // Whitespace-only Text keeps a run open — `\n   \n` is a blank line however
    // it was typed — and cannot change `in_paragraph`, so the flag the run
    // carries is still true at the close.
    let mut run: Option<Run> = None;

    for (idx, token) in tokens.iter().enumerate() {
        if removed.get(idx).copied().unwrap_or(false) {
            continue;
        }
        let idx = idx as u32;
        let kind = token.kind();
        if kind == TokenKind::Newline {
            run = Some(match run {
                Some(open) => Run {
                    last: idx,
                    count: open.count + 1,
                    ..open
                },
                None => Run {
                    first: idx,
                    last: idx,
                    count: 1,
                    in_paragraph,
                },
            });
            at_line_start = true;
            continue;
        }
        if is_ws_text(source, token) {
            // A whitespace-only run neither ends the line-start state nor breaks
            // a vertical run: `\n   \n` is a blank line however it was typed.
            trim_edges(source, tokens, opts, idx, at_line_start, out);
            continue;
        }
        if let Some(open) = run.take() {
            close_run(source, tokens, opts, open, Some(idx), paragraphs, out);
        }

        match kind {
            TokenKind::Text => {
                trim_edges(source, tokens, opts, idx, at_line_start, out);
                at_line_start = false;
            }
            TokenKind::Marker { .. } | TokenKind::Milestone { .. } => {
                if at_line_start {
                    indentation(source, tokens, opts, idx, out);
                } else if opts.block_marker_own_line && is_block_like(token.marker_idx, opts) {
                    block_break(source, opts, token, out);
                }
                delimiter(source, tokens, opts, idx, Code::DelimiterSingle, out);
                in_paragraph = match generated::kind(token.marker_idx) {
                    MarkerKind::Paragraph | MarkerKind::TableCell => true,
                    // A verse either sits in a paragraph already or receives one
                    // from `missing-paragraph` in THIS transaction, so the verses
                    // after it are inside a paragraph either way.
                    MarkerKind::Verse => true,
                    MarkerKind::Chapter
                    | MarkerKind::Sidebar
                    | MarkerKind::Periph
                    | MarkerKind::Header
                    | MarkerKind::TableRow
                    // Row 0 is the walker's pop-all recovery: no paragraph
                    // survives an unclassifiable marker (en_ulb's `\s5`).
                    | MarkerKind::Unknown => false,
                    _ => in_paragraph,
                };
                at_line_start = false;
            }
            TokenKind::Designator | TokenKind::NoteCaller | TokenKind::BookCode => {
                if opts.designator_ws_single {
                    delimiter(source, tokens, opts, idx, Code::DesignatorWsSingle, out);
                }
                if kind == TokenKind::Designator && opts.dedupe_verse_number {
                    dedupe_verse_number(source, tokens, idx, out);
                }
                at_line_start = false;
            }
            _ => at_line_start = false,
        }
    }
    if let Some(open) = run.take() {
        close_run(source, tokens, opts, open, None, paragraphs, out);
    }

    if opts.bridge_empty_verses {
        bridge_empty_verses(source, tokens, out);
    }
}

/// One run of vertical whitespace: where it starts, the last ending in it, how
/// many there are, and whether a paragraph was open when it began.
#[derive(Clone, Copy)]
struct Run {
    first: u32,
    last: u32,
    count: u32,
    in_paragraph: bool,
}

/// A run of vertical whitespace, decided once its FOLLOWER is known.
///
/// The run collapses to its LAST newline, not its first: the surviving break is
/// then the one touching what comes next, which is the break the verse-join and
/// the line-join rules want to speak about. Collapsing to the first would leave
/// those rules editing bytes this rule had already deleted, and the document
/// would need a second pass to settle.
fn close_run(
    source: &[u8],
    tokens: &[Token],
    opts: &FormatOptions,
    run: Run,
    next: Option<u32>,
    paragraphs: &[u32],
    out: &mut Claims,
) {
    let Run {
        first,
        last,
        count,
        in_paragraph,
    } = run;
    let collapsing = count > 1 && opts.collapse_blank_lines;
    if collapsing {
        let from = tokens[first as usize].start;
        let to = tokens[last as usize].start;
        out.one(Code::CollapseBlankLines, from, to, b"");
    } else if opts.normalize_newlines {
        // Without the collapse the run keeps every one of its endings, and every
        // one of them owes the configured form.
        for token in &tokens[first as usize..last as usize] {
            if token.kind() == TokenKind::Newline && span(source, token) != opts.newline.bytes() {
                out.one(
                    Code::NormalizeNewlines,
                    token.start,
                    token.end(),
                    opts.newline.bytes(),
                );
            }
        }
    }

    let newline = &tokens[last as usize];
    let follower = next.map(|idx| &tokens[idx as usize]);
    let opener = follower.filter(|token| {
        matches!(
            token.kind(),
            TokenKind::Marker { .. } | TokenKind::Milestone { .. }
        )
    });

    // A verse break inside a paragraph, when the caller asked for flowing text —
    // unless THIS verse is where `missing-paragraph` writes its `\p`, which is a
    // block marker and wants the line this break gives it.
    let joins_verse = opts.verse_breaks == VerseBreaks::Remove
        && in_paragraph
        && opener.is_some_and(|token| {
            generated::kind(token.marker_idx) == MarkerKind::Verse
                && paragraphs.binary_search(&token.start).is_err()
        });
    // A break sitting on a character/note/milestone boundary, when asked. A run
    // with no follower at all is the file's last line ending: there is nothing
    // on the far side of it to join to.
    let joins_chars = opts.char_marker_breaks == CharBreaks::Join
        && next.is_some()
        && (opener.is_some_and(|token| is_inline_boundary(token.marker_idx))
            || first > 0 && closes_inline(&tokens[first as usize - 1]));

    if joins_verse || joins_chars {
        let code = if joins_verse {
            Code::BlockMarkerOwnLine
        } else {
            Code::CharMarkerLineJoin
        };
        out.one(
            code,
            ws_run_start(source, newline.start),
            newline.end(),
            b" ",
        );
    } else if opts.normalize_newlines && span(source, newline) != opts.newline.bytes() {
        out.one(
            Code::NormalizeNewlines,
            newline.start,
            newline.end(),
            opts.newline.bytes(),
        );
    }
}

/// Does this token END a character/note/milestone element — the left half of a
/// joinable boundary?
fn closes_inline(token: &Token) -> bool {
    match token.kind() {
        TokenKind::ClosingMarker { .. } => is_inline_boundary(token.marker_idx),
        TokenKind::MilestoneTerminator => true,
        _ => false,
    }
}

/// The EDGES of a text run — never its interior. `\v 1 In  the beginning` keeps
/// its double space because those bytes are content; `\p   Text` has no leading
/// run at all, the scanner having folded it into the marker.
///
/// A run against a line boundary goes away; a run against a marker shrinks to
/// one space. NBSP is not structural whitespace and is never collapsed.
fn trim_edges(
    source: &[u8],
    tokens: &[Token],
    opts: &FormatOptions,
    idx: u32,
    at_line_start: bool,
    out: &mut Claims,
) {
    if !opts.trim_text_edges {
        return;
    }
    let token = &tokens[idx as usize];
    let bytes = span(source, token);
    let at_line_end = tokens
        .get(idx as usize + 1)
        .is_none_or(|next| next.kind() == TokenKind::Newline);

    let lead = bytes.iter().take_while(|byte| is_hws(**byte)).count() as u32;
    if lead as usize == bytes.len() {
        // Indentation in front of a line-leading marker is the other row's, so
        // the two never both claim one span.
        if at_line_start
            && tokens.get(idx as usize + 1).is_some_and(|next| {
                matches!(
                    next.kind(),
                    TokenKind::Marker { .. } | TokenKind::Milestone { .. }
                )
            })
        {
            return;
        }
        // A whitespace-only run between two markers, or a line of nothing.
        let text: &[u8] = if at_line_start || at_line_end {
            b""
        } else {
            b" "
        };
        if bytes != text {
            out.one(Code::TrimTextEdges, token.start, token.end(), text);
        }
        return;
    }
    if lead > 0 {
        let text: &[u8] = if at_line_start { b"" } else { b" " };
        if &bytes[..lead as usize] != text {
            out.one(Code::TrimTextEdges, token.start, token.start + lead, text);
        }
    }
    let trail = bytes.iter().rev().take_while(|byte| is_hws(**byte)).count() as u32;
    if trail > 0 {
        let text: &[u8] = if at_line_end { b"" } else { b" " };
        if &bytes[bytes.len() - trail as usize..] != text {
            out.one(Code::TrimTextEdges, token.end() - trail, token.end(), text);
        }
    }
}

/// The whitespace in front of a line-leading marker: gone. The scanner leaves it
/// as a whitespace-only Text token, so the rule is a span delete.
fn indentation(source: &[u8], tokens: &[Token], opts: &FormatOptions, idx: u32, out: &mut Claims) {
    if !opts.marker_ws_at_line_start || idx == 0 {
        return;
    }
    let before = &tokens[idx as usize - 1];
    if is_ws_text(source, before) {
        out.one(Code::MarkerWsAtLineStart, before.start, before.end(), b"");
    }
}

/// A block-like marker that is not at the start of its line gets one.
///
/// The edit SWALLOWS the horizontal whitespace in front of the marker, so
/// `\p \v 1 a` becomes `\p\n\v 1 a` and not `\p \n\v 1 a`.
///
/// The GLUED paragraph case (`text\p`) is left alone here: that is
/// `marker-not-ws-preceded`'s site, whose fix format has already harvested —
/// one emit site for the two rows, the old code keeping the case it owns.
fn block_break(source: &[u8], opts: &FormatOptions, token: &Token, out: &mut Claims) {
    let glued = token.start > 0 && !is_hws(source[token.start as usize - 1]);
    if glued && generated::kind(token.marker_idx) == MarkerKind::Paragraph {
        return;
    }
    out.one(
        Code::BlockMarkerOwnLine,
        ws_run_start(source, token.start),
        token.start,
        opts.newline.bytes(),
    );
}

/// The delimiter the scanner folded onto the end of a marker's span (or of a
/// designator/caller/book-code payload): one space, or nothing when the line
/// ends right after it.
fn delimiter(
    source: &[u8],
    tokens: &[Token],
    opts: &FormatOptions,
    idx: u32,
    code: Code,
    out: &mut Claims,
) {
    if code == Code::DelimiterSingle && !opts.delimiter_single {
        return;
    }
    let token = &tokens[idx as usize];
    let bytes = span(source, token);
    let run = bytes.iter().rev().take_while(|byte| is_hws(**byte)).count() as u32;
    if run == 0 {
        return;
    }
    let at_line_end = tokens
        .get(idx as usize + 1)
        .is_none_or(|next| next.kind() == TokenKind::Newline);
    let text: &[u8] = if at_line_end { b"" } else { b" " };
    if &bytes[bytes.len() - run as usize..] != text {
        out.one(code, token.end() - run, token.end(), text);
    }
}

/// Token index → the node that token OPENS, or `u32::MAX`. Built once, and only
/// when `remove_markers` is non-empty: `remove-marker` deletes a whole subtree,
/// which is a node fact, and 13,636 linear scans of a 1.7M-node arena is not.
fn node_owners(tokens: &[Token], cst: &Cst) -> Vec<u32> {
    let mut owners = vec![u32::MAX; tokens.len()];
    for (id, node) in cst.nodes.iter().enumerate().skip(1) {
        let slot = &mut owners[node.token as usize];
        // A container start pushes two nodes from one token; the container (the
        // lower id) is the one that owns the whole subtree.
        if *slot == u32::MAX {
            *slot = id as u32;
        }
    }
    owners
}

/// Wholesale removal of the named markers, as a pre-pass.
///
/// Each match takes its whole [`Cst::extent`] — the caller asked for the marker,
/// and no safety rail stands between them — plus the line ending behind it when
/// the marker had the line to itself, or removing `\s5` would leave the blank
/// line it stood on for a second pass to collapse.
///
/// The returned bitmap is what makes a removal INVISIBLE to the other rules
/// rather than merely earlier than them: the break in front of a removed `\s5`
/// is judged against the `\v` that follows it.
fn remove_markers(
    source: &[u8],
    tokens: &[Token],
    cst: &Cst,
    opts: &FormatOptions,
    out: &mut Claims,
) -> Vec<bool> {
    if opts.remove_markers.is_empty() {
        return Vec::new();
    }
    let owners = node_owners(tokens, cst);
    let mut removed = vec![false; tokens.len()];
    for (idx, token) in tokens.iter().enumerate() {
        if removed[idx] || !matches!(token.kind(), TokenKind::Marker { .. }) {
            continue;
        }
        let name = written_name(source, token);
        if !opts
            .remove_markers
            .iter()
            .any(|wanted| wanted.as_bytes() == name)
        {
            continue;
        }
        let owner = owners[idx];
        let extent = if owner == u32::MAX {
            token.start..token.end()
        } else {
            cst.extent(owner, tokens)
        };
        // Tokens partition the source, so the far end is one binary search away.
        let mut last = tokens.partition_point(|token| token.start < extent.end);
        let mut to = extent.end;
        if at_line_start(source, tokens, idx)
            && let Some(newline) = tokens
                .get(last)
                .filter(|token| token.kind() == TokenKind::Newline && token.start == to)
        {
            to = newline.end();
            last += 1;
        }
        removed[idx..last].fill(true);
        out.one(Code::RemoveMarker, extent.start, to, b"");
    }
    removed
}

/// Is this token the first thing on its line? Whitespace-only text in front of
/// it does not count — that indentation is `marker-ws-at-line-start`'s and goes
/// away with it.
fn at_line_start(source: &[u8], tokens: &[Token], idx: usize) -> bool {
    tokens[..idx]
        .iter()
        .rev()
        .find_map(|token| match token.kind() {
            TokenKind::Newline => Some(true),
            TokenKind::Text if span(source, token).iter().all(|byte| is_hws(*byte)) => None,
            _ => Some(false),
        })
        .unwrap_or(true)
}

/// `\v 2 2 men went` → `\v 2 men went`. The boundary is a NUMBER boundary, so
/// `\v 2 2000 men` is prose and stays: the repeat must be the whole first word.
fn dedupe_verse_number(source: &[u8], tokens: &[Token], idx: u32, out: &mut Claims) {
    if idx == 0 || generated::kind(tokens[idx as usize - 1].marker_idx) != MarkerKind::Verse {
        return;
    }
    let number = payload_label(span(source, &tokens[idx as usize]));
    if number.is_empty() || !number.iter().all(u8::is_ascii_digit) {
        return;
    }
    let Some(text) = tokens.get(idx as usize + 1) else {
        return;
    };
    if text.kind() != TokenKind::Text {
        return;
    }
    let bytes = span(source, text);
    let lead = bytes.iter().take_while(|byte| is_hws(**byte)).count();
    let rest = &bytes[lead..];
    if !rest.starts_with(number) || rest[number.len()..].first().is_some_and(u8::is_ascii_digit) {
        return;
    }
    let mut end = lead + number.len();
    while bytes.get(end).is_some_and(|byte| is_hws(*byte)) {
        end += 1;
    }
    out.one(
        Code::DedupeVerseNumber,
        text.start,
        text.start + end as u32,
        b"",
    );
}

/// `\v 1\v 2\v 3 asdf` → `\v 1-3 asdf`: the first designator becomes the range
/// and the empty markers go. No verse IDENTITY is dropped — the run's numbers
/// are all still named by the bridge, which is what separates this from deleting
/// an orphan empty verse (never ported).
fn bridge_empty_verses(source: &[u8], tokens: &[Token], out: &mut Claims) {
    let mut at = 0usize;
    while at < tokens.len() {
        let Some((first_designator, first)) = verse_at(source, tokens, at) else {
            at += 1;
            continue;
        };
        // Walk the run of EMPTY verses that follows. `next_verse` stops at the
        // first byte of content, so reaching another `\v` IS the emptiness proof.
        let mut last_designator = first_designator;
        let mut last = first;
        let mut bridged = 0u32;
        while let Some((designator, number)) = next_verse(source, tokens, last_designator + 1) {
            bridged += 1;
            last_designator = designator;
            last = number;
        }
        if bridged > 0 && last > first {
            let head = &tokens[first_designator as usize];
            let tail = &tokens[last_designator as usize];
            let label = payload_label(span(source, head)).len() as u32;
            let tail_label = payload_label(span(source, tail)).len() as u32;
            let mut buf = [0u8; 21];
            let range = range_label(first, last, &mut buf);
            out.group(
                Code::BridgeEmptyVerses,
                [
                    Edit {
                        from: head.start,
                        to: head.start + label,
                        insert: FixStr::new(range),
                    },
                    Edit {
                        from: head.start + label,
                        to: tail.start + tail_label,
                        insert: FixStr::EMPTY,
                    },
                ]
                .into_iter(),
            );
        }
        at = last_designator as usize + 1;
    }
}

/// `(designator token, first number)` when `at` is a `\v` with a well-formed
/// plain designator.
fn verse_at(source: &[u8], tokens: &[Token], at: usize) -> Option<(u32, u32)> {
    let token = tokens.get(at)?;
    if !matches!(token.kind(), TokenKind::Marker { .. })
        || generated::kind(token.marker_idx) != MarkerKind::Verse
    {
        return None;
    }
    let designator = tokens.get(at + 1)?;
    if designator.kind() != TokenKind::Designator {
        return None;
    }
    match designator::verse(span(source, designator)) {
        Designator::Wellformed { first, .. } => Some((at as u32 + 1, first)),
        Designator::Malformed => None,
    }
}

/// The next `\v` after `from`, or `None` at the first byte of CONTENT: a bridge
/// may only close a run of verses that hold nothing.
fn next_verse(source: &[u8], tokens: &[Token], from: u32) -> Option<(u32, u32)> {
    let mut at = from as usize;
    while at < tokens.len() {
        let token = &tokens[at];
        match token.kind() {
            TokenKind::Marker { .. } if generated::kind(token.marker_idx) == MarkerKind::Verse => {
                return verse_at(source, tokens, at);
            }
            TokenKind::Newline => {}
            TokenKind::Text if span(source, token).iter().all(|byte| is_hws(*byte)) => {}
            _ => return None,
        }
        at += 1;
    }
    None
}

/// `1`, `3` → `1-3`, back to front into the caller's buffer.
fn range_label(first: u32, last: u32, buf: &mut [u8; 21]) -> &[u8] {
    fn write(buf: &mut [u8; 21], at: &mut usize, number: u32) {
        let mut rest = number;
        loop {
            *at -= 1;
            buf[*at] = b'0' + (rest % 10) as u8;
            rest /= 10;
            if rest == 0 {
                return;
            }
        }
    }
    let mut at = buf.len();
    write(buf, &mut at, last);
    at -= 1;
    buf[at] = b'-';
    write(buf, &mut at, first);
    &buf[at..]
}

// ---------------------------------------------------------------------------
// Merging: row order, first writer wins
// ---------------------------------------------------------------------------

/// One rule's claim on a run of bytes. A claim is ATOMIC — `bridge-empty-verses`
/// rewrites a designator AND deletes the run behind it, and half of that is
/// worse than none — so a claim that cannot be honored is dropped whole.
#[derive(Default)]
struct Claims {
    edits: Vec<Edit>,
    claims: Vec<(Code, u32, u32)>,
}

impl Claims {
    fn one(&mut self, code: Code, from: u32, to: u32, insert: &[u8]) {
        debug_assert!(from <= to, "{}: {from}..{to}", code.row().name);
        if from == to && insert.is_empty() {
            return;
        }
        self.group(
            code,
            core::iter::once(Edit {
                from,
                to,
                insert: FixStr::new(insert),
            }),
        );
    }

    fn group(&mut self, code: Code, edits: impl Iterator<Item = Edit>) {
        let start = self.edits.len() as u32;
        self.edits.extend(edits);
        let end = self.edits.len() as u32;
        if end > start {
            self.claims.push((code, start, end));
        }
    }

    /// THE PRECEDENCE RULE: claims are settled in (position, ROW ORDER) order
    /// and the first writer wins. A claim overlapping bytes already spoken for
    /// is not emitted at all — which is what makes two rules that want the same
    /// newline yield ONE owner instead of an invalid transaction.
    /// Each surviving edit carries its CLAIM SLOT, so a consumer that filters
    /// (`format_edits_in`) can keep or drop a multi-edit claim as one thing.
    fn resolve(self) -> Vec<(Code, u32, Edit)> {
        let mut order: Vec<u32> = (0..self.claims.len() as u32).collect();
        order.sort_unstable_by_key(|slot| {
            let (code, start, _) = self.claims[*slot as usize];
            (self.edits[start as usize].from, code as u16, *slot)
        });
        let mut out: Vec<(Code, u32, Edit)> = Vec::new();
        let mut claimed = 0u32;
        // Where an accepted insertion already put a LINE BREAK. Two insertions at
        // one point are legal and concatenate — `\f*` then the break in front of
        // the `\c` that truncated the note — but two BREAKS at one point are a
        // blank line nobody asked for (the `missing-paragraph` fix writes its own
        // `\p` line, and `block-marker-own-line` must not add a second).
        let mut broke_at = u32::MAX;
        for slot in order {
            let (code, start, end) = self.claims[slot as usize];
            let group = &self.edits[start as usize..end as usize];
            let free = group
                .iter()
                .all(|edit| edit.from >= claimed && !(edit.from == broke_at && ends_a_line(edit)));
            if !free {
                continue;
            }
            for edit in group {
                claimed = claimed.max(edit.to);
                if ends_a_line(edit) {
                    broke_at = edit.from;
                }
                out.push((code, slot, *edit));
            }
        }
        // (from, to): a pure insertion at a position sorts BEFORE a replacement
        // starting there, which is the order `apply`'s right-to-left splice and
        // `check_edits`' disjointness both want.
        out.sort_by_key(|(_, _, edit)| (edit.from, edit.to));
        out
    }
}

/// A pure insertion whose text ends the line it was inserted into.
fn ends_a_line(edit: &Edit) -> bool {
    edit.from == edit.to && edit.insert.as_bytes().last() == Some(&b'\n')
}

#[cfg(test)]
mod tests;
