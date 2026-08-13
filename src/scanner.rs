//! The Scanner: the only code that owns position. One fused pass over the
//! source that only ever does two things, kept strictly apart —
//!
//! - **boundary finding** (the `*_end` functions): the only code that decides
//!   where a token stops. Owns all cursor movement.
//! - **classification** (`classify_marker`, plus the small mode decisions in
//!   the ws/text arms): names the shape of a slice. May read scan mode (and,
//!   later, marker-table columns), but never parses a payload's interior —
//!   attribute key/values, verse numbers, book codes are interpreters' work,
//!   on demand, later.
//!
//! `Header` lives here because it is what one scan discovers: scan output
//! shape, not row format. It moves out only if emission ever gives it
//! behavior of its own.

use memchr::memchr3;
use memchr::memmem;

use crate::tables::generated;
use crate::tables::schema::{
    Numbering, Payload, SpellingShape, StructuralWhitespaceRequirement as Ws, USV_ESCAPE_LETTERS,
};
use crate::token::{Token, TokenKind};

// Named once so every match arm/peek reads as "is this a marker-start"
// instead of a bare `b'\\'` scattered across every function.
const BACKSLASH: u8 = b'\\';
const PIPE: u8 = b'|';
const SLASH: u8 = b'/';
const TILDE: u8 = b'~';
const SPACE: u8 = b' ';
const TAB: u8 = b'\t';
const CR: u8 = b'\r';
const LF: u8 = b'\n';
const STAR: u8 = b'*';
const PLUS: u8 = b'+';
const HYPHEN: u8 = b'-';
const MILESTONE_START: u8 = b's';
const MILESTONE_END: u8 = b'e';

/// What one scan of a book discovers about its structure, beyond the tokens
/// themselves. STUB — defined for shape agreement, not yet emitted by `lex`;
/// emission is a later step (planning/NEXT-STEPS.md step "Header emission").
///
/// Likely rename: `ParseHeader`. Three jobs, zero extra fields: nav toc,
/// materialize-one-chapter index, and the book↔slot COORDINATE ADAPTER —
/// the run table's base offsets convert book-absolute spans to
/// slot-relative (subtract; find the run by binary search over bases) and
/// back (add), so the store derives its slot view from one spec parse
/// without re-lexing (planning/ideas/committed/braidv2.md).
#[derive(Debug, Clone, Default)]
pub struct Header {
    /// The book code as a span over whatever came after the first `\id` —
    /// a SLICE, any length, invalid codes kept verbatim, never truncated.
    /// Later `\id` occurrences are ordinary tokens (and lint's business).
    pub book: Option<(u32, u16)>,
    /// One entry per `\c` run, in source order.
    /// toc and the single-chapter materialization index.
    pub runs: Vec<ChapterRun>,
}

/// One chapter run: the row range it covers, where its label text lives, and
/// which repeat of that label this is (reopened/duplicate chapters are real
/// data — the ordinal is derived and positional, never typed).
#[derive(Debug, Clone)]
pub struct ChapterRun {
    /// Token row indices `[first, last)` belonging to this run.
    pub rows: core::ops::Range<u32>,
    /// Span of the label text after `\c`.
    pub label: (u32, u16),
    /// 0 for the first occurrence of this label in the book, 1 for the next…
    pub occurrence: u8,
}

/// Scan-pass state. Mode only — never payload knowledge.
struct ScanMode {
    // True right after emitting a marker whose row takes a structural
    // delimiter, until the one whitespace run that delimits it has been
    // consumed (step 4.2: per-class, read off `ws_after_name`).
    awaiting_delimiter_ws: bool,
    // True right after emitting a marker whose row consumes a Designator
    // payload (`\c`/`\v`, step 4.3), until the next region: text becomes
    // ONE Designator token; anything else (newline, marker) drops the
    // expectation — a `\v` with no number emits no empty token.
    pending_designator: bool,
}

/// Lexes a whole source into compact token rows.
///
/// The loop dispatches on the first byte of the next region; each arm calls
/// a boundary finder (which alone moves the cursor) and then classifies the
/// slice it found.
pub fn lex(source: &str) -> Vec<Token> {
    lex_impl::<true>(source)
}

/// The general path alone, fast checks compiled out. Exists ONLY as the
/// oracle for `tests/fast_path_identity.rs`: every `common_marker_checks`
/// arm must produce a token stream identical to this.
#[doc(hidden)]
pub fn lex_general_path_only(source: &str) -> Vec<Token> {
    lex_impl::<false>(source)
}

