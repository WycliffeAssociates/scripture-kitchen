//! The Scanner: the only code that owns position. One fused pass that only
//! ever does two things, kept strictly apart —
//!
//! - **boundary finding** (the `*_end` functions): the only code that decides
//!   where a token stops. Owns all cursor movement.
//! - **classification** (`classify_marker`, plus the mode decisions in the
//!   ws/text arms): names the shape of a slice. May read scan mode and
//!   marker-table columns, but never a payload's INTERIOR — attribute k/v,
//!   verse numbers, book codes are interpreters' work, on demand, later.
//!
//! ```text
//! \v 1 gr//ace \w x|lemma="g"\w*
//! → Marker("\v ") Designator("1 ") Text("gr") OptBreak("//") Text("ace ")
//!   Marker("\w ") Text("x") AttrList("|lemma=\"g\"") ClosingMarker("\w*")
//! ```
//!
//! Concatenating every span reproduces the source byte for byte. That
//! partition IS the losslessness guarantee.
//!
//! # Where the grammar's whitespace goes
//!
//! The delimiter class is `[\t\n\r ]+` and every byte of it stays in the
//! stream; two rules decide which token holds it.
//!
//! 1. **A NEWLINE is always its own token.** No exceptions: block seams, lint's
//!    line discipline, `\n\c` chunking and an editor's line model all key on it.
//! 2. **ONE horizontal code unit (space/tab) folds into the token that
//!    grammatically requires it** — a marker name's delimiter, and each carved
//!    payload's: designator, note caller, book code. The run's surplus is a
//!    `Pad` token: reducible bytes, visible to an editor, dropped whole by
//!    text views.
//!
//! ```text
//! \v 1 text    Marker("\v ") Designator("1 ") Text("text")
//! \v   1 text  Marker("\v ") Pad("  ") Designator("1 ") Text("text")
//! \v 1\ntext   Marker("\v ") Designator("1") Newline("\n") Text("text")
//! \f + \ft n   Marker("\f ") NoteCaller("+ ") Marker("\ft ") Text("n")
//! \id GEN Gen  Marker("\id ") BookCode("GEN ") Text("Gen")
//! ```
//!
//! Both spellings write the same required delimiter; neither is normalized into
//! the other, and either concatenates back to the source.
//!
//! # The designator gate
//!
//! A `Designator` after `\c`/`\v` requires a LEADING ASCII DIGIT; anything else
//! is ordinary Text, so the token kind never claims prose is a number.
//!
//! ```text
//! \v 012 text  Marker("\v ") Designator("012 ") Text("text")
//! \v Then He…  Marker("\v ") Text("Then He…")
//! ```
//!
//! One byte, and no more: `[1-9]`, segments and ranges are `crate::designator`'s
//! grammar, judged where the judgment is richer. `\ca`/`\cp`/`\va`/`\vp` are NOT
//! gated — their payload is a published label (`\cp M`), not a number.

use memchr::memchr;
use memchr::memchr3;
use memchr::memmem;

use crate::tables::generated;
use crate::tables::schema::{
    MarkerKind, Numbering, Payload, SpellingShape, StructuralWhitespaceRequirement as Ws,
    USV_ESCAPE_LETTERS,
};
use crate::token::{Token, TokenKind};

pub(crate) const BACKSLASH: u8 = b'\\';
pub(crate) const PIPE: u8 = b'|';
pub(crate) const SLASH: u8 = b'/';
pub(crate) const TILDE: u8 = b'~';
pub(crate) const SPACE: u8 = b' ';
pub(crate) const TAB: u8 = b'\t';
pub(crate) const CR: u8 = b'\r';
pub(crate) const LF: u8 = b'\n';
pub(crate) const STAR: u8 = b'*';
pub(crate) const PLUS: u8 = b'+';
pub(crate) const HYPHEN: u8 = b'-';
pub(crate) const MILESTONE_START: u8 = b's';
pub(crate) const MILESTONE_END: u8 = b'e';

/// Scan-pass state. Mode only — never payload knowledge.
pub(crate) struct ScanState {
    // Set when the marker just emitted takes a structural delimiter (read off
    // `ws_after_name`), until that one whitespace run has been folded in.
    pub(crate) awaiting_delimiter_ws: bool,
    // What the marker just emitted still owes, straight off its row. Carrying
    // the row's enum rather than a bool per payload keeps ONE code path for all
    // three carved payloads (`\c`/`\v` designator, note caller, `\id` book
    // code).
    pub(crate) pending_payload: Payload,
    // Does the pending Designator have to START WITH A DIGIT to be carved? True
    // for `\c`/`\v`, whose payload IS a number; false for `\ca`/`\cp`/`\va`/
    // `\vp`, whose published labels (`M`, `א`) are legitimately not numbers.
    // Read only while `pending_payload` is `Designator`, and written with it.
    pub(crate) designator_gated: bool,
    // True from an opener/milestone token until ANY other token, so that
    // "pipe in FRONT position" (U25001: attributes precede content) reduces to
    // "the dispatch loop found this pipe" — no byte-scanning predicate.
    pub(crate) after_marker: bool,
    // How many attrs-capable frames are open ON THIS LINE. Nonzero is the ONLY
    // condition under which the text arm looks for a pipe, which keeps the
    // deprecated back-position form from taxing the stop set globally (stop
    // density is the measured wall). Reset at every newline: a frame whose
    // opener and trailing list sit on DIFFERENT lines is then not read as a
    // list (lossless; lint reports the pipe), but never resetting would leave
    // the needle armed for a whole document after one unclosed `\w`.
    pub(crate) attr_frames: u8,
    // The one row whose attribute list is terminated by the LINE rather than by
    // a closer: `\periph My Title|id="x"`. Read ONLY by `try_attr_list`, so the
    // byte-level boundary scan stays closer-shaped.
    pub(crate) attr_list_ends_at_line: bool,
}

/// One scan in progress: the source, what has been emitted, the mode flags,
/// and the two things resolved ONCE per lex.
///
/// Membership is a rule. **A method is anything that emits a token or mutates
/// mode** — the arms. **Everything PURE stays a FREE FUNCTION** — every `*_end`
/// finder, `classify_marker`, `escape_len`, `resolve_marker_idx`,
/// `folds_delimiter`. That makes "boundary finding never emits" checkable from
/// a signature, and keeps `lex_general_path_only` a trustworthy oracle: both
/// paths share every stateless decision, so they can only diverge in EMISSION.
/// Sections below pair each boundary finder with the arm using it, so
/// `impl Scanner` recurs per section and reading order follows the scan.
///
/// **Every arm is `#[inline(always)]`, and that is LOAD-BEARING.** The arms are
/// organizational, not call-sharing (`run` is the only caller of each), so
/// inlining reassembles the one loop body they were split out of — and it
/// decides whether the mode flags live in registers or memory. Out-of-line,
/// `&mut self` flag access measured 6% below threaded parameters (1034-1045 vs
/// 1096-1112 MiB/s on en_ult); inlined it gains ~5% (1164-1167).
struct Scanner<'a> {
    bytes: &'a [u8],
    tokens: Vec<Token>,
    mode: ScanState,
    /// Built ONCE per lex: the one-shot `memmem::find` rebuilds its searcher per
    /// call — 31ns vs 6ns prebuilt, against a ~5ns/token budget.
    opt_break_finder: memmem::Finder<'static>,
    /// Resolved once per lex so no fast arm ever pays a name match.
    hot: HotIdx,
}

