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

use memchr::memchr;
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
    // consumed (read off `ws_after_name`).
    awaiting_delimiter_ws: bool,
    // True right after emitting a marker whose row consumes a Designator
    // payload (`\c`/`\v`), until the next region: text becomes
    // ONE Designator token; anything else (newline, marker) drops the
    // expectation — a `\v` with no number emits no empty token.
    pending_designator: bool,
    // True from the moment an opener/milestone token is emitted until ANY
    // other token is. Its one job is deciding whether a pipe is in FRONT
    // position (U25001: attributes precede content), and because the text
    // arm clears it on entry, "front position" reduces to "the dispatch loop
    // found this pipe" — no byte-scanning predicate anywhere.
    after_marker: bool,
}

/// One scan in progress: the source, what has been emitted so far, the mode
/// flags, and the two things resolved ONCE per lex.
///
/// Membership here is the file's law made mechanical, not a convenience
/// bundle. **A method is anything that emits a token or mutates mode** — the
/// arms. **Everything PURE stays a FREE FUNCTION** — no scan state in, no
/// emission out. That covers the `*_end` boundary finders, `classify_marker`,
/// `escape_len` and `attr_list_end` (questions about BYTES) and equally
/// `resolve_marker_idx`/`folds_delimiter` (questions about the TABLE): a
/// lookup is pure, so it belongs on this side too.
///
/// That split is why "boundary finding never emits" and "classification is
/// position-free" are checkable from a signature instead of trusted from a
/// comment: `fn marker_end(bytes, start) -> usize` cannot touch mode or push
/// a row, and could not be made to without changing its type. It is the same
/// state discipline the emit-funnel listener is already ruled to follow
/// (ideas/committed/linter.md: `feed(token, source)` only, never ScanMode),
/// applied inward — and it is what keeps `lex_general_path_only` a
/// trustworthy oracle, since both paths share every stateless decision and
/// can therefore only diverge in EMISSION, which is exactly what
/// tests/fast_path_identity.rs checks.
///
/// The sections below each keep their boundary finder next to the arm that
/// uses it, so `impl Scanner` appears once per section rather than as one
/// block — reading order follows the scan, not the type.
///
/// **Every arm is `#[inline(always)]`, and that is LOAD-BEARING, not
/// decoration.** The arms are ORGANIZATIONAL boundaries, not call-sharing
/// ones — `run` is the only caller of each — so forcing them inline just
/// reassembles the one big loop body they were split out of, at no code-size
/// cost. It also decides whether the mode flags live in registers or in
/// memory: reached through `&mut self` from an out-of-line arm, every flag
/// read and write is a load/store, and MEASURED that way this struct was 6%
/// SLOWER than the threaded-parameter version it replaced (en_ult 1096-1112
/// → 1034-1045 MiB/s). Inlined, the same code is ~5% FASTER than that
/// baseline (1164-1167) because the flags and `bytes` stay in registers
/// across the whole dispatch loop. Dropping an `inline(always)` here looks
/// like tidying and costs ~10%.
struct Scanner<'a> {
    bytes: &'a [u8],
    tokens: Vec<Token>,
    mode: ScanMode,
    /// Built ONCE per lex: `memmem::find` (the one-shot form) reconstructs its
    /// searcher on every call — measured 31ns/call vs 6ns with a prebuilt
    /// Finder, against a ~5ns/token budget. The text arm calls this once per
    /// text-run iteration, so the one-shot form was a real tax.
    opt_break_finder: memmem::Finder<'static>,
    /// Resolved once per lex so no fast arm ever pays a name match.
    hot: HotIdx,
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
    let mut scanner = Scanner::new(source);
    scanner.run::<FAST>();
    scanner.tokens
}

impl<'a> Scanner<'a> {
    fn new(source: &'a str) -> Self {
        Scanner {
            bytes: source.as_bytes(),
            // Presize from source length: onion measured a hard density floor
            // of ~6.5 bytes per lexeme across the corpus (poetry, `\w`, and
            // full `\zaln` alignment all land 6.6-7.4; plain prose is sparser
            // at ~16), so `/6` sits just under the floor and effectively never
            // reallocs.
            tokens: Vec::with_capacity(source.len() / 6),
            mode: ScanMode {
                awaiting_delimiter_ws: false,
                pending_designator: false,
                after_marker: false,
            },
            opt_break_finder: memmem::Finder::new(b"//"),
            hot: HotIdx::resolve(),
        }
    }