fn lex_impl<const FAST: bool>(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    // Presize from source length: onion measured a hard density floor of
    // ~6.5 bytes per lexeme across the corpus (poetry, `\w`, and full
    // `\zaln` alignment all land 6.6-7.4; plain prose is sparser at ~16),
    // so `/6` sits just under the floor and effectively never reallocs.
    let mut tokens: Vec<Token> = Vec::with_capacity(source.len() / 6);
    let mut mode = ScanMode {
        awaiting_delimiter_ws: false,
        pending_designator: false,
    };
    // Built ONCE per lex: memmem::find (the one-shot form) reconstructs its
    // searcher on every call — measured 31ns/call vs 6ns with a prebuilt
    // Finder, against a ~5ns/token budget. The text arm calls this once per
    // text-run iteration, so the one-shot form was a real tax.
    let opt_break_finder = memmem::Finder::new(b"//");
    // Resolved once per lex so no fast arm ever pays a name match.
    let hot = HotIdx::resolve();
    let mut index = 0usize;

    while index < bytes.len() {
        // common_marker_checks (step 4.4): fused fast checks for the hottest
        // markers, added ONE ARM AT A TIME in measured-frequency order
        // (planning/marker-frequencies.md). Each arm must be token-identical
        // to the general path — pinned by tests/fast_path_identity.rs, which
        // runs the corpora against `lex_general_path_only`.
        if FAST && bytes[index] == BACKSLASH {
            if let Some(next) = common_marker_checks(bytes, index, &hot, &mut mode, &mut tokens) {
                index = next;
                continue;
            }
        }
        index = match bytes[index] {
            // A space run is special ONLY as a marker's awaited delimiter.
            // Otherwise it is ordinary content and flows into the text arm,
            // merging with what follows — never Text + Text back to back.
            SPACE | TAB if mode.awaiting_delimiter_ws => {
                whitespace_arm(bytes, index, &mut mode, &mut tokens)
            }
            CR | LF => newline_arm(bytes, index, &mut mode, &mut tokens),
            // An escape is content WHEREVER it appears: at a region start
            // (`\~` right after a newline) the marker arm must never see it.
            BACKSLASH if escape_len(bytes, index).is_some() => {
                text_arm(bytes, index, &mut mode, &mut tokens, &opt_break_finder)
            }
            BACKSLASH => marker_arm(bytes, index, &mut mode, &mut tokens),
            PIPE => pipe_arm(index, &mut mode, &mut tokens),
            _ => text_arm(bytes, index, &mut mode, &mut tokens, &opt_break_finder),
        };
    }

    tokens
}

// ---- common marker checks (step 4.4) ---------------------------------------

/// The hot-marker rows, resolved ONCE per lex so no arm ever pays a name
/// match. Membership is the measured top-9 cut (planning/marker-frequencies.md:
/// v 62k · q 47k · p 18k · s 17k · f · b · ft · fr · xt, ~85% of occurrences;
/// `c` deliberately excluded — one per chapter doesn't pay for an arm).
/// One hot row: its index plus whether its class folds a following
/// space/tab run as the structural delimiter — the same table fact the
/// general path reads per hit (`\b` does NOT fold; its space is content).
#[derive(Clone, Copy)]
struct Hot {
    idx: generated::MarkerIdx,
    folds: bool,
}

struct HotIdx {
    v: Hot,
    q: Hot,
    q_max: u8,
    p: Hot,
    s: Hot,
    s_max: u8,
    b: Hot,
    f: Hot,
    ft: Hot,
    fr: Hot,
    xt: Hot,
}

impl HotIdx {
    fn resolve() -> Self {
        let hot = |name: &[u8]| {
            let idx = generated::marker_idx(name, SpellingShape::PlainOnly);
            // Every hot marker is a plain opener by construction (the arms
            // bail on `\+q`, `\f*`), so the shape half of the predicate is
            // fixed here — but it is the SAME predicate the general path
            // runs, which is what keeps `fast_path_identity` honest.
            let folds = folds_delimiter(TokenKind::Marker { nested: false }, idx);
            Hot { idx, folds }
        };
        let cap = |h: Hot| match generated::numbering(h.idx) {
            Numbering::UpTo(cap) => cap,
            _ => 0,
        };
        let (q, s) = (hot(b"q"), hot(b"s"));
        HotIdx {
            v: hot(b"v"),
            q,
            q_max: cap(q),
            p: hot(b"p"),
            s,
            s_max: cap(s),
            b: hot(b"b"),
            f: hot(b"f"),
            ft: hot(b"ft"),
            fr: hot(b"fr"),
            xt: hot(b"xt"),
        }
    }
}