/// Lexes a whole source into compact token rows. The loop dispatches on the
/// first byte of the next region; each arm calls a boundary finder (which
/// alone moves the cursor), then classifies the slice.
pub fn lex(source: &str) -> Vec<Token> {
    lex_impl::<true>(source)
}

/// The general path alone, fast checks compiled out. Exists ONLY as the oracle
/// for `tests/fast_path_identity.rs`: every `common_marker_checks` arm must
/// produce a token stream identical to this.
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
            // The measured density floor across the corpus is ~6.5 bytes per
            // lexeme, so `/6` sits just under it and never reallocs.
            tokens: Vec::with_capacity(source.len() / 6),
            mode: ScanState {
                awaiting_delimiter_ws: false,
                pending_payload: Payload::None,
                designator_gated: false,
                after_marker: false,
                attr_frames: 0,
                attr_list_ends_at_line: false,
            },
            opt_break_finder: memmem::Finder::new(b"//"),
            hot: HotIdx::resolve(),
        }
    }

    fn run<const FAST: bool>(&mut self) {
        // Hoisted: read twice per ITERATION, and reaching through `&mut self`
        // for a ptr+len pair each time is measured cost on token-dense input.
        let bytes = self.bytes;
        let mut index = 0usize;
        while index < bytes.len() {
            if FAST
                && bytes[index] == BACKSLASH
                && let Some(next) = self.common_marker_checks(index)
            {
                index = next;
                continue;
            }
            index = match bytes[index] {
                // A space run is special ONLY as a marker's awaited delimiter;
                // otherwise it merges into the text arm's run — never Text +
                // Text back to back.
                SPACE | TAB if self.mode.awaiting_delimiter_ws => self.whitespace_arm(index),
                CR | LF => self.newline_arm(index),
                // An escape is content WHEREVER it appears, so the marker arm
                // never sees one (`\~` right after a newline).
                BACKSLASH if escape_len(bytes, index).is_some() => self.text_arm(index),
                BACKSLASH => self.marker_arm(index),
                // A pipe here is in FRONT position exactly when a marker
                // preceded it; if it opens no list the bytes were content.
                PIPE => match self.try_attr_list(index, self.mode.after_marker, index, None) {
                    Ok(end) => end,
                    Err(_) => self.text_arm(index),
                },
                _ => self.text_arm(index),
            };
        }
    }
}

// ---- common marker checks ---------------------------------------