    fn run<const FAST: bool>(&mut self) {
        // Hoisted out of the loop: this is read twice per ITERATION (bound
        // check + dispatch byte), and reaching through `&mut self` for a
        // ptr+len pair each time is real measured cost on token-dense input.
        let bytes = self.bytes;
        let mut index = 0usize;
        while index < bytes.len() {
            // common_marker_checks: fused fast checks for the hottest
            // markers, added ONE ARM AT A TIME in measured-frequency order
            // (planning/marker-frequencies.md). Each arm must be
            // token-identical to the general path — pinned by
            // tests/fast_path_identity.rs, which runs the corpora against
            // `lex_general_path_only`.
            if FAST && bytes[index] == BACKSLASH {
                if let Some(next) = self.common_marker_checks(index) {
                    index = next;
                    continue;
                }
            }
            index = match bytes[index] {
                // A space run is special ONLY as a marker's awaited delimiter.
                // Otherwise it is ordinary content and flows into the text arm,
                // merging with what follows — never Text + Text back to back.
                SPACE | TAB if self.mode.awaiting_delimiter_ws => self.whitespace_arm(index),
                CR | LF => self.newline_arm(index),
                // An escape is content WHEREVER it appears: at a region start
                // (`\~` right after a newline) the marker arm must never see it.
                BACKSLASH if escape_len(bytes, index).is_some() => self.text_arm(index),
                BACKSLASH => self.marker_arm(index),
                // A pipe at a region start is in FRONT position exactly when a
                // marker preceded it — the U25001 node-initial form, plus the
                // legacy empty-content form (`\zaln-s |x-strong="G1"\*`), which
                // only reaches here because it  folds the delimiter into the
                // marker's span. If it opens no list the bytes were content all
                // along, so the text arm takes them from the pipe.
                PIPE => match self.try_attr_list(index, self.mode.after_marker) {
                    Ok(end) => end,
                    Err(_) => self.text_arm(index),
                },
                _ => self.text_arm(index),
            };
        }
    }
}

// ---- common marker checks ---------------------------------------

/// The hot-marker rows, resolved ONCE per lex so no arm ever pays a name
/// match. Membership is the measured top-9 cut (planning/marker-frequencies.md:
/// v 62k · q 47k · p 18k · s 17k · f · b · ft · fr · xt, ~85% of occurrences;
/// `c` deliberately excluded — one per chapter doesn't pay for an arm).
/// One hot row: everything an arm needs about it, so an arm never reads the
/// table. Its index, whether its class folds a following space/tab run as the
/// structural delimiter (the same table fact the general path reads per hit —
/// `\b` does NOT fold; its space is content), and its highest legal level
/// digit (0 = unnumbered, which is most of them).
#[derive(Clone, Copy)]
struct Hot {
    idx: generated::MarkerIdx,
    folds: bool,
    level_max: u8,
}