/// The fused fast checks for the hot markers: one shape test emits what the
/// general path needs several arm passes for. Every arm is CONSERVATIVE —
/// anything off its happy shape (`\+q`, `\q1a`, `\s5`, `\v  1`, `\f*`)
/// returns None and takes the general path, which stays the definition
/// (pinned token-identical by tests/fast_path_identity.rs).
///
/// On a hit the cursor returns just past what was consumed; the next loop
/// iteration dispatches normally, so the text arm's memchr scan picks up
/// exactly at the next region.
#[inline(always)]
fn common_marker_checks(
    bytes: &[u8],
    index: usize,
    hot: &HotIdx,
    mode: &mut ScanMode,
    tokens: &mut Vec<Token>,
) -> Option<usize> {
    match *bytes.get(index + 1)? {
        // `\v ` + pure digits + structural stop: marker (delimiter folded)
        // plus its Designator, the corpus's most frequent hit by 2x.
        b'v' if bytes.get(index + 2) == Some(&SPACE) => {
            let digits_from = index + 3;
            let mut end = digits_from;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end == digits_from
                || !matches!(
                    bytes.get(end),
                    None | Some(&SPACE | &TAB | &CR | &LF | &BACKSLASH | &PIPE)
                )
            {
                return None; // `\v \p`, `\v 1a`: the general path decides.
            }
            push_marker(tokens, index, digits_from, hot.v.idx);
            push_token(tokens, TokenKind::Designator, digits_from, end);
            mode.awaiting_delimiter_ws = false;
            mode.pending_designator = false;
            Some(end)
        }
        // Numbered paragraph families: name + at most ONE level digit,
        // validated against the row's cap (an over-cap level is row 0 —
        // general path's business).
        b'q' => fused_leveled(bytes, index, index + 2, hot.q, hot.q_max, mode, tokens),
        b's' => fused_leveled(bytes, index, index + 2, hot.s, hot.s_max, mode, tokens),
        // Plain one/two-letter names, delimiter or line end after.
        b'p' => fused_plain(bytes, index, index + 2, hot.p, mode, tokens),
        b'b' => fused_plain(bytes, index, index + 2, hot.b, mode, tokens),
        b'f' => match bytes.get(index + 2) {
            Some(&b't') => fused_plain(bytes, index, index + 3, hot.ft, mode, tokens),
            Some(&b'r') => fused_plain(bytes, index, index + 3, hot.fr, mode, tokens),
            _ => fused_plain(bytes, index, index + 2, hot.f, mode, tokens),
        },
        b'x' if bytes.get(index + 2) == Some(&b't') => {
            fused_plain(bytes, index, index + 3, hot.xt, mode, tokens)
        }
        _ => None,
    }
}

/// A hot NUMBERED marker: `name_end` sits right after the alpha stem; accept
/// at most one digit `1..=max` before the delimiter.
#[inline(always)]
fn fused_leveled(
    bytes: &[u8],
    index: usize,
    name_end: usize,
    hot: Hot,
    max: u8,
    mode: &mut ScanMode,
    tokens: &mut Vec<Token>,
) -> Option<usize> {
    let name_end = match bytes.get(name_end) {
        Some(&d) if d.is_ascii_digit() => {
            if !(b'1'..=b'0' + max).contains(&d) {
                return None; // over-cap level, or a second digit follows.
            }
            name_end + 1
        }
        _ => name_end,
    };
    fused_plain(bytes, index, name_end, hot, mode, tokens)
}

/// The shared tail of every non-`v` arm: after the (possibly leveled) name,
/// fold a space/tab delimiter run into the marker span exactly like the
/// general path, or take the marker + its line ending in one hit. Any other
/// next byte (alnum continuing a longer name, `*`, `-`, EOF) bails.
#[inline(always)]
fn fused_plain(
    bytes: &[u8],
    index: usize,
    name_end: usize,
    hot: Hot,
    mode: &mut ScanMode,
    tokens: &mut Vec<Token>,
) -> Option<usize> {
    match bytes.get(name_end) {
        Some(&SPACE | &TAB) if hot.folds => {
            // The whole run is the structural delimiter, same as the fold.
            let end = ws_run_end(bytes, name_end + 1);
            push_marker(tokens, index, end, hot.idx);
            mode.awaiting_delimiter_ws = false;
            mode.pending_designator = false;
            Some(end)
        }
        Some(&SPACE | &TAB) => {
            // Non-delimiter class (`\b`): the space is CONTENT — emit the
            // marker alone and let the space open the next text run.
            push_marker(tokens, index, name_end, hot.idx);
            mode.awaiting_delimiter_ws = false;
            mode.pending_designator = false;
            Some(name_end)
        }
        Some(&CR | &LF) => {
            // Marker + its line ending (`\n` or `\r\n`) in one hit.
            push_marker(tokens, index, name_end, hot.idx);
            let end = newline_end(bytes, name_end);
            push_token(tokens, TokenKind::Newline, name_end, end);
            mode.awaiting_delimiter_ws = false;
            mode.pending_designator = false;
            Some(end)
        }
        _ => None,
    }
}