/// Everything a fast arm needs about one hot marker, so an arm never reads the
/// table. Membership is the measured top-9 cut (`v q p s f b ft fr xt`, ~85% of
/// occurrences); `c` is excluded — one per chapter doesn't pay for an arm.
///
/// Every field is precomputed from the SAME predicate the general path runs per
/// hit, because an arm must decide exactly what that path decides. `folds`:
/// does the class take a following space/tab run as its structural delimiter
/// (`\b` does NOT). `level_max`: highest legal level digit, 0 = unnumbered.
#[derive(Clone, Copy)]
pub(crate) struct Hot {
    pub(crate) idx: generated::MarkerIdx,
    pub(crate) folds: bool,
    pub(crate) level_max: u8,
    /// Does opening this marker arm the text arm's pipe needle? True for the
    /// character-class rows (`ft`, `fr`, `xt`) only.
    pub(crate) attrs_frame: bool,
    /// What this row owes after its delimiter — `NoteCaller` for `\f`,
    /// `Designator` for `\v`, `None` for the rest.
    pub(crate) payload: Payload,
    /// The digit gate on that payload (see `ScanState::designator_gated`).
    pub(crate) designator_gated: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct HotIdx {
    pub(crate) v: Hot,
    pub(crate) q: Hot,
    pub(crate) p: Hot,
    pub(crate) s: Hot,
    pub(crate) b: Hot,
    pub(crate) f: Hot,
    pub(crate) ft: Hot,
    pub(crate) fr: Hot,
    pub(crate) xt: Hot,
}

impl HotIdx {
    pub(crate) fn resolve() -> Self {
        let hot = |name: &[u8]| {
            let idx = generated::marker_idx(name, SpellingShape::PlainOnly);
            Hot {
                idx,
                // Every hot marker is a plain opener by construction (the arms
                // bail on `\+q`, `\f*`), so the shape half of the predicate is
                // fixed here — but the predicate itself is the general path's.
                folds: folds_delimiter(TokenKind::Marker { nested: false }, idx),
                level_max: match generated::numbering(idx) {
                    Numbering::UpTo(cap) => cap,
                    _ => 0,
                },
                attrs_frame: opens_attrs_frame(idx),
                payload: generated::payload(idx),
                designator_gated: designator_gated(idx),
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
/// anything off its happy shape (`\+q`, `\q1a`, `\s5`, `\v  1`, `\f*`) returns
/// None and takes the general path, which stays the definition (pinned
/// token-identical by tests/fast_path_identity.rs). On a hit the cursor returns
/// just past what was consumed, so the next iteration dispatches normally.
impl Scanner<'_> {
    #[inline(always)]
    fn common_marker_checks(&mut self, index: usize) -> Option<usize> {
        let bytes = self.bytes;
        match *bytes.get(index + 1)? {
            // `\v ` + pure digits + structural stop: marker (delimiter folded)
            // plus its Designator. The corpus's most frequent hit by 2x.
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
                // ONE delimiter byte rides the designator's span, exactly as
                // the marker name's does (see `payload_end`); surplus is Pad.
                let designator_end = ws_run_end(bytes, end);
                let keep = designator_end.min(end + 1);
                self.push_marker(index, digits_from, self.hot.v.idx);
                self.push_token(TokenKind::Designator, digits_from, keep);
                if designator_end > keep {
                    self.push_token(TokenKind::Pad, keep, designator_end);
                }
                self.mode.awaiting_delimiter_ws = false;
                self.mode.pending_payload = Payload::None;
                // The designator is content, so this arm ends on a non-marker.
                self.mode.after_marker = false;
                Some(designator_end)
            }
            // Numbered families: name + at most ONE level digit, checked
            // against the row's cap (an over-cap level is the general path's).
            b'q' => self.fused_leveled(index, index + 2, self.hot.q),
            b's' => self.fused_leveled(index, index + 2, self.hot.s),
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

    /// A hot NUMBERED marker: `name_end` sits after the alpha stem; accept at
    /// most one digit `1..=hot.level_max` before the delimiter.
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

    /// The shared tail of every non-`v` arm: after the (possibly leveled)
    /// name, fold ONE delimiter byte into the marker span (any surplus is a
    /// Pad token), or take the marker + its line ending in one hit. Any other
    /// next byte (alnum continuing a longer name, `*`, `-`, EOF) bails.
    #[inline(always)]
    fn fused_plain(&mut self, index: usize, name_end: usize, hot: Hot) -> Option<usize> {
        match self.bytes.get(name_end) {
            Some(&SPACE | &TAB) if hot.folds => {
                let end = ws_run_end(self.bytes, name_end + 1);
                self.push_marker(index, name_end + 1, hot.idx);
                if end > name_end + 1 {
                    self.push_token(TokenKind::Pad, name_end + 1, end);
                }
                self.mode.awaiting_delimiter_ws = false;
                self.mode.pending_payload = Payload::None;
                // Every hot marker is an opener, so a pipe right after the
                // folded delimiter is in front position.
                self.mode.after_marker = true;
                self.mode.pending_payload = hot.payload;
                self.mode.designator_gated = hot.designator_gated;
                if hot.attrs_frame {
                    self.mode.attr_frames = self.mode.attr_frames.saturating_add(1);
                    // No hot row is a Periph — asserted, not assumed.
                    debug_assert_ne!(generated::kind(hot.idx), MarkerKind::Periph);
                }
                Some(end)
            }
            Some(&SPACE | &TAB) => {
                // Non-delimiter class (`\b`): the space is CONTENT — emit the
                // marker alone and let the space open the next text run.
                self.push_marker(index, name_end, hot.idx);
                self.mode.awaiting_delimiter_ws = false;
                self.mode.pending_payload = Payload::None;
                // Still correct: the space is content, so the text arm clears
                // this before any pipe can be reached.
                self.mode.after_marker = true;
                self.mode.pending_payload = hot.payload;
                self.mode.designator_gated = hot.designator_gated;
                if hot.attrs_frame {
                    self.mode.attr_frames = self.mode.attr_frames.saturating_add(1);
                    debug_assert_ne!(generated::kind(hot.idx), MarkerKind::Periph);
                }
                Some(name_end)
            }
            Some(&CR | &LF) => {
                // Marker + its line ending in one hit.
                self.push_marker(index, name_end, hot.idx);
                let end = newline_end(self.bytes, name_end);
                self.push_token(TokenKind::Newline, name_end, end);
                self.mode.awaiting_delimiter_ws = false;
                self.mode.pending_payload = Payload::None;
                // This arm ends on the Newline token, not the marker.
                self.mode.after_marker = false;
                self.mode.attr_frames = 0;
                self.mode.attr_list_ends_at_line = false;
                Some(end)
            }
            _ => None,
        }
    }
}

/// Does opening this row put an attrs-capable frame on the line — i.e. arm the
/// text arm's pipe needle, so a BACK-position (pre-3.2 trailing) list can be
/// found inside a text run?
///
/// ```text
/// \w gracious|lemma="x"\w*     Character: armed
/// \periph My Title|id="x"      Periph:    armed (its list has no closer)
/// \p a|b|c                     paragraph: NOT armed — a pipe here is content
/// ```
///
/// `\periph` is the one PARAGRAPH-shaped marker whose attributes come AFTER its
/// content (usx.rng `PeripheralDivision`: the title is `alt`, the pipe carries
/// `id`). Milestones and notes/paragraphs/verses take the front form only,
/// whose pipe sits at a region start the dispatch arm already sees; row 0 is
/// excluded so arming for every unconfigured `\z` cannot put the needle over
/// custom content. Shared with `Hot::attrs_frame`, so the two paths cannot
/// disagree about when the needle is live.
pub(crate) fn opens_attrs_frame(idx: generated::MarkerIdx) -> bool {
    matches!(
        generated::kind(idx),
        MarkerKind::Character | MarkerKind::Figure | MarkerKind::Periph
    )
}

/// Is this row's `Designator` payload a NUMBER, and therefore subject to the
/// digit gate? `\c`/`\v` yes; `\ca`/`\cp`/`\va`/`\vp` no — their payload is a
/// published label, and `\cp M` must keep carving one.
pub(crate) fn designator_gated(idx: generated::MarkerIdx) -> bool {
    matches!(
        generated::kind(idx),
        MarkerKind::Chapter | MarkerKind::Verse
    )
}

/// Does a marker of this SHAPE and row fold a following space/tab run into its
/// own span as the structural delimiter?
///
/// The table is the authority, but it cannot be asked first, because shape
/// plays three roles around it:
///
/// 1. **Shape VETOES — it beats the table.** A closer never absorbs (`\w*`'s
///    trailing space is content), yet `\w*` shares `\w`'s row, which says
///    `TagEndDelimiter`. A closer has no row of its own to disagree with, so
///    shape has to win or nothing can.
/// 2. **The row DECIDES, read permissively.** Openers and milestones fold
///    whenever the row PERMITS horizontal whitespace after the name, required
///    or optional alike. Only `SingleNewline` abstains, since a newline is
///    never folded into a marker span (Newline tokens are structurally
///    load-bearing) — so `\v`, an `AtLeastOneWhitespace` row, still emits its
///    Newline for `\v\n1`, because `ws_run_end` eats space/tab only.
/// 3. **Shape DEFAULTS when there is no row.** Its own branch even though row
///    0's `NotRequired` gives the same answer: row 0 must keep its honest value
///    so lint never reads "unknown markers require a delimiter" out of a
///    scanner convenience.
pub(crate) fn folds_delimiter(kind: TokenKind, idx: generated::MarkerIdx) -> bool {
    if !matches!(kind, TokenKind::Marker { .. } | TokenKind::Milestone { .. }) {
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

    /// Pushes one token, splitting anything longer than `u16::MAX` into
    /// several same-kind rows — harmless under partition, since adjacent
    /// same-kind spans concatenate back to identical bytes.
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

pub(crate) fn ws_run_end(bytes: &[u8], from: usize) -> usize {
    let mut index = from;
    while index < bytes.len() && matches!(bytes[index], SPACE | TAB) {
        index += 1;
    }
    index
}

impl Scanner<'_> {
    /// Folds a marker's structural delimiter into that marker's span — ONE
    /// code unit of it; the run's remainder is a Pad token. Only dispatched
    /// while `awaiting_delimiter_ws`; any other space run enters the text arm
    /// as ordinary content.
    ///
    /// A Pad push touches NO mode flag: the surplus is invisible to
    /// classification, so a pending payload still carves across it and a
    /// front-position pipe stays front.
    #[inline(always)]
    fn whitespace_arm(&mut self, index: usize) -> usize {
        let end = ws_run_end(self.bytes, index);
        self.mode.awaiting_delimiter_ws = false;
        if let Some(last) = self.tokens.last_mut() {
            last.len = (index as u32 + 1 - last.start) as u16;
        }
        if end > index + 1 {
            self.push_token(TokenKind::Pad, index + 1, end);
        }
        end
    }
}

// ---- newline arm ------------------------------------------------------------

/// Boundary: one newline, `\r\n` taken as a single token.
pub(crate) fn newline_end(bytes: &[u8], from: usize) -> usize {
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
        self.mode.pending_payload = Payload::None;
        self.mode.after_marker = false;
        self.mode.attr_frames = 0;
        self.mode.attr_list_ends_at_line = false;
        let end = newline_end(self.bytes, index);
        self.push_token(TokenKind::Newline, index, end);
        end
    }
}

// ---- marker arm ---------------------------------------------------------------

/// Boundary: where a `\`-initiated token ends. Walks the marker grammar
/// (optional `+`, alnum name, optional `-s`/`-e` milestone suffix, optional
/// closing `*`) purely to find the END — the shape decision is re-derived from
/// the slice by `classify_marker`, cursor-free.
pub(crate) fn marker_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1; // past the `\`

    if bytes.get(index) == Some(&PLUS) {
        index += 1;
    }

    // `p` of `\p`, `qt2` of `\qt2`, `zaln` of `\zaln-s` (stops at the hyphen).
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
    index
}

/// Classification: names the shape of one already-bounded `\...` slice.
/// Position-free — takes only the bytes of the token itself.
///
/// Deliberately NOT the table's job. This names the SPELLING (opener /
/// `*`-closer / `-s|-e` milestone), a fact of these bytes that survives an
/// unknown name: `\zaln-s` has no row yet must still be a Milestone token to
/// pair with its `\*`. The table dictates IDENTITY afterwards.
pub(crate) fn classify_marker(slice: &[u8]) -> TokenKind {
    debug_assert_eq!(slice.first(), Some(&BACKSLASH));
    let nested = slice.get(1) == Some(&PLUS);
    let name_from = if nested { 2 } else { 1 };

    let name_len = slice[name_from..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric())
        .count();
    let after_name = name_from + name_len;

    if name_len == 0 && slice.get(after_name) == Some(&STAR) {
        // No name before the `*` — closes a milestone span, not `\it*`.
        return TokenKind::MilestoneTerminator;
    }
    if slice.get(after_name) == Some(&HYPHEN) {
        // The suffix is one byte (`marker_end` bounds it) and `e` is the end
        // spelling; anything else, `-s` included, is an opener.
        return TokenKind::Milestone {
            end: slice.get(after_name + 1) == Some(&b'e'),
        };
    }
    if slice.get(after_name) == Some(&STAR) {
        // `\it*`, `\+w*` — star after the name closes it.
        return TokenKind::ClosingMarker { nested };
    }
    TokenKind::Marker { nested }
}

/// Resolves an already-classified marker slice to its table row. The lexeme
/// handed to the table is the NAME as spelled — leading `\`/`+` and trailing
/// `*` stripped, `-s`/`-e` kept (the matcher strips those itself). `qt` is the
/// one name where the plain and milestone rows differ, hence the `kind` arg.
pub(crate) fn resolve_marker_idx(slice: &[u8], kind: TokenKind) -> generated::MarkerIdx {
    let name_from = if slice.get(1) == Some(&PLUS) { 2 } else { 1 };
    let name_to = if slice.last() == Some(&STAR) {
        slice.len() - 1
    } else {
        slice.len()
    };
    let shape = match kind {
        TokenKind::Milestone { .. } | TokenKind::MilestoneTerminator => {
            SpellingShape::MilestoneOnly
        }
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
        // A marker slice is far below the u16 split threshold, so `last` IS it.
        let idx = resolve_marker_idx(slice, kind);
        if let Some(last) = self.tokens.last_mut() {
            last.marker_idx = idx;
        }
        self.mode.awaiting_delimiter_ws = folds_delimiter(kind, idx);
        // Only an opener or milestone can have attributes in front of it.
        self.mode.after_marker =
            matches!(kind, TokenKind::Marker { .. } | TokenKind::Milestone { .. });
        // Only OPENERS owe a carved payload — a closer shares its opener's row
        // and would otherwise re-arm it.
        self.mode.pending_payload = if matches!(kind, TokenKind::Marker { .. }) {
            generated::payload(idx)
        } else {
            Payload::None
        };
        self.mode.designator_gated = designator_gated(idx);
        // Any closer decrements, even a mismatched one: the count only gates a
        // needle, so being approximately right costs one refuted ladder at
        // most, and pairing closers to openers is the walker's job.
        match kind {
            TokenKind::Marker { .. } if opens_attrs_frame(idx) => {
                self.mode.attr_frames = self.mode.attr_frames.saturating_add(1);
                if generated::kind(idx) == MarkerKind::Periph {
                    self.mode.attr_list_ends_at_line = true;
                }
            }
            TokenKind::ClosingMarker { .. } | TokenKind::MilestoneTerminator => {
                self.mode.attr_frames = self.mode.attr_frames.saturating_sub(1)
            }
            _ => {}
        }
        end
    }
}

// ---- attribute lists ------------------------------------------------------

/// What one pipe turned out to be — the ONLY three things a raw pipe can mean.
pub(crate) enum AttrScan {
    /// U25001 node-initial: the list closed itself with a second pipe.
    /// Payload is one PAST that pipe.
    NodeInitial(usize),
    /// Legacy trailing (3.1; deprecated in 3.2, removed in 4): the list ran to
    /// the node's own terminator. Payload is AT the terminating backslash,
    /// which the list does not include.
    Trailing(usize),
    /// Not a list at all. Payload is the byte the scan refuted on — the
    /// caller resumes THERE, never at the pipe, since any pipe in between
    /// would refute on that identical byte.
    NotAList(usize),
}

/// Boundary: where a list starting at `pipe_at` ends, and which shape it is.
/// Escape-aware and BOUNDED TO THE LINE — that bound is the whole recovery
/// story, since a malformed list degrades to content without ever consuming
/// past its own newline. `front` (nothing but the marker and its folded
/// delimiter before the pipe) is the only thing that makes a second raw pipe a
/// terminator, and that one gate reproduces the spec's disambiguation:
///
/// ```text
/// \w|Jesus|\w*     NodeInitial   the closing pipe decides it
/// \w|Jesus\w*      Trailing      3.1 default-attribute form
/// \w a|b|c\w*      Trailing      after content, so interior pipes are bytes
/// \p |x="y"\n      NotAList      the line refutes it; bytes stay content
/// ```
///
/// The terminator is only checked for BEING a closer, never for matching the
/// open frame: lint owns the real stack, so `\add*` ending a `\w` list is a
/// finding, not a lexing decision.
///
/// VECTORIZED, and it has to be: alignment corpora are mostly attribute bytes
/// (`\zaln-s` lists run ~150 bytes each). Same two-scan shape the text arm
/// uses — three needles fit one `memchr3`, and the closing pipe rides a second
/// call BOUNDED to the first hit, since a pipe past the terminator could never
/// win the min.
pub(crate) fn attr_list_end(
    bytes: &[u8],
    pipe_at: usize,
    front: bool,
    first_stop: Option<usize>,
) -> AttrScan {
    let mut index = pipe_at + 1;
    // The text arm has ALREADY located the next `\`/CR/LF for its own run, and
    // in back position that byte is necessarily this scan's first stop too.
    // Reusing it saves one vectorized pass per list, and aligned corpora carry
    // one list per WORD.
    let mut known = first_stop;
    while index < bytes.len() {
        let pos = match known.take() {
            Some(stop) => stop,
            None => {
                let rest = &bytes[index..];
                let control = memchr3(BACKSLASH, CR, LF, rest);
                let bound = control.unwrap_or(rest.len());
                let pipe = if front {
                    memchr(PIPE, &rest[..bound])
                } else {
                    None
                };
                match [control, pipe].into_iter().flatten().min() {
                    Some(offset) => index + offset,
                    None => return AttrScan::NotAList(bytes.len()),
                }
            }
        };
        match bytes[pos] {
            CR | LF => return AttrScan::NotAList(pos),
            // Only ever searched for when `front`.
            PIPE => return AttrScan::NodeInitial(pos + 1),
            _ => match escape_len(bytes, pos) {
                // `\|` is load-bearing under U25001, where a raw pipe would
                // end the list.
                Some(len) => index = pos + len,
                // A real marker: this node's terminator, or a refutation.
                None => {
                    let slice = &bytes[pos..marker_end(bytes, pos)];
                    return match classify_marker(slice) {
                        TokenKind::ClosingMarker { .. } | TokenKind::MilestoneTerminator => {
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

/// Emits one `AttrList` token if the pipe at `pipe_at` opens a list. `Err(stop)`
/// carries where the caller should resume (see [`AttrScan::NotAList`]). Shared
/// by both callers — the dispatch loop for front position, the text arm for
/// back — so the two can never disagree about what a pipe means. `text_from` is
/// pending content, which must be emitted BEFORE the list to keep the stream in
/// source order; pass `pipe_at` for "nothing pending".
impl Scanner<'_> {
    #[inline(always)]
    fn try_attr_list(
        &mut self,
        pipe_at: usize,
        front: bool,
        text_from: usize,
        first_stop: Option<usize>,
    ) -> Result<usize, usize> {
        let (end, absorbs_trailing_ws) = match attr_list_end(self.bytes, pipe_at, front, first_stop)
        {
            // U25001's production puts `<HS>*` INSIDE the attribute_list, after
            // the closing pipe, so those bytes belong to the list. Re-arming
            // the delimiter fold is all it takes.
            AttrScan::NodeInitial(end) => (end, true),
            // A trailing list is followed by its closer, never by a delimiter.
            AttrScan::Trailing(end) => (end, false),
            // `\periph Title|id="x"` has NO closer — its list ends with the
            // LINE. A question about the open FRAME, not the bytes, so
            // `attr_list_end` stays a pure byte boundary and mode answers here.
            AttrScan::NotAList(stop)
                if self.mode.attr_list_ends_at_line
                    && matches!(self.bytes.get(stop), None | Some(&CR) | Some(&LF)) =>
            {
                (stop, false)
            }
            AttrScan::NotAList(stop) => return Err(stop),
        };
        if pipe_at > text_from {
            self.push_token(TokenKind::Text, text_from, pipe_at);
        }
        self.push_token(TokenKind::AttrList, pipe_at, end);
        self.mode.awaiting_delimiter_ws = absorbs_trailing_ws;
        // Nothing can be in front of a node twice.
        self.mode.after_marker = false;
        // `pending_payload` is deliberately UNTOUCHED: `\v|script="Arab"| 1`
        // still owes its designator; a list is not content that cancels it.
        Ok(end)
    }
}

// ---- text arm ---------------------------------------------------------------

/// The escaped-content forms the text arm folds — the TEXT escapes (`\/` `\~`
/// `\\` `\|`) plus the U25004 USV escapes, whose letter and fixed hex width
/// come from [`USV_ESCAPE_LETTERS`] (no terminator).
///
/// ```text
/// \~b        Some(2)    escape, so content
/// \u0041b    Some(6)    the exact USV pattern BEATS the marker claim
/// \u12 x     None       wrong width: stays a marker (row 0, lint's)
/// ```
///
/// Hex case is lint's business, never a rejection here.
pub(crate) fn escape_len(bytes: &[u8], pos: usize) -> Option<usize> {
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

/// Boundary: where a carved payload ends — the next structural stop. Shared by
/// designator, note caller and book code, since all three are "the run up to
/// the next structural byte": `1`, `12-14a`, `GEN`, `GENESIS`, `+`, junk alike
/// get one unvalidated span.
///
/// - The note caller's spec pattern is `/[^\\\s]+/`, which would admit a pipe;
///   stopping at one anyway is REQUIRED, because `\f |aid="x"| + …` is a
///   front-position attribute list, not a caller.
/// - Stopping at the first space is what leaves `\id GEN Some description`'s
///   description as ordinary Text.
///
/// Each then takes one more step, ONE byte of its delimiter run: the
/// whitespace after a `\c`/`\v` number, after a note caller and after a book
/// code is required grammar the way the whitespace after a marker NAME is, so
/// the module doc's second rule covers all three and `\v 1 text` carves
/// `Designator("1 ")` — with any surplus of the run emitted as Pad.
pub(crate) fn payload_end(bytes: &[u8], from: usize) -> usize {
    let mut index = from;
    while index < bytes.len() && !matches!(bytes[index], SPACE | TAB | CR | LF | BACKSLASH | PIPE) {
        index += 1;
    }
    index
}

/// The fold's inverse: a carved payload's span MINUS its one delimiter byte.
/// Every consumer that reads a payload's VALUE starts here.
///
/// ```text
/// "1 "    →  "1"      "GEN\t"  →  "GEN"    "+ "  →  "+"
/// "1"     →  "1"      a newline delimiter is its own token
/// ```
///
/// Safe on every payload because `payload_end` stops at the first horizontal
/// byte: only a folded delimiter can be at the tail.
pub(crate) fn payload_label(span: &[u8]) -> &[u8] {
    let mut end = span.len();
    while end > 0 && matches!(span[end - 1], SPACE | TAB) {
        end -= 1;
    }
    &span[..end]
}

impl Scanner<'_> {
    #[inline(always)]
    fn text_arm(&mut self, index: usize) -> usize {
        // Copied out ONCE so the slices below borrow the SOURCE (`'a`) rather
        // than `self`, which would hold a shared borrow across the pushes.
        let bytes = self.bytes;
        self.mode.awaiting_delimiter_ws = false;
        // Entering this arm IS content starting, so nothing past here can be in
        // front position — that is what reduces "front" to "the dispatch loop
        // found the pipe".
        self.mode.after_marker = false;
        // The first content region after a payload-owing marker IS that
        // payload. `Version` (`\usfm 3.0`) deliberately carves NOTHING: its
        // Text is already isolated by the line ending, and its only consumer
        // reads it off the adjacent marker.
        let payload_kind = match self.mode.pending_payload {
            // THE DESIGNATOR GATE: a chapter/verse number starts with an ASCII
            // digit, so `\v Then He declared` is ordinary Text, not a Designator
            // the interpreter has to reject. Digit-start-but-wrong (`\v 012`,
            // `\c 12b`) still tokenizes — the full grammar's judgment is
            // `crate::designator`'s, and it is richer there.
            Payload::Designator
                if !self.mode.designator_gated
                    || bytes.get(index).is_some_and(u8::is_ascii_digit) =>
            {
                Some(TokenKind::Designator)
            }
            Payload::Designator => None,
            Payload::NoteCaller => Some(TokenKind::NoteCaller),
            Payload::BookCode => Some(TokenKind::BookCode),
            Payload::None | Payload::Version => None,
        };
        self.mode.pending_payload = Payload::None;
        if let Some(kind) = payload_kind {
            let end = payload_end(bytes, index);
            // A region opening with an escape (`\v \~…`) has nothing to take —
            // fall through to the ordinary scan and emit no empty token.
            if end > index {
                // Every carved payload takes ONE delimiter byte with it
                // (see `payload_end`); the run's surplus is Pad.
                let run_end = ws_run_end(bytes, end);
                let keep = run_end.min(end + 1);
                self.push_token(kind, index, keep);
                if run_end > keep {
                    self.push_token(TokenKind::Pad, keep, run_end);
                }
                return run_end;
            }
        }
        // Separate from `cursor` because a `//` found mid-run ends the current
        // segment early (pushed as its own `OptBreak`) without ending the call.
        let mut segment_start = index;
        let mut cursor = index;

        // This function sees the bulk of a document's bytes, so it is the one
        // worth making SIMD. Four bytes matter (`\`, `\r`, `\n`, `/`), one more
        // than a single `memchr3` holds — so two vectorized scans per iteration
        // taking whichever hit comes first, rather than one scan that quietly
        // drops `\r` and would then tear `\r\n` in half.
        loop {
            let rest = &bytes[cursor..];
            let control = memchr3(BACKSLASH, CR, LF, rest);
            // BOUNDED to the control hit — `//` cannot contain a control byte,
            // so a hit past it could never win the min. Unbounded, every text
            // region scans to END OF FILE for a rare-to-absent needle, which
            // makes the whole lex quadratic (measured 152ms for 66 books).
            let bound = control.unwrap_or(rest.len());
            // Called through `self` rather than held in a local: a
            // `&self.opt_break_finder` binding would live across the pushes.
            let opt_break = self.opt_break_finder.find(&rest[..bound]);
            // The MODE-SWAPPED needle: a pipe is a stop only while an
            // attrs-capable frame is open on this line. Off, this costs one
            // already-loaded flag test; on, one more vectorized pass over a
            // frame's worth of bytes.
            let pipe = if self.mode.attr_frames > 0 {
                memchr(PIPE, &rest[..bound])
            } else {
                None
            };
            let Some(offset) = [control, opt_break, pipe].into_iter().flatten().min() else {
                cursor = bytes.len();
                break;
            };
            let pos = cursor + offset;

            match bytes[pos] {
                // An escape keeps the run going; any other backslash is a marker.
                BACKSLASH => match escape_len(bytes, pos) {
                    Some(len) => cursor = pos + len,
                    None => {
                        cursor = pos;
                        break;
                    }
                },
                // A BACK-position attribute list (`\w gracious|lemma="x"\w*`):
                // there is content before the pipe by construction, so `front`
                // is false and node-initial can never fire from this path.
                PIPE => {
                    match self.try_attr_list(pos, false, segment_start, control.map(|c| cursor + c))
                    {
                        Ok(end) => return end,
                        // RESUME AT THE REFUTATION, not at `pos + 1`: in back
                        // position a pipe never terminates a scan, so every
                        // pipe in between refutes on that identical byte, and
                        // restarting per pipe is quadratic on a pipe-dense line.
                        Err(stop) => cursor = stop,
                    }
                }
                // A confirmed `//` — split the output into two Text segments.
                SLASH => {
                    if pos > segment_start {
                        self.push_token(TokenKind::Text, segment_start, pos);
                    }
                    self.push_token(TokenKind::OptBreak, pos, pos + 2);
                    segment_start = pos + 2;
                    cursor = pos + 2;
                }
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

    const MILESTONE: TokenKind = TokenKind::Milestone { end: false };
    const MS_END: TokenKind = TokenKind::MilestoneTerminator;
    const TEXT: TokenKind = TokenKind::Text;
    const ATTRS: TokenKind = TokenKind::AttrList;
    const CALLER: TokenKind = TokenKind::NoteCaller;
    const BOOK: TokenKind = TokenKind::BookCode;

    fn kinds_and_ranges(tokens: &[Token]) -> Vec<(TokenKind, usize, usize)> {
        tokens
            .iter()
            .map(|t| (t.kind(), t.start as usize, t.end() as usize))
            .collect()
    }

    /// Kinds paired with the bytes they cover. Preferred wherever the SPANS are
    /// the point: a wrong boundary shows up as wrong text instead of an
    /// off-by-one to decode, and concatenating the column is the partition.
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

    /// The region after `\c`/`\v` is ONE Designator token — digits, ranges,
    /// junk alike — plus one byte of the delimiter behind it (surplus is
    /// Pad); a designator-less `\v` emits nothing extra.
    #[test]
    fn chapter_and_verse_take_a_designator_token() {
        assert_eq!(
            kinds_and_text("\\v 1 text"),
            vec![
                (MARKER, "\\v "),
                (TokenKind::Designator, "1 "),
                (TokenKind::Text, "text"),
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
            kinds_and_text("\\v 12-14a x")[1],
            (TokenKind::Designator, "12-14a ")
        );
        // The fold takes ONE horizontal code unit; the run's surplus is Pad.
        assert_eq!(
            &kinds_and_text("\\v 1  \tx")[1..3],
            [(TokenKind::Designator, "1 "), (TokenKind::Pad, " \t")]
        );
        assert_eq!(
            kinds_and_text("\\v 1\ntext"),
            vec![
                (MARKER, "\\v "),
                (TokenKind::Designator, "1"),
                (TokenKind::Newline, "\n"),
                (TokenKind::Text, "text"),
            ]
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
        assert_eq!(
            kinds_and_ranges(&lex("\\p 12 x")),
            vec![(MARKER, 0, 3), (TokenKind::Text, 3, 7)]
        );
    }

    /// The RFC's load-bearing shape (`GAP v: WS+ both positions`): both
    /// delimiter gaps keep one byte and pad the rest, the designator still
    /// attaches across the surplus, and the spans tile.
    #[test]
    fn surplus_at_both_verse_gaps_is_pad() {
        let source = "\\v       1     Text";
        assert_eq!(
            kinds_and_text(source),
            vec![
                (MARKER, "\\v "),
                (TokenKind::Pad, "      "),
                (TokenKind::Designator, "1 "),
                (TokenKind::Pad, "    "),
                (TokenKind::Text, "Text"),
            ]
        );
        let concat: String = lex(source)
            .iter()
            .map(|t| &source[t.start as usize..t.end() as usize])
            .collect();
        assert_eq!(concat, source);
    }

    /// THE DESIGNATOR GATE, byte for byte. One leading ASCII digit is the whole
    /// test: `[1-9]`, segments and ranges are the interpreter's
    /// ([`crate::designator`]), so `\v 012` must still reach it.
    #[test]
    fn a_designator_token_requires_a_leading_digit() {
        // Digit-start: unchanged, wellformed or not.
        assert_eq!(
            kinds_and_text("\\v 2b text")[1],
            (TokenKind::Designator, "2b ")
        );
        assert_eq!(
            kinds_and_text("\\v 012 text")[1],
            (TokenKind::Designator, "012 ")
        );
        assert_eq!(kinds_and_text("\\c 12b")[1], (TokenKind::Designator, "12b"));
        // Prose after `\v `: Marker + Text, no designator token — the same
        // shape `\v \p` and `\v ⏎` already had.
        assert_eq!(
            kinds_and_text("\\v Then He declared"),
            vec![(MARKER, "\\v "), (TokenKind::Text, "Then He declared")]
        );
        assert_eq!(
            kinds_and_text("\\c Chapter One"),
            vec![(MARKER, "\\c "), (TokenKind::Text, "Chapter One")]
        );
        // The gate is `\c`/`\v`'s alone: `\cp`/`\va` carry PUBLISHED labels,
        // which are letters by design.
        assert_eq!(kinds_and_text("\\cp M\n")[1], (TokenKind::Designator, "M"));
        assert_eq!(
            kinds_and_text("\\vp א\\vp*")[1],
            (TokenKind::Designator, "א")
        );
    }

    /// The fold is SHAPE first, then the row read permissively —
    /// `folds_delimiter` is the whole rule.
    #[test]
    fn only_delimiter_taking_markers_absorb_whitespace() {
        // The closer's space is CONTENT: shape decides, and the row shared with
        // the opener says nothing.
        assert_eq!(
            kinds_and_ranges(&lex("\\w* x")),
            vec![(CLOSING, 0, 3), (TokenKind::Text, 3, 5)]
        );
        // Unresolved `\zaln-s` (row 0) DOES fold, which is what puts a list's
        // pipe at a region start.
        assert_eq!(
            kinds_and_ranges(&lex("\\zaln-s x")),
            vec![
                (TokenKind::Milestone { end: false }, 0, 8),
                (TokenKind::Text, 8, 9)
            ]
        );
        // Known milestones fold too: "optional HS" means permitted, not content.
        assert_eq!(
            kinds_and_ranges(&lex("\\qt-s x"))[0],
            (TokenKind::Milestone { end: false }, 0, 6)
        );
        // `\b` is the one abstainer: SingleNewline, so the space stays content.
        assert_eq!(
            kinds_and_ranges(&lex("\\b x")),
            vec![(MARKER, 0, 2), (TokenKind::Text, 2, 4)]
        );
        // A newline is NEVER folded in, even where the row permits it as the
        // delimiter (`\v`): Newline tokens are structurally load-bearing.
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
            vec![(TokenKind::Milestone { end: false }, 0, 7)]
        );
    }

    #[test]
    fn bare_star_is_milestone_end_not_a_nameless_closing_marker() {
        assert_eq!(
            kinds_and_ranges(&lex("\\*")),
            vec![(TokenKind::MilestoneTerminator, 0, 2)]
        );
    }

    /// FRONT position: the pipe sits at a region start because the marker's
    /// delimiter was folded into its span. One span per list, pipes included.
    #[test]
    fn front_position_attribute_lists() {
        assert_eq!(
            kinds_and_text("\\zaln-s |x-strong=\"G46130\"\\*"),
            vec![
                (MILESTONE, "\\zaln-s "),
                (ATTRS, "|x-strong=\"G46130\""),
                (MS_END, "\\*"),
            ]
        );
        // Node-initial (U25001): per the grammar's trailing `<HS>*` the
        // delimiter space belongs to the LIST, not to the content.
        assert_eq!(
            kinds_and_text("\\p|cat=\"emphasised\"| text"),
            vec![
                (MARKER, "\\p"),
                (ATTRS, "|cat=\"emphasised\"| "),
                (TEXT, "text")
            ]
        );
        // U25001 writes a space before the pipe though its own production
        // disallows one; the fold absorbs it, so both spellings lex the same.
        assert_eq!(
            kinds_and_text("\\f |aid=\"mynote\"| +")[1],
            (ATTRS, "|aid=\"mynote\"| ")
        );
        // A `\v`'s designator SURVIVES its attribute list.
        assert_eq!(
            kinds_and_text("\\v|script=\"Arab\"| 1 x"),
            vec![
                (MARKER, "\\v"),
                (ATTRS, "|script=\"Arab\"| "),
                (TokenKind::Designator, "1 "),
                (TEXT, "x"),
            ]
        );
        // The spec hangs node-initial vs 3.1-trailing on the closing pipe alone.
        assert_eq!(kinds_and_text("\\w|Jesus|\\w*")[1], (ATTRS, "|Jesus|"));
        assert_eq!(kinds_and_text("\\w|Jesus\\w*")[1], (ATTRS, "|Jesus"));
        assert_eq!(kinds_and_text("\\w|\\w*")[1], (ATTRS, "|"));
        assert_eq!(
            kinds_and_text("\\fig |src=\"a.png\" size=\"col\"\\fig*")[1],
            (ATTRS, "|src=\"a.png\" size=\"col\"")
        );
        // An escaped pipe is a value byte, not a terminator.
        assert_eq!(kinds_and_text("\\w|a\\|b|\\w*")[1], (ATTRS, "|a\\|b|"));
    }

    /// BACK position: the list trails real content, found by the mode-swapped
    /// pipe needle. Deprecated in 3.2 and removed in 4, yet recognized
    /// unconditionally — the declared version is lint's, not the lexer's.
    #[test]
    fn back_position_attribute_lists() {
        assert_eq!(
            kinds_and_text("\\w gracious|lemma=\"grace\"\\w*"),
            vec![
                (MARKER, "\\w "),
                (TEXT, "gracious"),
                (ATTRS, "|lemma=\"grace\""),
                (CLOSING, "\\w*"),
            ]
        );
        assert_eq!(kinds_and_text("\\w Jésus|Jesus\\w*")[2], (ATTRS, "|Jesus"));
        // Legal though absurd: TWO lists on one node, both kept in source order
        // so passthrough is byte-identical. "Later definition wins" is the
        // interpreter's merge rule, never the lexer's.
        assert_eq!(
            kinds_and_text("\\w |Fred|Jésus|Jesus\\w*"),
            vec![
                (MARKER, "\\w "),
                (ATTRS, "|Fred|"),
                (TEXT, "Jésus"),
                (ATTRS, "|Jesus"),
                (CLOSING, "\\w*"),
            ]
        );
        // Interior pipes are span BYTES: node-initial is ineligible once
        // content has been seen.
        assert_eq!(kinds_and_text("\\w a|b|c\\w*")[2], (ATTRS, "|b|c"));
        // `add` defines no attributes at all, yet lexes identically — the shape
        // set never depends on the row; AttrStatus is lint's.
        assert_eq!(
            kinds_and_text("\\add x|k=\"v\"\\add*")[2],
            (ATTRS, "|k=\"v\"")
        );
        // A fused-arm marker (`\ft`) arms the needle too, via `Hot::attrs_frame`.
        assert_eq!(
            kinds_and_text("\\ft x|k=\"v\"\\ft*")[2],
            (ATTRS, "|k=\"v\"")
        );
    }

    /// The note caller: one span after the delimiter, spec pattern `/[^\\\s]+/`,
    /// so `+`/`-`/`?` are conventional values, never an enumeration. The
    /// delimiter the grammar requires after it rides the span.
    #[test]
    fn notes_and_cross_references_carve_their_caller() {
        assert_eq!(
            kinds_and_text("\\f + \\ft text\\f*"),
            vec![
                (MARKER, "\\f "),
                (CALLER, "+ "),
                (MARKER, "\\ft "),
                (TEXT, "text"),
                (CLOSING, "\\f*"),
            ]
        );
        // The case that justifies the token kind: without carving, the caller
        // and bare content would share ONE Text token.
        assert_eq!(
            kinds_and_text("\\f - bare note\\f*"),
            vec![
                (MARKER, "\\f "),
                (CALLER, "- "),
                (TEXT, "bare note"),
                (CLOSING, "\\f*"),
            ]
        );
        assert_eq!(
            kinds_and_text("\\f ?custom \\ft x\\f*")[1],
            (CALLER, "?custom ")
        );
        // Cross-references owe one too (`x`, and `fe`/`ef`/`ex` alike).
        assert_eq!(kinds_and_text("\\x - \\xo 1.1\\x*")[1], (CALLER, "- "));
        // A front-position list comes FIRST, and the caller expectation must
        // survive it.
        assert_eq!(
            kinds_and_text("\\f |aid=\"n1\"| + \\ft x\\f*")[..3],
            [(MARKER, "\\f "), (ATTRS, "|aid=\"n1\"| "), (CALLER, "+ ")]
        );
        // Nothing to take: no empty token.
        assert_eq!(
            kinds_and_text("\\f\n"),
            vec![(MARKER, "\\f"), (TokenKind::Newline, "\n")]
        );
    }

    /// `\id`'s book code stops at the first space, leaving the description as
    /// content. Nothing is validated, because the spec contradicts itself: its
    /// prose says "a standard 3-character identifier" while its published
    /// pattern admits none of the 27 digit-leading codes like `1JN`.
    #[test]
    fn id_carves_the_book_code_and_leaves_the_description() {
        assert_eq!(
            kinds_and_text("\\id GEN Some description\n"),
            vec![
                (MARKER, "\\id "),
                (BOOK, "GEN "),
                (TEXT, "Some description"),
                (TokenKind::Newline, "\n"),
            ]
        );
        assert_eq!(kinds_and_text("\\id 1JN\n")[1], (BOOK, "1JN"));
        // Wrong shape is still ONE span — the anchor lint wants.
        assert_eq!(kinds_and_text("\\id GENESIS\n")[1], (BOOK, "GENESIS"));
        assert_eq!(kinds_and_text("\\id gen\n")[1], (BOOK, "gen"));
        // No code at all: no empty token.
        assert_eq!(
            kinds_and_text("\\id\n"),
            vec![(MARKER, "\\id"), (TokenKind::Newline, "\n")]
        );
    }

    /// `\usfm 3.0` carves NOTHING: the version string is already isolated by the
    /// line ending, and its only consumer reads it off the adjacent marker.
    #[test]
    fn the_version_marker_carves_no_payload() {
        assert_eq!(
            kinds_and_text("\\usfm 3.0\n"),
            vec![
                (MARKER, "\\usfm "),
                (TEXT, "3.0"),
                (TokenKind::Newline, "\n"),
            ]
        );
    }

    /// A Byte Order Mark is ALLOWED at the start of a file, so nothing should
    /// flag it: it lexes as three bytes of Text, keeping the partition exact.
    ///
    /// The consequence downstream: `\id` is NOT guaranteed to be the first
    /// token, so find it by SEARCHING for the marker, never by indexing token 0.
    #[test]
    fn a_leading_byte_order_mark_is_content() {
        assert_eq!(
            kinds_and_text("\u{FEFF}\\id GEN\n"),
            vec![
                (TEXT, "\u{FEFF}"),
                (MARKER, "\\id "),
                (TokenKind::BookCode, "GEN"),
                (TokenKind::Newline, "\n"),
            ]
        );
    }

    /// The three INTERIOR syntaxes the spec defines inside an attribute list —
    /// unnamed default value, comma-separated values, colon-separated compound
    /// parts — are invisible here: none of `,` `:` `=` `"` or space is a stop.
    /// And the spec's "whitespace adjacent to the comma separators is ignored"
    /// is a NORMALIZATION rule, so the span keeps `"a, b"` byte-for-byte and
    /// trimming belongs to whoever asks for the values.
    #[test]
    fn interior_attribute_syntax_is_one_span() {
        // The default attribute: a bare value, no `=`. Binding it to the row's
        // `default_attribute` is the interpreter's job.
        assert_eq!(
            kinds_and_text("\\w gracious|grace\\w*")[2],
            (ATTRS, "|grace")
        );
        assert_eq!(
            kinds_and_text("\\w x|lemma=\"a, b\" strong=\"H1,H2\"\\w*")[2],
            (ATTRS, "|lemma=\"a, b\" strong=\"H1,H2\"")
        );
        assert_eq!(
            kinds_and_text("\\w x|x-content=\"alpha:beta\"\\w*")[2],
            (ATTRS, "|x-content=\"alpha:beta\"")
        );
        // A whole real alignment word: several pairs, ONE span.
        assert_eq!(
            kinds_and_text("\\w In|x-occurrence=\"1\" x-occurrences=\"1\"\\w*"),
            vec![
                (MARKER, "\\w "),
                (TEXT, "In"),
                (ATTRS, "|x-occurrence=\"1\" x-occurrences=\"1\""),
                (CLOSING, "\\w*"),
            ]
        );
    }

    /// The ladder is QUOTE-BLIND on purpose, so delimiters win over quoting.
    /// Documented because it looks like a bug: the spec gives attribute values
    /// no escape mechanism, `\|` covers the one case that needs escaping, and
    /// U25001 depends on the pipe being a hard delimiter — so quote state would
    /// tax a hot scan purely to serve already-non-conforming strings. Failure
    /// mode is a truncated span plus a lint finding; every byte stays.
    #[test]
    fn the_ladder_does_not_track_quotes() {
        // A raw pipe inside a value ends a front-position list early…
        assert_eq!(
            kinds_and_text("\\w|lemma=\"a|b\"|\\w*")[1],
            (ATTRS, "|lemma=\"a|")
        );
        // …which is exactly what `\|` is for.
        assert_eq!(
            kinds_and_text("\\w|lemma=\"a\\|b\"|\\w*")[1],
            (ATTRS, "|lemma=\"a\\|b\"|")
        );
    }

    /// The needle is MODE-SWAPPED: with no attrs-capable frame open a pipe is not
    /// a stop at all, so the run never splits. Asserting the whole span is the
    /// point — a globally-armed needle would pass a kinds-only check.
    #[test]
    fn the_pipe_needle_is_off_outside_a_character_frame() {
        assert_eq!(kinds_and_text("\\p a|b|c d\n")[1], (TEXT, "a|b|c d"));
        // A note frame takes the front form only, so it does NOT arm the needle.
        assert_eq!(kinds_and_text("\\f + x|y\\f*")[2], (TEXT, "x|y"));
        // Closing the frame disarms it again.
        assert_eq!(kinds_and_text("\\w a\\w* b|c\n")[3], (TEXT, " b|c"));
    }

    /// The needle's line bound as behavior: a character frame whose opener and
    /// trailing list sit on DIFFERENT lines is NOT read as a list. Documented
    /// limitation (see `ScanState::attr_frames`), lossless either way.
    #[test]
    fn a_frame_does_not_carry_the_needle_across_a_newline() {
        assert_eq!(
            kinds_and_text("\\w gracious\nmore|lemma=\"x\"\\w*"),
            vec![
                (MARKER, "\\w "),
                (TEXT, "gracious"),
                (TokenKind::Newline, "\n"),
                (TEXT, "more|lemma=\"x\""),
                (CLOSING, "\\w*"),
            ]
        );
    }

    /// A pipe that opens no list was never a delimiter: no token of its own, no
    /// repair, and NOTHING consumed past its own line — that bound is the whole
    /// recovery story.
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
        // EOF refutes it too; a bare pipe after a milestone is just Text.
        assert_eq!(
            kinds_and_text("\\zaln-s|"),
            vec![(MILESTONE, "\\zaln-s"), (TEXT, "|")]
        );
        assert_eq!(kinds_and_text("a|b"), vec![(TEXT, "a|b")]);
    }

    #[test]
    fn keeps_escaped_delimiters_as_text() {
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
        // Wrong width: a MARKER, unresolved, so it folds its delimiter space.
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

    /// Marker tokens carry their table row; every other spelling fact lives in
    /// the span. Asserted by name round-trip, so row reordering can't break it.
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
        // The one overloaded name resolves per shape: both spellings name qt,
        // but they are different rows.
        let plain = lex("\\qt x")[0].marker_idx;
        let milestone = lex("\\qt-s x")[0].marker_idx;
        assert_ne!(plain, milestone);
        assert_eq!(generated::name(plain), "qt");
        assert_eq!(generated::name(milestone), "qt");
    }
}