#[derive(Clone, Copy)]
struct HotIdx {
    v: Hot,
    q: Hot,
    p: Hot,
    s: Hot,
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
            Hot {
                idx,
                // Every hot marker is a plain opener by construction (the arms
                // bail on `\+q`, `\f*`), so the shape half of the predicate is
                // fixed here — but it is the SAME predicate the general path
                // runs, which is what keeps `fast_path_identity` honest.
                folds: folds_delimiter(TokenKind::Marker { nested: false }, idx),
                level_max: match generated::numbering(idx) {
                    Numbering::UpTo(cap) => cap,
                    _ => 0,
                },
            }
        };
        HotIdx {
            v: hot(b"v"),
            q: hot(b"q"),
            p: hot(b"p"),
            s: hot(b"s"),
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
impl Scanner<'_> {
    #[inline(always)]
    fn common_marker_checks(&mut self, index: usize) -> Option<usize> {
        let bytes = self.bytes;
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
                self.push_marker(index, digits_from, self.hot.v.idx);
                self.push_token(TokenKind::Designator, digits_from, end);
                self.mode.awaiting_delimiter_ws = false;
                self.mode.pending_designator = false;
                // The designator is content, so this arm ends with a token that is
                // NOT a marker — matching the general path, where the text arm
                // clears the flag before taking the designator.
                self.mode.after_marker = false;
                Some(end)
            }
            // Numbered paragraph families: name + at most ONE level digit,
            // validated against the row's cap (an over-cap level is row 0 —
            // general path's business).
            b'q' => self.fused_leveled(index, index + 2, self.hot.q),
            b's' => self.fused_leveled(index, index + 2, self.hot.s),
            // Plain one/two-letter names, delimiter or line end after.
            b'p' => self.fused_plain(index, index + 2, self.hot.p),
            b'b' => self.fused_plain(index, index + 2, self.hot.b),
            b'f' => match bytes.get(index + 2) {
                Some(&b't') => self.fused_plain(index, index + 3, self.hot.ft),
                Some(&b'r') => self.fused_plain(index, index + 3, self.hot.fr),
                _ => self.fused_plain(index, index + 2, self.hot.f),
            },
            b'x' if bytes.get(index + 2) == Some(&b't') => {
                self.fused_plain(index, index + 3, self.hot.xt)
            }
            _ => None,
        }
    }

    /// A hot NUMBERED marker: `name_end` sits right after the alpha stem; accept
    /// at most one digit `1..=hot.level_max` before the delimiter.
    #[inline(always)]
    fn fused_leveled(&mut self, index: usize, name_end: usize, hot: Hot) -> Option<usize> {
        let name_end = match self.bytes.get(name_end) {
            Some(&d) if d.is_ascii_digit() => {
                if !(b'1'..=b'0' + hot.level_max).contains(&d) {
                    return None; // over-cap level, or a second digit follows.
                }
                name_end + 1
            }
            _ => name_end,
        };
        self.fused_plain(index, name_end, hot)
    }

    /// The shared tail of every non-`v` arm: after the (possibly leveled) name,
    /// fold a space/tab delimiter run into the marker span exactly like the
    /// general path, or take the marker + its line ending in one hit. Any other
    /// next byte (alnum continuing a longer name, `*`, `-`, EOF) bails.
    #[inline(always)]
    fn fused_plain(&mut self, index: usize, name_end: usize, hot: Hot) -> Option<usize> {
        match self.bytes.get(name_end) {
            Some(&SPACE | &TAB) if hot.folds => {
                // The whole run is the structural delimiter, same as the fold.
                let end = ws_run_end(self.bytes, name_end + 1);
                self.push_marker(index, end, hot.idx);
                self.mode.awaiting_delimiter_ws = false;
                self.mode.pending_designator = false;
                // Every hot marker is an opener, so a pipe right after the folded
                // delimiter is in front position — same as the general path.
                self.mode.after_marker = true;
                Some(end)
            }
            Some(&SPACE | &TAB) => {
                // Non-delimiter class (`\b`): the space is CONTENT — emit the
                // marker alone and let the space open the next text run.
                self.push_marker(index, name_end, hot.idx);
                self.mode.awaiting_delimiter_ws = false;
                self.mode.pending_designator = false;
                // Still true here, and still correct: the space is content, so the
                // text arm clears the flag before any pipe can be reached.
                self.mode.after_marker = true;
                Some(name_end)
            }
            Some(&CR | &LF) => {
                // Marker + its line ending (`\n` or `\r\n`) in one hit.
                self.push_marker(index, name_end, hot.idx);
                let end = newline_end(self.bytes, name_end);
                self.push_token(TokenKind::Newline, name_end, end);
                self.mode.awaiting_delimiter_ws = false;
                self.mode.pending_designator = false;
                // This arm ends on the Newline token, not the marker.
                self.mode.after_marker = false;
                Some(end)
            }
            _ => None,
        }
    }
}

