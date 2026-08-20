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

use memchr::memchr;
use memchr::memchr3;
use memchr::memmem;

use crate::tables::generated;
use crate::tables::schema::{
    MarkerKind, Numbering, Payload, SpellingShape, StructuralWhitespaceRequirement as Ws,
    USV_ESCAPE_LETTERS,
};
use crate::token::{Token, TokenKind};

// Named once so every match arm/peek reads as "is this a marker-start"
// instead of a bare `b'\\'` scattered across every function.
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
    // True right after emitting a marker whose row takes a structural
    // delimiter, until the one whitespace run that delimits it has been
    // consumed (read off `ws_after_name`).
    pub(crate) awaiting_delimiter_ws: bool,
    // True right after emitting a marker whose row consumes a Designator
    // payload (`\c`/`\v`), until the next region: text becomes
    // ONE Designator token; anything else (newline, marker) drops the
    // expectation — a `\v` with no number emits no empty token.
    // What payload the marker just emitted still owes, straight off its row —
    // `Payload::None` when nothing is pending. Carrying the row's own enum
    // rather than a bool per payload keeps one code path for all three carved
    // payloads (`\c`/`\v` designators, note callers, the `\id` book code): the
    // text arm maps it to a token kind and clears it. The expectation dies at
    // the next region if nothing is there to take, so a payload-less `\v` or
    // `\id` emits no empty token.
    pub(crate) pending_payload: Payload,
    // True from the moment an opener/milestone token is emitted until ANY
    // other token is. Its one job is deciding whether a pipe is in FRONT
    // position (U25001: attributes precede content), and because the text
    // arm clears it on entry, "front position" reduces to "the dispatch loop
    // found this pipe" — no byte-scanning predicate anywhere.
    pub(crate) after_marker: bool,
    // How many attrs-capable character frames are open ON THIS LINE. Nonzero
    // is the ONLY condition under which the text arm looks for a pipe, which
    // is what keeps the deprecated back-position form from taxing the stop
    // set globally (stop density is the measured wall).
    //
    // Reset to 0 at every newline, which BOUNDS what a stale count can cost
    // to one line. The honest limitation that buys: a character frame whose
    // opener and trailing list sit on DIFFERENT lines
    // (`\w gracious\nmore|lemma="x"\w*`) is not recognized as a list — the
    // bytes stay content and lint reports the pipe. Lossless either way, and
    // no corpus book contains the shape; the alternative (never resetting)
    // leaves the needle armed for the rest of the document after a single
    // unclosed `\w`, which is the worse failure since prose is full of
    // character markers.
    pub(crate) attr_frames: u8,
    // The one row whose attribute list is terminated by the LINE rather than
    // by a closer: `\periph My Title|id="x"` (usx.rng's
    // `PeripheralDivision`, testData advanced/periph). Set with the arming
    // above, cleared with it at every newline, and read ONLY by
    // `try_attr_list` — the byte-level boundary scan stays closer-shaped.
    pub(crate) attr_list_ends_at_line: bool,
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
/// state discipline every downstream pass follows (cst/lint read only the
/// token rows and source bytes, never scanner internals), applied inward —
/// and it is what keeps `lex_general_path_only` a
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
    mode: ScanState,
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
            mode: ScanState {
                awaiting_delimiter_ws: false,
                pending_payload: Payload::None,
                after_marker: false,
                attr_frames: 0,
                attr_list_ends_at_line: false,
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
pub(crate) struct Hot {
    pub(crate) idx: generated::MarkerIdx,
    pub(crate) folds: bool,
    pub(crate) level_max: u8,
    /// Does opening this marker arm the text arm's pipe needle? True for the
    /// character-class hot rows (`ft`, `fr`, `xt`), false for the rest — the
    /// same `MarkerKind` test the general path runs, precomputed so no fast
    /// arm ever reads the table.
    pub(crate) attrs_frame: bool,
    /// The payload this row owes after its delimiter — `NoteCaller` for `\f`,
    /// `Designator` for `\v`, `None` for the rest. Precomputed for the same
    /// reason as the others: an arm must owe EXACTLY what the general path
    /// owes, and `fast_path_identity` is what proves it.
    pub(crate) payload: Payload,
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
                // fixed here — but it is the SAME predicate the general path
                // runs, which is what keeps `fast_path_identity` honest.
                folds: folds_delimiter(TokenKind::Marker { nested: false }, idx),
                level_max: match generated::numbering(idx) {
                    Numbering::UpTo(cap) => cap,
                    _ => 0,
                },
                attrs_frame: opens_attrs_frame(idx),
                payload: generated::payload(idx),
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
                self.mode.pending_payload = Payload::None;
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
                self.mode.pending_payload = Payload::None;
                // Every hot marker is an opener, so a pipe right after the folded
                // delimiter is in front position — same as the general path.
                self.mode.after_marker = true;
                // `\f` owes its note caller here exactly as the general path does.
                self.mode.pending_payload = hot.payload;
                if hot.attrs_frame {
                    self.mode.attr_frames = self.mode.attr_frames.saturating_add(1);
                    // No hot row is a Periph, so the line-end shape cannot
                    // arm here — asserted rather than assumed.
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
                // Still true here, and still correct: the space is content, so the
                // text arm clears the flag before any pipe can be reached.
                self.mode.after_marker = true;
                self.mode.pending_payload = hot.payload;
                if hot.attrs_frame {
                    self.mode.attr_frames = self.mode.attr_frames.saturating_add(1);
                    // No hot row is a Periph, so the line-end shape cannot
                    // arm here — asserted rather than assumed.
                    debug_assert_ne!(generated::kind(hot.idx), MarkerKind::Periph);
                }
                Some(name_end)
            }
            Some(&CR | &LF) => {
                // Marker + its line ending (`\n` or `\r\n`) in one hit.
                self.push_marker(index, name_end, hot.idx);
                let end = newline_end(self.bytes, name_end);
                self.push_token(TokenKind::Newline, name_end, end);
                self.mode.awaiting_delimiter_ws = false;
                self.mode.pending_payload = Payload::None;
                // This arm ends on the Newline token, not the marker.
                self.mode.after_marker = false;
                // The line ended, so no frame this marker opened can still be
                // collecting content on it — same reset the newline arm does.
                self.mode.attr_frames = 0;
                self.mode.attr_list_ends_at_line = false;
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
/// Does opening this row put an attrs-capable frame on the line — i.e. arm the
/// text arm's pipe needle so a BACK-position (pre usfm 3.2 legacy trailing) list can be
/// found inside a text run?
///
/// Character, Figure and Periph only. `\periph` is the one PARAGRAPH-shaped
/// marker whose attributes come AFTER its content — `\periph My Title|id="x"`
/// (usx.rng's `PeripheralDivision`: the title is the `alt` attribute, the
/// pipe carries `id`) — so it needs the needle exactly the way `\w` does
/// (testData advanced/periph). It costs one line: the count resets at the
/// newline, and a `\periph` line is a title.
///
/// Deliberately NOT:
/// - **milestones**, which have no content, so their pipe is always at a region
///   start and the dispatch arm already sees it;
/// - **notes, paragraphs, verses**, which take the U25001 front form only —
///   also a region start, also no needle;
/// - **row 0**, since arming for every unconfigured `\z` marker would put the
///   needle over custom paragraph content for nothing.
///
/// One predicate, shared with `Hot::attrs_frame`, so the fast and general
/// paths cannot disagree about when the needle is live.
pub(crate) fn opens_attrs_frame(idx: generated::MarkerIdx) -> bool {
    matches!(
        generated::kind(idx),
        MarkerKind::Character | MarkerKind::Figure | MarkerKind::Periph
    )
}

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
pub(crate) fn ws_run_end(bytes: &[u8], from: usize) -> usize {
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
pub(crate) fn newline_end(bytes: &[u8], from: usize) -> usize {
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
        self.mode.pending_payload = Payload::None;
        self.mode.after_marker = false;
        // Attribute lists never span a line, so the needle is disarmed here —
        // the bound on what a stale count can cost.
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
/// closing `*`) purely to find the END — the shape decision is re-derived
/// from the slice by `classify_marker`, cursor-free.
pub(crate) fn marker_end(bytes: &[u8], start: usize) -> usize {
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
        // No name at all before the `*` — closes a milestone span
        // (`\zaln-s ... \*`), not a named closing marker like `\it*`.
        return TokenKind::MilestoneTerminator;
    }
    if slice.get(after_name) == Some(&HYPHEN) {
        // `\zaln-s`, `\qt-e` — hyphen after the name is the milestone form;
        // the suffix is exactly one byte (`marker_end` bounds it), and `e`
        // is the end spelling. Anything else (`-s` included) is an opener.
        return TokenKind::Milestone {
            end: slice.get(after_name + 1) == Some(&b'e'),
        };
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
        self.mode.after_marker =
            matches!(kind, TokenKind::Marker { .. } | TokenKind::Milestone { .. });
        // Does this row owe a carved payload (`\c`/`\v` designator, a note
        // caller, `\id`'s book code)? Straight off the row; the assignment
        // doubles as clearing any stale expectation. Only OPENERS owe one — a
        // closer's row is shared with its opener and would otherwise re-arm it.
        self.mode.pending_payload = if matches!(kind, TokenKind::Marker { .. }) {
            generated::payload(idx)
        } else {
            Payload::None
        };
        // Attrs-capable frame bookkeeping — see `ScanMode::attr_frames`. Any
        // closer decrements, even a mismatched one: the count only gates a
        // needle, so being approximately right costs at most one refuted
        // ladder, and pairing closers to openers is the walker's job.
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

/// What one pipe turned out to be. The three rungs of the ladder, and the
/// ONLY three things a raw pipe can mean.
pub(crate) enum AttrScan {
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
pub(crate) fn attr_list_end(
    bytes: &[u8],
    pipe_at: usize,
    front: bool,
    first_stop: Option<usize>,
) -> AttrScan {
    let mut index = pipe_at + 1;
    // The text arm reaches this having ALREADY located the next `\`/CR/LF for
    // its own run, and in back position that byte is necessarily this scan's
    // first stop too (no control byte can sit between the pipe and it, and a
    // pipe never stops a back-position scan). Taking it instead of re-deriving
    // it removes one vectorized pass per list — which matters because
    // word-aligned corpora carry one list per WORD.
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
                    // No terminator anywhere ahead: never a list.
                    None => return AttrScan::NotAList(bytes.len()),
                }
            }
        };
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

/// Emits one `AttrList` token if the pipe at `pipe_at` opens a list.
///
/// `Ok(cursor)` = a list was recognized and emitted. `Err(stop)` = it was
/// not; `stop` is where the caller should resume (see [`AttrScan::NotAList`]).
/// Shared by both callers — the dispatch loop for front position, the text arm
/// for back position — so the two can never disagree about what a pipe means.
///
/// `text_from` is where content pending emission begins, which for the text
/// arm is its open segment: a back-position list has content before it, and
/// that content must be emitted BEFORE the list to keep the stream in source
/// order (order is the losslessness guarantee, and the partition oracle fails
/// loudly otherwise). Pass `pipe_at` for "nothing pending", as front position
/// always does.
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
            // the closing pipe — so those bytes belong to the list. Re-arming the
            // delimiter fold is all it takes: `whitespace_arm` extends the last
            // token, which is this one.
            AttrScan::NodeInitial(end) => (end, true),
            // A trailing list is followed by its closer, never by a delimiter.
            AttrScan::Trailing(end) => (end, false),
            // `\periph Title|id="x"` has NO closer to be followed by — its
            // list ends with the LINE (usx.rng's `PeripheralDivision`). That
            // is a question about the open FRAME, not about the bytes, so
            // `attr_list_end` (pure byte boundary) stays as it is and the mode
            // answers it here. Any other refutation is still a refutation.
            AttrScan::NotAList(stop)
                if self.mode.attr_list_ends_at_line
                    && matches!(self.bytes.get(stop), None | Some(&CR) | Some(&LF)) =>
            {
                (stop, false)
            }
            AttrScan::NotAList(stop) => return Err(stop),
        };
        // The content this list trails, if any, goes out FIRST.
        if pipe_at > text_from {
            self.push_token(TokenKind::Text, text_from, pipe_at);
        }
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
/// all three (designator, note caller, book code), because all three are "the
/// run of bytes up to the next structural byte" and none is validated here.
///
/// The interior is NEVER parsed: `1`, `12-14a`, `GEN`, `GENESIS`, `+`, junk
/// alike get one span, and the interpreter or lint judges the content. Two
/// specifics worth knowing:
///
/// - The note caller's spec pattern is `/[^\\\s]+/`, which would admit a pipe;
///   stopping at one anyway is REQUIRED, because `\f |aid="x"| + …` is U25001's
///   front-position attribute list, not a caller. (In practice that pipe sits at
///   a region start and the dispatch arm takes it before this is reached.)
/// - Stopping at the first space is what makes `\id GEN Some description` give
///   the CODE as the payload and leave the description as ordinary Text.
pub(crate) fn payload_end(bytes: &[u8], from: usize) -> usize {
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
        // The first content region after a payload-owing marker IS that payload
        // — one token, then this arm is done; whatever follows re-enters the
        // loop as ordinary text. `Version` (`\usfm 3.0`) deliberately carves
        // NOTHING: its Text token is already isolated by the line ending, and
        // its only consumer reads it off the adjacent marker.
        let payload_kind = match self.mode.pending_payload {
            Payload::Designator => Some(TokenKind::Designator),
            Payload::NoteCaller => Some(TokenKind::NoteCaller),
            Payload::BookCode => Some(TokenKind::BookCode),
            Payload::None | Payload::Version => None,
        };
        self.mode.pending_payload = Payload::None;
        if let Some(kind) = payload_kind {
            let end = payload_end(bytes, index);
            // A region opening with an escape (`\v \~…`) has nothing to take —
            // fall through to the ordinary scan, same as a payload-less `\v`,
            // and emit no empty token.
            if end > index {
                self.push_token(kind, index, end);
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
            // The MODE-SWAPPED needle: a pipe is a stop only while an
            // attrs-capable character frame is open on this line, i.e. only
            // where the deprecated back-position list can legally be. Bounded
            // to the control hit for the same reason `//` is. Off, this costs
            // one already-loaded flag test; on, it costs one more vectorized
            // pass over a frame's worth of bytes (a word or two, typically).
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
                // A BACK-position attribute list (`\w gracious|lemma="x"\w*`):
                // there is content before the pipe by construction, which is
                // what back position means — so `front` is false here, and rung
                // 1 can never fire from this path (an interior pipe in
                // `\w a|b|c\w*` stays a span byte with no extra gate).
                PIPE => {
                    match self.try_attr_list(pos, false, segment_start, control.map(|c| cursor + c))
                    {
                        // The open segment was emitted by `try_attr_list`, before
                        // the list, so this call is done.
                        Ok(end) => return end,
                        // Not a list. RESUME AT THE REFUTATION, not at `pos + 1`:
                        // the stop is either this line's newline or a raw
                        // non-closer backslash, and in back position a pipe never
                        // terminates a scan — so every pipe in between would refute
                        // on that identical byte. Resuming at `pos + 1` instead
                        // re-scans to the same stop for each of them, which is
                        // quadratic on a pipe-dense line. The pipe stays ordinary
                        // content inside this same Text run: no token, no split.
                        Err(stop) => cursor = stop,
                    }
                }
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
            vec![
                (TokenKind::Milestone { end: false }, 0, 8),
                (TokenKind::Text, 8, 9)
            ]
        );
        // Known milestones fold too — their rows are OptionalHorizontalWhitespace,
        // and "optional" means the HS is permitted, not that it is content.
        assert_eq!(
            kinds_and_ranges(&lex("\\qt-s x"))[0],
            (TokenKind::Milestone { end: false }, 0, 6)
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

    /// 5B rung 2 in BACK position: the list trails real content, found by the
    /// mode-swapped pipe needle. Deprecated in 3.2, removed in 4 — recognized
    /// unconditionally anyway, because "wrong form for your declared version"
    /// is lint severity, never a lexing decision.
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
        // The bare default-value form: one span, no interior parse.
        assert_eq!(kinds_and_text("\\w Jésus|Jesus\\w*")[2], (ATTRS, "|Jesus"));
        // The proposal's own "ridiculous but legal" case — TWO lists on one
        // node, front and back, both kept in source order so passthrough is
        // byte-identical. "Later definition wins" is the interpreter's merge
        // rule; the lexer never resolves it.
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
        // Back position, so interior pipes are just span BYTES: rung 1 is
        // ineligible once content has been seen.
        assert_eq!(kinds_and_text("\\w a|b|c\\w*")[2], (ATTRS, "|b|c"));
        // Lexically identical to the first case even though `add` defines no
        // attributes at all. AttrStatus is lint's business; the shape set never
        // depends on the row.
        assert_eq!(
            kinds_and_text("\\add x|k=\"v\"\\add*")[2],
            (ATTRS, "|k=\"v\"")
        );
        // A hot-path character marker (`\ft` is one of the fused arms) arms the
        // needle too — the precomputed `Hot::attrs_frame` agreeing with the
        // general path is what `fast_path_identity` pins.
        assert_eq!(
            kinds_and_text("\\ft x|k=\"v\"\\ft*")[2],
            (ATTRS, "|k=\"v\"")
        );
    }

    /// The note caller: one span after the delimiter, spec pattern
    /// `/[^\\\s]+/`, so `+`/`-`/`?` are just its conventional values and the
    /// scanner enumerates nothing.
    #[test]
    fn notes_and_cross_references_carve_their_caller() {
        assert_eq!(
            kinds_and_text("\\f + \\ft text\\f*"),
            vec![
                (MARKER, "\\f "),
                (CALLER, "+"),
                (TEXT, " "),
                (MARKER, "\\ft "),
                (TEXT, "text"),
                (CLOSING, "\\f*"),
            ]
        );
        // A note whose content is bare text: without carving, the caller and the
        // content would share ONE Text token with nothing to separate them.
        // This is the case that justifies the token kind.
        assert_eq!(
            kinds_and_text("\\f - bare note\\f*"),
            vec![
                (MARKER, "\\f "),
                (CALLER, "-"),
                (TEXT, " bare note"),
                (CLOSING, "\\f*"),
            ]
        );
        // Not an enumeration — a custom caller string is one run.
        assert_eq!(
            kinds_and_text("\\f ?custom \\ft x\\f*")[1],
            (CALLER, "?custom")
        );
        // Cross-references owe one too (`x`, and `fe`/`ef`/`ex` alike).
        assert_eq!(kinds_and_text("\\x - \\xo 1.1\\x*")[1], (CALLER, "-"));
        // U25001 front-position attributes on a note: the list comes FIRST and
        // the caller expectation must survive it, exactly as `\v`'s designator
        // survives `\v|script="Arab"| 1`.
        assert_eq!(
            kinds_and_text("\\f |aid=\"n1\"| + \\ft x\\f*")[..3],
            [(MARKER, "\\f "), (ATTRS, "|aid=\"n1\"| "), (CALLER, "+")]
        );
        // Nothing to take: no empty token, same rule as a designator-less `\v`.
        assert_eq!(
            kinds_and_text("\\f\n"),
            vec![(MARKER, "\\f"), (TokenKind::Newline, "\n")]
        );
    }

    /// `\id`'s book code stops at the first space, which is what leaves the
    /// optional description as ordinary content. Nothing is validated here: the
    /// spec's own prose ("a standard 3-character identifier" + the books list)
    /// and its railroad pattern disagree — the pattern as published admits none
    /// of the 27 digit-leading codes like `1JN` — so shape and membership are
    /// both lint's, against an authored books table.
    #[test]
    fn id_carves_the_book_code_and_leaves_the_description() {
        assert_eq!(
            kinds_and_text("\\id GEN Some description\n"),
            vec![
                (MARKER, "\\id "),
                (BOOK, "GEN"),
                (TEXT, " Some description"),
                (TokenKind::Newline, "\n"),
            ]
        );
        // Digit-leading codes are ordinary here, whatever the railroad says.
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

    /// `\usfm 3.0` carves NOTHING (open question, current answer): the version
    /// string is already isolated by the line ending, and its only consumer —
    /// lint asking which version is declared — reads it off the adjacent
    /// marker, the same adjacency shape as `ca`/`cp`.
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

    /// A Byte Order Mark is ALLOWED at the start of a file
    /// so it is not an error and nothing should flag it. It lexes as what it
    /// is — ordinary content, three bytes of Text — which keeps the partition
    /// exact and round-trips the file byte-for-byte.
    ///
    /// The consequence for anything downstream: `\id` is NOT guaranteed to be
    /// the first token, so find it by SEARCHING for the marker, never by
    /// indexing token 0. (`ParseHeader` is the first consumer that will
    /// care.)
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
    /// the unnamed default value, comma-separated multiple values, and
    /// colon-separated compound parts — are invisible to the scanner. None of
    /// `,` `:` `=` `"` or space is a stop, so each is ONE span, and the
    /// interpreter does the splitting on demand.
    ///
    /// Note especially that the spec's "whitespace adjacent to the comma
    /// separators is ignored" is a NORMALIZATION rule and therefore must not
    /// happen here: the span keeps `"a, b"` byte-for-byte so it round-trips,
    /// and trimming belongs to whoever asks for the values.
    #[test]
    fn interior_attribute_syntax_is_one_span() {
        // The default attribute: a bare value, no `=`. Binding it to the row's
        // `default_attribute` is the interpreter's job; a row with no default
        // is lint's.
        assert_eq!(
            kinds_and_text("\\w gracious|grace\\w*")[2],
            (ATTRS, "|grace")
        );
        // Comma-separated values, spaces preserved exactly as written.
        assert_eq!(
            kinds_and_text("\\w x|lemma=\"a, b\" strong=\"H1,H2\"\\w*")[2],
            (ATTRS, "|lemma=\"a, b\" strong=\"H1,H2\"")
        );
        // Colon-separated compound parts.
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

    /// The ladder is QUOTE-BLIND, on purpose: it tracks no `"…"` state, so the
    /// delimiters win over quoting. Documented because it looks like a bug.
    /// The spec gives attribute values no escape mechanism, `\|` covers the one
    /// case that needs escaping, and U25001 depends on the pipe being a hard
    /// delimiter — so quote state would add work to a hot scan purely to serve
    /// strings that are already non-conforming. Failure mode is a truncated
    /// span plus a lint finding; the bytes are always all still there.
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

    /// The needle is MODE-SWAPPED: with no attrs-capable frame open, a pipe is
    /// not a stop at all, so the text run never splits. Asserting token COUNT
    /// is the point — a globally-armed needle would still pass a kinds check
    /// while costing the whole corpus.
    #[test]
    fn the_pipe_needle_is_off_outside_a_character_frame() {
        assert_eq!(kinds_and_text("\\p a|b|c d\n")[1], (TEXT, "a|b|c d"));
        // A note frame takes the front form only, so it does NOT arm the
        // needle: `\f`'s own pipe would be at a region start. (Index 2 because
        // the `+` is now carved as the note caller.)
        assert_eq!(kinds_and_text("\\f + x|y\\f*")[2], (TEXT, " x|y"));
        // Closing the frame disarms it again.
        assert_eq!(kinds_and_text("\\w a\\w* b|c\n")[3], (TEXT, " b|c"));
    }

    /// The needle's line bound, stated as behavior: a character frame whose
    /// opener and trailing list sit on DIFFERENT lines is NOT recognized as a
    /// list. Documented limitation, not an oversight — see
    /// `ScanMode::attr_frames`. Lossless either way; lint reports the pipe.
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