/// Step 4.6: does a marker of this SHAPE and row fold a following
/// space/tab run into its own span as the structural delimiter?
///
/// Two facts, in this order:
///
/// 1. **Shape first.** Openers and milestones delimit with space; end
///    markers (`\w*`, `\*`) do not — their trailing whitespace is content,
///    whatever the shared row says.
/// 2. **Then the row, read permissively.** Fold whenever the row PERMITS
///    horizontal whitespace after the name, required or optional alike:
///    "optional" means zero-or-more HS is allowed, and HS that is actually
///    present is still the delimiter, not content. Only `SingleNewline`
///    abstains, because its delimiter is a NEWLINE and a newline is never
///    folded into a marker span — Newline tokens are structurally
///    load-bearing, and hiding a line boundary inside a marker would cost
///    more than the token saves. (`\v`'s row is `AtLeastOneWhitespace`,
///    i.e. HS *or* newline; `ws_run_end` eats space/tab only, so `\v\n1`
///    still emits its Newline. Deliberate.)
///
/// The UNRESOLVED row is defaulted here, explicitly, rather than by
/// reading its `NotRequired`: unknown markers follow the shape rule like
/// anything else. The row keeps its honest value so lint can never read
/// "an unknown marker requires a delimiter" out of a scanner
/// convenience. Today `NotRequired` has exactly one holder — row 0 itself
/// — so leaving this to fall out of the match would work by accident and
/// would silently start folding if a real row ever took that value.
fn folds_delimiter(kind: TokenKind, idx: generated::MarkerIdx) -> bool {
    if !matches!(kind, TokenKind::Marker { .. } | TokenKind::Milestone) {
        return false;
    }
    if idx == generated::UNRESOLVED {
        return true;
    }
    !matches!(generated::ws_after_name(idx), Ws::SingleNewline)
}

/// A fused marker token: plain opener shape, row already known.
#[inline(always)]
fn push_marker(tokens: &mut Vec<Token>, start: usize, end: usize, idx: generated::MarkerIdx) {
    push_token(tokens, TokenKind::Marker { nested: false }, start, end);
    if let Some(last) = tokens.last_mut() {
        last.marker_idx = idx;
    }
}

// ---- emit -----------------------------------------------------------------

/// Pushes one token, splitting anything longer than `u16::MAX` into several
/// same-kind rows. Splitting is harmless under partition (adjacent same-kind
/// spans concatenate back to identical bytes); realistically only text runs
/// could ever approach the limit.
fn push_token(tokens: &mut Vec<Token>, kind: TokenKind, start: usize, end: usize) {
    debug_assert!(end >= start);
    let mut at = start;
    while end - at > u16::MAX as usize {
        tokens.push(Token {
            start: at as u32,
            len: u16::MAX,
            kind_bits: kind.to_bits(),
            marker_idx: 0,
        });
        at += u16::MAX as usize;
    }
    tokens.push(Token {
        start: at as u32,
        len: (end - at) as u16,
        kind_bits: kind.to_bits(),
        marker_idx: 0,
    });
}

// ---- whitespace arm ---------------------------------------------------------

/// Boundary: the end of a space/tab run.
fn ws_run_end(bytes: &[u8], from: usize) -> usize {
    let mut index = from;
    while index < bytes.len() && matches!(bytes[index], SPACE | TAB) {
        index += 1;
    }
    index
}

/// Folds a marker's structural delimiter run into that marker's span. Only
/// dispatched while `awaiting_delimiter_ws` (marker_arm decides, per-class);
/// any other space run enters the text arm as ordinary content.
fn whitespace_arm(
    bytes: &[u8],
    index: usize,
    mode: &mut ScanMode,
    tokens: &mut Vec<Token>,
) -> usize {
    let end = ws_run_end(bytes, index);
    mode.awaiting_delimiter_ws = false;
    if let Some(last) = tokens.last_mut() {
        last.len = (end as u32 - last.start) as u16;
    }
    end
}

// ---- newline arm ------------------------------------------------------------

/// Boundary: one newline, `\r\n` taken as a single token.
fn newline_end(bytes: &[u8], from: usize) -> usize {
    // One byte consumed for `\n` (or a bare `\r`); when that byte was CR and
    // an LF follows, consume it too so `\r\n` is ONE two-byte token rather
    // than being torn into two newlines.
    let mut index = from + 1;
    if bytes[from] == CR && index < bytes.len() && bytes[index] == LF {
        index += 1;
    }
    index
}

fn newline_arm(bytes: &[u8], index: usize, mode: &mut ScanMode, tokens: &mut Vec<Token>) -> usize {
    mode.awaiting_delimiter_ws = false;
    mode.pending_designator = false;
    let end = newline_end(bytes, index);
    push_token(tokens, TokenKind::Newline, index, end);
    end
}

// ---- marker arm ---------------------------------------------------------------