/// does a marker of this SHAPE and row fold a following
/// space/tab run into its own span as the structural delimiter?
///
/// The table is the authority on the QUESTION, but it cannot be asked
/// first, because shape plays two different roles around it:
///
/// 1. **Shape VETOES (not a pre-filter — it beats the table).** End markers
///    never absorb: `\w*`'s trailing space is content. Reading the row
///    first would get this WRONG, since `\w*` shares `\w`'s row and that
///    row says `TagEndDelimiter` — a closer has no row of its own to
///    disagree with, so the shape has to win here or nothing can.
/// 2. **The row DECIDES, read permissively.** For openers and milestones,
///    fold whenever the row PERMITS horizontal whitespace after the name,
///    required or optional alike: "optional" means zero-or-more HS is
///    allowed, and HS that is actually present is still the delimiter, not
///    content. Only `SingleNewline` abstains, because its delimiter is a
///    NEWLINE and a newline is never folded into a marker span — Newline
///    tokens are structurally load-bearing, and hiding a line boundary
///    inside a marker would cost more than the token saves. (`\v`'s row is
///    `AtLeastOneWhitespace`, i.e. HS *or* newline; `ws_run_end` eats
///    space/tab only, so `\v\n1` still emits its Newline. Deliberate.)
/// 3. **Shape DEFAULTS when there is no row.** An unresolved marker has no
///    table opinion to read, so it follows the shape rule like anything
///    else. Written as its own branch even though row 0's `NotRequired`
///    would fall through to the same answer: that coincidence holds only
///    while row 0 is the value's sole holder, and the row must keep its
///    honest value so lint can never read "an unknown marker requires a
///    delimiter" out of a scanner convenience.
fn folds_delimiter(kind: TokenKind, idx: generated::MarkerIdx) -> bool {
    if !matches!(kind, TokenKind::Marker { .. } | TokenKind::Milestone) {
        return false;
    }
    if idx == generated::UNRESOLVED {
        return true;
    }
    !matches!(generated::ws_after_name(idx), Ws::SingleNewline)
}

// ---- emit -----------------------------------------------------------------

impl Scanner<'_> {
    /// A fused marker token: plain opener shape, row already known.
    #[inline(always)]
    fn push_marker(&mut self, start: usize, end: usize, idx: generated::MarkerIdx) {
        self.push_token(TokenKind::Marker { nested: false }, start, end);
        if let Some(last) = self.tokens.last_mut() {
            last.marker_idx = idx;
        }
    }

    /// Pushes one token, splitting anything longer than `u16::MAX` into several
    /// same-kind rows. Splitting is harmless under partition (adjacent same-kind
    /// spans concatenate back to identical bytes); realistically only text runs
    /// could ever approach the limit.
    fn push_token(&mut self, kind: TokenKind, start: usize, end: usize) {
        debug_assert!(end >= start);
        let mut at = start;
        while end - at > u16::MAX as usize {
            self.tokens.push(Token {
                start: at as u32,
                len: u16::MAX,
                kind_bits: kind.to_bits(),
                marker_idx: 0,
            });
            at += u16::MAX as usize;
        }
        self.tokens.push(Token {
            start: at as u32,
            len: (end - at) as u16,
            kind_bits: kind.to_bits(),
            marker_idx: 0,
        });
    }
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

impl Scanner<'_> {
    /// Folds a marker's structural delimiter run into that marker's span. Only
    /// dispatched while `awaiting_delimiter_ws` (marker_arm decides, per-class);
    /// any other space run enters the text arm as ordinary content.
    #[inline(always)]
    fn whitespace_arm(&mut self, index: usize) -> usize {
        let end = ws_run_end(self.bytes, index);
        self.mode.awaiting_delimiter_ws = false;
        if let Some(last) = self.tokens.last_mut() {
            last.len = (end as u32 - last.start) as u16;
        }
        end
    }
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

impl Scanner<'_> {
    #[inline(always)]
    fn newline_arm(&mut self, index: usize) -> usize {
        self.mode.awaiting_delimiter_ws = false;
        self.mode.pending_designator = false;
        self.mode.after_marker = false;
        let end = newline_end(self.bytes, index);
        self.push_token(TokenKind::Newline, index, end);
        end
    }
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

///  resolve an already-bounded, already-classified marker slice to
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

impl Scanner<'_> {
    #[inline(always)]
    fn marker_arm(&mut self, index: usize) -> usize {
        let bytes = self.bytes;
        let end = marker_end(bytes, index);
        let slice = &bytes[index..end];
        let kind = classify_marker(slice);
        self.push_token(kind, index, end);
        // push_token always pushes at least one row, and a marker slice is far
        // below the u16 split threshold, so `last` IS this marker's token.
        let idx = resolve_marker_idx(slice, kind);
        if let Some(last) = self.tokens.last_mut() {
            last.marker_idx = idx;
        }
        // Per-class delimiter fold — one predicate, shared
        // with the fast arms so the two paths cannot drift.
        self.mode.awaiting_delimiter_ws = folds_delimiter(kind, idx);
        // Only an opener or milestone can have attributes in front of it; a
        // closer has nothing in front of it by definition.
        self.mode.after_marker = matches!(kind, TokenKind::Marker { .. } | TokenKind::Milestone);
        //  does this row consume a designator payload (`\c`/`\v`)?
        // The assignment doubles as clearing any stale expectation.
        self.mode.pending_designator = matches!(kind, TokenKind::Marker { .. })
            && matches!(generated::payload(idx), Payload::Designator);
        end
    }
}

// ---- attribute lists ------------------------------------------------------

/// What one pipe turned out to be. The three rungs of the ladder, and the
/// ONLY three things a raw pipe can mean.
enum AttrScan {
    /// U25001 node-initial: the list closed itself with a second pipe.
    /// Payload is one PAST that pipe.
    NodeInitial(usize),
    /// Legacy trailing (3.1; deprecated in 3.2, removed in 4): the list ran
    /// to the node's own terminator. Payload is AT the terminating
    /// backslash, which the list does not include.
    Trailing(usize),
    /// Not a list at all. Payload is the byte the scan refuted on — the
    /// caller resumes THERE, never at the pipe, since any pipe in between
    /// would refute on that identical byte.
    NotAList(usize),
}

/// Boundary: where an attribute list starting at `pipe_at` ends, and which
/// of the three shapes it is. Escape-aware and BOUNDED TO THE LINE, which is
/// the whole recovery story: a malformed list degrades to content plus a lint
/// hint without ever consuming past its own newline.
///
/// `front` says the pipe is in front position — nothing but the marker and
/// its folded delimiter before it. It is the only thing that makes a second
/// raw pipe a terminator, and that single gate reproduces the spec's own
/// disambiguation: `\w|Jesus|\w*` is node-initial, `\w|Jesus\w*` is the 3.1
/// default-attribute form, and a pipe AFTER content (`\w a|b|c\w*`) can only
/// be legacy, so its interior pipes are ordinary span bytes.
///
/// The terminator is only checked for BEING a closer, never for matching the
/// open frame: lint owns the real stack, and `\add*` ending a `\w` list is a
/// finding, not a lexing decision.
/// VECTORIZED, and it has to be: alignment corpora are mostly attribute
/// bytes (`\zaln-s` lists run ~150 bytes each), so a scalar loop here scans
/// the majority of such a document one byte at a time — Same two-scan
/// shape the text arm uses: three needles fit one `memchr3`, and the fourth
/// (the closing pipe, which only terminates in front position) rides a
/// second call BOUNDED to the first hit, since a pipe past the terminator
/// could never win the minimum anyway.
fn attr_list_end(bytes: &[u8], pipe_at: usize, front: bool) -> AttrScan {
    let mut index = pipe_at + 1;
    while index < bytes.len() {
        let rest = &bytes[index..];
        let control = memchr3(BACKSLASH, CR, LF, rest);
        let bound = control.unwrap_or(rest.len());
        let pipe = if front {
            memchr(PIPE, &rest[..bound])
        } else {
            None
        };
        let offset = match (control, pipe) {
            (Some(c), Some(p)) => c.min(p),
            (Some(c), None) => c,
            (None, Some(p)) => p,
            // No terminator anywhere ahead: never a list.
            (None, None) => return AttrScan::NotAList(bytes.len()),
        };
        let pos = index + offset;
        match bytes[pos] {
            // The line bound. A list never spans one.
            CR | LF => return AttrScan::NotAList(pos),
            // Only ever searched for when `front`, so reaching this IS rung 1.
            PIPE => return AttrScan::NodeInitial(pos + 1),
            _ => match escape_len(bytes, pos) {
                // `\|` and `\\` inside a value are content and keep the scan
                // going — `\|` is load-bearing under U25001, where a raw pipe
                // would end the list.
                Some(len) => index = pos + len,
                // A real marker: either this node's terminator (list
                // confirmed) or a refutation. Nothing else it can be.
                None => {
                    let slice = &bytes[pos..marker_end(bytes, pos)];
                    return match classify_marker(slice) {
                        TokenKind::ClosingMarker { .. } | TokenKind::MilestoneEnd => {
                            AttrScan::Trailing(pos)
                        }
                        _ => AttrScan::NotAList(pos),
                    };
                }
            },
        }
    }
    AttrScan::NotAList(bytes.len())
}