/// Boundary: where a `\`-initiated token ends. Walks the marker grammar
/// (optional `+`, alnum name, optional `-s`/`-e` milestone suffix, optional
/// closing `*`) purely to find the END — the shape decision is re-derived
/// from the slice by `classify_marker`, cursor-free.
fn marker_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1; // past the `\`

    // Nested spelling: the `+` of `\+w` sits between `\` and the name.
    if bytes.get(index) == Some(&PLUS) {
        index += 1;
    }

    // The marker NAME: ascii letters/digits — the `p` of `\p`, `qt2` of
    // `\qt2`, `zaln` of `\zaln-s` (this loop stops at the hyphen).
    let name_start = index;
    while index < bytes.len() && bytes[index].is_ascii_alphanumeric() {
        index += 1;
    }

    if index == name_start && bytes.get(index) == Some(&STAR) {
        // `\*` — no name at all before the `*`: bare milestone-span close.
        return index + 1;
    }
    if bytes.get(index) == Some(&HYPHEN)
        && matches!(
            bytes.get(index + 1),
            Some(&MILESTONE_START) | Some(&MILESTONE_END)
        )
    {
        // `\zaln-s`, `\qt-e` — name + `-s`/`-e` milestone open/close.
        return index + 2;
    }
    if bytes.get(index) == Some(&STAR) {
        // `\it*`, `\+w*` — closing form: name + `*`.
        return index + 1;
    }
    // `\p`, `\v`, `\zsomething` — plain opener; the name's end is the end.
    index
}

/// Classification: names the shape of one already-bounded `\...` slice.
/// Position-free — takes only the bytes of the token itself.
// Deliberately NOT the table's job: this names the SPELLING (opener /
// `*`-closer / `-s|-e` milestone), a fact of these bytes that survives an
// unknown name — `\zaln-s` has no row yet must still be a Milestone token
// to pair with its `\*`. The table dictates the marker's IDENTITY (spec
// kind, contexts) once the shape has picked which row to ask for.
fn classify_marker(slice: &[u8]) -> TokenKind {
    debug_assert_eq!(slice.first(), Some(&BACKSLASH));
    let nested = slice.get(1) == Some(&PLUS);
    let name_from = if nested { 2 } else { 1 };

    let name_len = slice[name_from..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric())
        .count();
    let after_name = name_from + name_len;

    if name_len == 0 && slice.get(after_name) == Some(&STAR) {
        // No name at all before the `*` — closes a milestone span
        // (`\zaln-s ... \*`), not a named closing marker like `\it*`.
        return TokenKind::MilestoneEnd;
    }
    if slice.get(after_name) == Some(&HYPHEN) {
        // `\zaln-s`, `\qt-e` — hyphen after the name is the milestone form.
        return TokenKind::Milestone;
    }
    if slice.get(after_name) == Some(&STAR) {
        // `\it*`, `\+w*` — star after the name closes it.
        return TokenKind::ClosingMarker { nested };
    }
    // `\p`, `\v`, `\+w` — nothing after the name: an opener.
    TokenKind::Marker { nested }
}

/// Step 4.1: resolve an already-bounded, already-classified marker slice to
/// its table row. The lexeme handed to the table is the NAME as spelled —
/// leading `\`/`+` and trailing `*` stripped, `-s`/`-e` kept (the matcher
/// strips those itself). The shape argument is the classification we already
/// made; `qt` is the one name where plain and milestone rows differ.
fn resolve_marker_idx(slice: &[u8], kind: TokenKind) -> generated::MarkerIdx {
    let name_from = if slice.get(1) == Some(&PLUS) { 2 } else { 1 };
    let name_to = if slice.last() == Some(&STAR) {
        slice.len() - 1
    } else {
        slice.len()
    };
    let shape = match kind {
        TokenKind::Milestone | TokenKind::MilestoneEnd => SpellingShape::MilestoneOnly,
        _ => SpellingShape::PlainOnly,
    };
    generated::marker_idx(&slice[name_from..name_to], shape)
}

fn marker_arm(bytes: &[u8], index: usize, mode: &mut ScanMode, tokens: &mut Vec<Token>) -> usize {
    let end = marker_end(bytes, index);
    let slice = &bytes[index..end];
    let kind = classify_marker(slice);
    push_token(tokens, kind, index, end);
    // push_token always pushes at least one row, and a marker slice is far
    // below the u16 split threshold, so `last` IS this marker's token.
    let idx = resolve_marker_idx(slice, kind);
    if let Some(last) = tokens.last_mut() {
        last.marker_idx = idx;
    }
    // Per-class delimiter fold (steps 4.2 + 4.6) — one predicate, shared
    // with the fast arms so the two paths cannot drift.
    mode.awaiting_delimiter_ws = folds_delimiter(kind, idx);
    //  does this row consume a designator payload (`\c`/`\v`)?
    // The assignment doubles as clearing any stale expectation.
    mode.pending_designator = matches!(kind, TokenKind::Marker { .. })
        && matches!(generated::payload(idx), Payload::Designator);
    end
}

// ---- pipe arm -------------------------------------------------------------