/// Emits one `AttrList` token if the pipe at `pipe_at` opens a list.
///
/// `Ok(cursor)` = a list was recognized and emitted. `Err(stop)` = it was
/// not; `stop` is where the caller should resume (see [`AttrScan::NotAList`]).
/// Shared by both callers — the dispatch loop for front position and, later,
/// the text arm for back position — so the two can never disagree about what
/// a pipe means.
impl Scanner<'_> {
    #[inline(always)]
    fn try_attr_list(&mut self, pipe_at: usize, front: bool) -> Result<usize, usize> {
        let (end, absorbs_trailing_ws) = match attr_list_end(self.bytes, pipe_at, front) {
            // U25001's production puts `<HS>*` INSIDE the attribute_list, after
            // the closing pipe — so those bytes belong to the list. Re-arming the
            // delimiter fold is all it takes: `whitespace_arm` extends the last
            // token, which is this one.
            AttrScan::NodeInitial(end) => (end, true),
            // A trailing list is followed by its closer, never by a delimiter.
            AttrScan::Trailing(end) => (end, false),
            AttrScan::NotAList(stop) => return Err(stop),
        };
        self.push_token(TokenKind::AttrList, pipe_at, end);
        self.mode.awaiting_delimiter_ws = absorbs_trailing_ws;
        // Nothing can be in front of a node twice.
        self.mode.after_marker = false;
        // `pending_designator` is deliberately UNTOUCHED: `\v|script="Arab"| 1`
        // still owes its designator, and an attribute list is not the content
        // that would cancel it.
        Ok(end)
    }
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

impl Scanner<'_> {
    #[inline(always)]
    fn text_arm(&mut self, index: usize) -> usize {
        // Copied out ONCE so the byte slices below borrow the SOURCE (`'a`) rather
        // than `self` — otherwise every `rest`/`slice` would hold a shared borrow
        // across the `self.push_token(…)` calls that follow.
        let bytes = self.bytes;
        self.mode.awaiting_delimiter_ws = false;
        // Entering this arm IS content starting, so nothing after this point can
        // be in front position. That is what reduces "front" to "the dispatch
        // loop found the pipe".
        self.mode.after_marker = false;
        //the first content region after `\c`/`\v` is its designator —
        // one token, then this arm is done; whatever follows re-enters the loop
        // as ordinary text.
        if self.mode.pending_designator {
            self.mode.pending_designator = false;
            let end = designator_end(bytes, index);
            // A region opening with an escape (`\v \~…`) has no number to take —
            // fall through to the ordinary scan, same as a designator-less `\v`.
            if end > index {
                self.push_token(TokenKind::Designator, index, end);
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
            // Called through `self` rather than held in a local: a
            // `&self.opt_break_finder` binding would live across the pushes below.
            let opt_break = self.opt_break_finder.find(&rest[..bound]);
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
                        self.push_token(TokenKind::Text, segment_start, pos);
                    }
                    self.push_token(TokenKind::OptBreak, pos, pos + 2);
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
            self.push_token(TokenKind::Text, segment_start, cursor);
        }

        cursor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKER: TokenKind = TokenKind::Marker { nested: false };
    const NESTED_MARKER: TokenKind = TokenKind::Marker { nested: true };
    const CLOSING: TokenKind = TokenKind::ClosingMarker { nested: false };
    const NESTED_CLOSING: TokenKind = TokenKind::ClosingMarker { nested: true };

    const MILESTONE: TokenKind = TokenKind::Milestone;
    const MS_END: TokenKind = TokenKind::MilestoneEnd;
    const TEXT: TokenKind = TokenKind::Text;
    const ATTRS: TokenKind = TokenKind::AttrList;

    fn kinds_and_ranges(tokens: &[Token]) -> Vec<(TokenKind, usize, usize)> {
        tokens
            .iter()
            .map(|t| (t.kind(), t.start as usize, t.end() as usize))
            .collect()
    }

    /// Kinds paired with the bytes they actually cover. Preferred wherever the
    /// SPANS are the point (attribute lists, escapes): it reads as the source
    /// re-spelled, and a wrong boundary shows up as wrong text instead of an
    /// off-by-one to decode. Concatenating the second column is the partition.
    fn kinds_and_text(source: &str) -> Vec<(TokenKind, &str)> {
        lex(source)
            .iter()
            .map(|t| (t.kind(), &source[t.start as usize..t.end() as usize]))
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

    /// the region after `\c`/`\v` is ONE Designator token — happy
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

    /// the fold is SHAPE first, then the row read
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
        // list's pipe at a region start
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

    /// an attribute list in FRONT position — the pipe sits
    /// at a region start because the marker's delimiter was folded into its
    /// span. One span per list, pipes included, interior never parsed.
    #[test]
    fn front_position_attribute_lists() {
        // Milestone, no content, terminated by `\*` — the alignment corpus's
        // shape.
        assert_eq!(
            kinds_and_text("\\zaln-s |x-strong=\"G46130\"\\*"),
            vec![
                (MILESTONE, "\\zaln-s "),
                (ATTRS, "|x-strong=\"G46130\""),
                (MS_END, "\\*"),
            ]
        );
        // Node-initial on a paragraph (U25001): the list closes itself, and
        // per the grammar's trailing `<HS>*` the delimiter space that follows
        // belongs to the LIST, not to the content.
        assert_eq!(
            kinds_and_text("\\p|cat=\"emphasised\"| text"),
            vec![
                (MARKER, "\\p"),
                (ATTRS, "|cat=\"emphasised\"| "),
                (TEXT, "text")
            ]
        );
        // The proposal writes a space before the pipe (`\f |aid="x"| + …`) even
        // though its own production doesn't allow one. The fold absorbs it, so
        // both spellings lex the same and the disagreement stays lint's.
        assert_eq!(
            kinds_and_text("\\f |aid=\"mynote\"| +")[1],
            (ATTRS, "|aid=\"mynote\"| ")
        );
        // A `\v`'s designator SURVIVES its attribute list — the list is not
        // the content that would cancel the expectation.
        assert_eq!(
            kinds_and_text("\\v|script=\"Arab\"| 1 x"),
            vec![
                (MARKER, "\\v"),
                (ATTRS, "|script=\"Arab\"| "),
                (TokenKind::Designator, "1"),
                (TEXT, " x"),
            ]
        );
        // The spec hangs node-initial vs 3.1-trailing on the closing pipe
        // alone, and the ladder's ordering reproduces exactly that.
        assert_eq!(kinds_and_text("\\w|Jesus|\\w*")[1], (ATTRS, "|Jesus|"));
        assert_eq!(kinds_and_text("\\w|Jesus\\w*")[1], (ATTRS, "|Jesus"));
        // Empty list: the pipe alone is the token.
        assert_eq!(kinds_and_text("\\w|\\w*")[1], (ATTRS, "|"));
        // `\fig`: front, no content, terminated by its named closer.
        assert_eq!(
            kinds_and_text("\\fig |src=\"a.png\" size=\"col\"\\fig*")[1],
            (ATTRS, "|src=\"a.png\" size=\"col\"")
        );
        // An escaped pipe is a value byte, not a terminator — load-bearing
        // under U25001, where a raw pipe would end the list.
        assert_eq!(kinds_and_text("\\w|a\\|b|\\w*")[1], (ATTRS, "|a\\|b|"));
    }

    /// Rung 3: a pipe that opens no list was never a delimiter. No token of
    /// its own (the `Pipe` kind is retired), no repair, and NOTHING consumed
    /// past its own line — that bound is the whole recovery story.
    #[test]
    fn a_pipe_that_opens_no_list_is_ordinary_content() {
        // Newline before any terminator refutes it.
        assert_eq!(
            kinds_and_text("\\p |x=\"y\"\n"),
            vec![
                (MARKER, "\\p "),
                (TEXT, "|x=\"y\""),
                (TokenKind::Newline, "\n")
            ]
        );
        // So does a marker that is not a closer.
        assert_eq!(
            kinds_and_text("\\p |x=\"y\"\\add z\\add*"),
            vec![
                (MARKER, "\\p "),
                (TEXT, "|x=\"y\""),
                (MARKER, "\\add "),
                (TEXT, "z"),
                (CLOSING, "\\add*"),
            ]
        );
        // EOF refutes it too, and a bare pipe after a milestone is now Text
        // rather than a token of its own.
        assert_eq!(
            kinds_and_text("\\zaln-s|"),
            vec![(MILESTONE, "\\zaln-s"), (TEXT, "|")]
        );
        // No marker in front at all: an ordinary prose pipe, one Text run.
        assert_eq!(kinds_and_text("a|b"), vec![(TEXT, "a|b")]);
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
        // folds its delimiter space like any other opener.
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

    /// marker tokens carry their table row; every other spelling
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