// Just the delimiter byte itself — attribute-list handling (one `AttrList`
// region once the fused pass has an open-marker stack; a bare pipe in plain
// text is content) is deferred with the data tables.
fn pipe_arm(index: usize, mode: &mut ScanMode, tokens: &mut Vec<Token>) -> usize {
    mode.awaiting_delimiter_ws = false;
    mode.pending_designator = false;
    let end = index + 1;
    push_token(tokens, TokenKind::Pipe, index, end);
    end
}

// ---- text arm ---------------------------------------------------------------

/// The escaped-content forms the text arm folds: def.txt's TEXT escapes
/// (`\/` `\~` `\\` `\|`) plus the U25004 USV escapes — the letter and its
/// fixed hex width come from the schema's [`USV_ESCAPE_LETTERS`], no
/// terminator. Returns the whole escape's byte length, or None when this
/// backslash starts a real marker. The exact USV pattern BEATS the marker
/// claim, and hex case is lint's business, not a rejection (both ruled at
/// the schema const).
fn escape_len(bytes: &[u8], pos: usize) -> Option<usize> {
    match *bytes.get(pos + 1)? {
        SLASH | TILDE | BACKSLASH | PIPE => Some(2),
        letter => {
            let &(_, width) = USV_ESCAPE_LETTERS
                .iter()
                .find(|&&(l, _)| l as u8 == letter)?;
            let digits = bytes.get(pos + 2..pos + 2 + width)?;
            digits
                .iter()
                .all(|b| b.is_ascii_hexdigit())
                .then_some(2 + width)
        }
    }
}

/// Boundary: where a pending designator ends — the next structural stop.
/// One span, interior NEVER parsed here: `1`, `12-14a`, junk alike; the
/// linter can validate the content
fn designator_end(bytes: &[u8], from: usize) -> usize {
    let mut index = from;
    while index < bytes.len() && !matches!(bytes[index], SPACE | TAB | CR | LF | BACKSLASH | PIPE) {
        index += 1;
    }
    index
}

fn text_arm(
    bytes: &[u8],
    index: usize,
    mode: &mut ScanMode,
    tokens: &mut Vec<Token>,
    opt_break_finder: &memmem::Finder<'_>,
) -> usize {
    mode.awaiting_delimiter_ws = false;
    // Step 4.3: the first content region after `\c`/`\v` is its designator —
    // one token, then this arm is done; whatever follows re-enters the loop
    // as ordinary text.
    if mode.pending_designator {
        mode.pending_designator = false;
        let end = designator_end(bytes, index);
        // A region opening with an escape (`\v \~…`) has no number to take —
        // fall through to the ordinary scan, same as a designator-less `\v`.
        if end > index {
            push_token(tokens, TokenKind::Designator, index, end);
            return end;
        }
    }
    // The start of the `Text` segment currently being built. Separate from
    // `cursor` because a `//` found mid-run ends the current segment early
    // (pushed as its own `OptBreak`) without ending this whole call — text
    // may resume right after it, still inside this one call.
    let mut segment_start = index;
    let mut cursor = index;

    // This function sees the bulk of a typical unaligned-text document's
    // bytes, so it's the one worth making SIMD instead of scalar. Four
    // bytes matter here (`\`, `\r`, `\n`, `/`), one more than a single
    // `memchr3` call can hold — so two vectorized scans per iteration,
    // taking whichever hit comes first, rather than one scan that quietly
    // drops `\r` (and would then tear `\r\n` in half, not just miss the
    // rare bare-`\r` case). `OptBreak` (`//`) is genuinely rare in this
    // corpus, but it's still spec-correct to look for, so it's a real
    // needle, not folded away as a "someday" gap like the marker-payload
    // and attribute-run work.
    loop {
        let rest = &bytes[cursor..];
        let control = memchr3(BACKSLASH, CR, LF, rest);
        // Search for the literal 2-byte needle, not a lone `/` to reject
        // afterward — a hit here is already a confirmed OptBreak, no
        // single-slash false positives to filter, and stray single
        // slashes (legal, ordinary content) cost nothing extra to skip.
        //
        // BOUNDED to the control hit: `//` cannot contain a control byte,
        // so a hit past it could never win the min anyway — and unbounded,
        // every text region scanned to END OF FILE for a needle that is
        // rare-to-absent, turning the whole lex quadratic (measured: 152ms
        // for the 66-book corpus; bounded: see playground).
        let bound = control.unwrap_or(rest.len());
        let opt_break = opt_break_finder.find(&rest[..bound]);
        let offset = match (control, opt_break) {
            (Some(c), Some(o)) => c.min(o),
            (Some(c), None) => c,
            (None, Some(o)) => o,
            (None, None) => {
                cursor = bytes.len();
                break;
            }
        };
        let pos = cursor + offset;

        match bytes[pos] {
            // Escaped literal content (`escape_len`: the four TEXT escapes
            // plus USV) keeps the run going; any other backslash is a real
            // marker start.
            BACKSLASH => match escape_len(bytes, pos) {
                Some(len) => cursor = pos + len,
                None => {
                    cursor = pos;
                    break;
                }
            },
            // A confirmed "//" — split this call's output into two Text
            // segments around it.
            SLASH => {
                if pos > segment_start {
                    push_token(tokens, TokenKind::Text, segment_start, pos);
                }
                push_token(tokens, TokenKind::OptBreak, pos, pos + 2);
                segment_start = pos + 2;
                cursor = pos + 2;
            }
            // CR or LF: always ends the run.
            _ => {
                cursor = pos;
                break;
            }
        }
    }

    if cursor > segment_start {
        push_token(tokens, TokenKind::Text, segment_start, cursor);
    }

    cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKER: TokenKind = TokenKind::Marker { nested: false };
    const NESTED_MARKER: TokenKind = TokenKind::Marker { nested: true };
    const CLOSING: TokenKind = TokenKind::ClosingMarker { nested: false };
    const NESTED_CLOSING: TokenKind = TokenKind::ClosingMarker { nested: true };

    fn kinds_and_ranges(tokens: &[Token]) -> Vec<(TokenKind, usize, usize)> {
        tokens
            .iter()
            .map(|t| (t.kind(), t.start as usize, t.end() as usize))
            .collect()
    }

    #[test]
    fn folds_the_delimiter_whitespace_into_the_marker() {
        assert_eq!(
            kinds_and_ranges(&lex("\\p text here\n")),
            vec![
                (MARKER, 0, 3),
                (TokenKind::Text, 3, 12),
                (TokenKind::Newline, 12, 13),
            ]
        );
    }

    /// Step 4.3: the region after `\c`/`\v` is ONE Designator token — happy
    /// digits, ranges, and junk alike (the interpreter judges content); a
    /// designator-less `\v` emits nothing extra.
    #[test]
    fn chapter_and_verse_take_a_designator_token() {
        assert_eq!(
            kinds_and_ranges(&lex("\\v 1 text")),
            vec![
                (MARKER, 0, 3),
                (TokenKind::Designator, 3, 4),
                (TokenKind::Text, 4, 9),
            ]
        );
        assert_eq!(
            kinds_and_ranges(&lex("\\c 12\n")),
            vec![
                (MARKER, 0, 3),
                (TokenKind::Designator, 3, 5),
                (TokenKind::Newline, 5, 6),
            ]
        );
        // Range + suffix stay ONE span; the scanner never parses inside.
        assert_eq!(
            kinds_and_ranges(&lex("\\v 12-14a x"))[1],
            (TokenKind::Designator, 3, 9)
        );
        // No designator present: the expectation dies with the next marker.
        assert_eq!(
            kinds_and_ranges(&lex("\\v \\p t"))[..2],
            [(MARKER, 0, 3), (MARKER, 3, 6)]
        );
        // Escaped content after `\v `: no number to take, no empty token.
        assert_eq!(
            kinds_and_ranges(&lex("\\v \\~a")),
            vec![(MARKER, 0, 3), (TokenKind::Text, 3, 6)]
        );
        // Other payload-less markers are untouched.
        assert_eq!(
            kinds_and_ranges(&lex("\\p 12 x")),
            vec![(MARKER, 0, 3), (TokenKind::Text, 3, 7)]
        );
    }

    /// Steps 4.2 + 4.6: the fold is SHAPE first, then the row read
    /// permissively — `folds_delimiter` is the whole rule.
    #[test]
    fn only_delimiter_taking_markers_absorb_whitespace() {
        // `\w*` then space: the space is CONTENT — it opens the following
        // text run (one Text token, never Text + Text back to back). Shape
        // decides; the row is shared with the opener and says nothing here.
        assert_eq!(
            kinds_and_ranges(&lex("\\w* x")),
            vec![(CLOSING, 0, 3), (TokenKind::Text, 3, 5)]
        );
        // Unresolved `\zaln-s` (row 0) DOES fold: unknown openers follow the
        // shape rule like anything else, which is what puts an attribute
        // list's pipe at a region start (step 5A).
        assert_eq!(
            kinds_and_ranges(&lex("\\zaln-s x")),
            vec![(TokenKind::Milestone, 0, 8), (TokenKind::Text, 8, 9)]
        );
        // Known milestones fold too — their rows are OptionalHorizontalWhitespace,
        // and "optional" means the HS is permitted, not that it is content.
        assert_eq!(
            kinds_and_ranges(&lex("\\qt-s x"))[0],
            (TokenKind::Milestone, 0, 6)
        );
        // `\b` is the one abstainer: its row is SingleNewline, so its
        // delimiter is a NEWLINE and the space stays content.
        assert_eq!(
            kinds_and_ranges(&lex("\\b x")),
            vec![(MARKER, 0, 2), (TokenKind::Text, 2, 4)]
        );
        // A newline is NEVER folded into a marker span, even when the row
        // permits newline as its delimiter (`\v` is AtLeastOneWhitespace):
        // Newline tokens are structurally load-bearing.
        assert_eq!(
            kinds_and_ranges(&lex("\\v\n1")),
            vec![
                (MARKER, 0, 2),
                (TokenKind::Newline, 2, 3),
                (TokenKind::Text, 3, 4),
            ]
        );
    }

    #[test]
    fn recognizes_marker_sub_kinds() {
        assert_eq!(kinds_and_ranges(&lex("\\it*")), vec![(CLOSING, 0, 4)]);
        assert_eq!(kinds_and_ranges(&lex("\\+w")), vec![(NESTED_MARKER, 0, 3)]);
        assert_eq!(
            kinds_and_ranges(&lex("\\+w*")),
            vec![(NESTED_CLOSING, 0, 4)]
        );
        assert_eq!(
            kinds_and_ranges(&lex("\\zaln-s")),
            vec![(TokenKind::Milestone, 0, 7)]
        );
    }

    #[test]
    fn bare_star_is_milestone_end_not_a_nameless_closing_marker() {
        assert_eq!(
            kinds_and_ranges(&lex("\\*")),
            vec![(TokenKind::MilestoneEnd, 0, 2)]
        );
    }

    #[test]
    fn pipe_delimits_after_a_milestone() {
        assert_eq!(
            kinds_and_ranges(&lex("\\zaln-s|")),
            vec![(TokenKind::Milestone, 0, 7), (TokenKind::Pipe, 7, 8)]
        );
    }

    #[test]
    fn keeps_escaped_delimiters_as_text() {
        // "a" + "\~" (escaped tilde, content) + "b" + "\p" (real marker)
        assert_eq!(
            kinds_and_ranges(&lex("a\\~b\\p")),
            vec![(TokenKind::Text, 0, 4), (MARKER, 4, 6)]
        );
        // At a REGION START too — the marker arm never sees an escape.
        assert_eq!(
            kinds_and_ranges(&lex("\\~b")),
            vec![(TokenKind::Text, 0, 3)]
        );
    }

    /// U25004: `\u` + exactly 4 hex digits (`\U` + 8) is content, beating the
    /// marker claim; a wrong-width spelling stays a marker (row 0, lint's).
    #[test]
    fn usv_escapes_fold_into_the_text_run() {
        assert_eq!(
            kinds_and_ranges(&lex("a\\u0041b")),
            vec![(TokenKind::Text, 0, 8)]
        );
        assert_eq!(
            kinds_and_ranges(&lex("\\U0001F600")),
            vec![(TokenKind::Text, 0, 10)]
        );
        // Wrong width: `\u12` stays a MARKER — unresolved (row 0), so per
        // step 4.6 it folds its delimiter space like any other opener.
        assert_eq!(kinds_and_ranges(&lex("\\u12 x"))[0], (MARKER, 0, 5));
    }

    #[test]
    fn a_lone_slash_is_ordinary_text() {
        assert_eq!(kinds_and_ranges(&lex("a/b")), vec![(TokenKind::Text, 0, 3)]);
    }

    #[test]
    fn splits_text_around_an_optbreak() {
        assert_eq!(
            kinds_and_ranges(&lex("a//b")),
            vec![
                (TokenKind::Text, 0, 1),
                (TokenKind::OptBreak, 1, 3),
                (TokenKind::Text, 3, 4),
            ]
        );
    }

    #[test]
    fn optbreak_at_the_very_start_emits_no_empty_text() {
        assert_eq!(
            kinds_and_ranges(&lex("//x")),
            vec![(TokenKind::OptBreak, 0, 2), (TokenKind::Text, 2, 3)]
        );
    }

    /// Step 4.1: marker tokens carry their table row; every other spelling
    /// fact still lives in the span. Asserted by name round-trip so the test
    /// survives row reordering.
    #[test]
    fn marker_tokens_are_stamped_with_their_row() {
        let named = |source: &str| {
            let tokens = lex(source);
            generated::name(tokens[0].marker_idx)
        };
        assert_eq!(named("\\p x"), "p");
        assert_eq!(named("\\q2 x"), "q"); // level lives in the span
        assert_eq!(named("\\+nd*"), "nd"); // nested + closing both strip
        assert_eq!(named("\\qt-s |who=\"P\"\\*"), "qt");
        assert_eq!(named("\\zaln-s x"), ""); // unconfigured extension → row 0
        assert_eq!(named("\\s7 x"), ""); // illegal level → row 0
        // The one overloaded name resolves per shape: both spellings hit a
        // row NAMED qt, but different rows.
        let plain = lex("\\qt x")[0].marker_idx;
        let milestone = lex("\\qt-s x")[0].marker_idx;
        assert_ne!(plain, milestone);
        assert_eq!(generated::name(plain), "qt");
        assert_eq!(generated::name(milestone), "qt");
    }
}
