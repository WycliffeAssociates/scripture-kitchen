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
/// without re-lexing (planning/QUESTIONS.md Q9).
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
    // True right after emitting a Marker, until the one whitespace run that
    // structurally delimits it (if any) has been consumed.
    //
    // TODO(table): this fold is currently UNCONDITIONAL, which is knowingly
    // wrong — whether a marker takes a delimiting whitespace is a fact of
    // its marker class (closing markers don't; their trailing space is
    // content). Becomes a table lookup when the spine + role column land.
    awaiting_delimiter_ws: bool,
}

/// Lexes a whole source into compact token rows.
///
/// The loop dispatches on the first byte of the next region; each arm calls
/// a boundary finder (which alone moves the cursor) and then classifies the
/// slice it found.
pub fn lex(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    // Presize from source length: onion measured a hard density floor of
    // ~6.5 bytes per lexeme across the corpus (poetry, `\w`, and full
    // `\zaln` alignment all land 6.6-7.4; plain prose is sparser at ~16),
    // so `/6` sits just under the floor and effectively never reallocs.
    let mut tokens: Vec<Token> = Vec::with_capacity(source.len() / 6);
    let mut mode = ScanMode {
        awaiting_delimiter_ws: false,
    };
    // Built ONCE per lex: memmem::find (the one-shot form) reconstructs its
    // searcher on every call — measured 31ns/call vs 6ns with a prebuilt
    // Finder, against a ~5ns/token budget. The text arm calls this once per
    // text-run iteration, so the one-shot form was a real tax.
    let opt_break_finder = memmem::Finder::new(b"//");
    let mut index = 0usize;

    while index < bytes.len() {
        index = match bytes[index] {
            SPACE | TAB => whitespace_arm(bytes, index, &mut mode, &mut tokens),
            CR | LF => newline_arm(bytes, index, &mut mode, &mut tokens),
            BACKSLASH => marker_arm(bytes, index, &mut mode, &mut tokens),
            PIPE => pipe_arm(index, &mut mode, &mut tokens),
            _ => text_arm(bytes, index, &mut mode, &mut tokens, &opt_break_finder),
        };
    }

    tokens
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

/// A whitespace run is either a marker's structural delimiter (folded into
/// that marker's span — see the ScanMode TODO about making this per-class)
/// or plain content, classified `Text` like any other non-marker run.
fn whitespace_arm(
    bytes: &[u8],
    index: usize,
    mode: &mut ScanMode,
    tokens: &mut Vec<Token>,
) -> usize {
    let end = ws_run_end(bytes, index);

    if mode.awaiting_delimiter_ws {
        mode.awaiting_delimiter_ws = false;
        if let Some(last) = tokens.last_mut() {
            last.len = (end as u32 - last.start) as u16;
        }
    } else {
        push_token(tokens, TokenKind::Text, index, end);
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

fn marker_arm(bytes: &[u8], index: usize, mode: &mut ScanMode, tokens: &mut Vec<Token>) -> usize {
    let end = marker_end(bytes, index);
    let kind = classify_marker(&bytes[index..end]);
    push_token(tokens, kind, index, end);
    mode.awaiting_delimiter_ws = true;
    end
}

// ---- pipe arm -------------------------------------------------------------

// Just the delimiter byte itself — attribute-list handling (one `AttrList`
// region once the fused pass has an open-marker stack; a bare pipe in plain
// text is content) is deferred with the data tables.
fn pipe_arm(index: usize, mode: &mut ScanMode, tokens: &mut Vec<Token>) -> usize {
    mode.awaiting_delimiter_ws = false;
    let end = index + 1;
    push_token(tokens, TokenKind::Pipe, index, end);
    end
}

// ---- text arm ---------------------------------------------------------------

fn text_arm(
    bytes: &[u8],
    index: usize,
    mode: &mut ScanMode,
    tokens: &mut Vec<Token>,
    opt_break_finder: &memmem::Finder<'_>,
) -> usize {
    mode.awaiting_delimiter_ws = false;
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
            // `\/`, `\~`, `\\`, `\|` are escaped literal content (def.txt's
            // TEXT pattern) — keep the run going. Any other backslash is a
            // real marker start.
            BACKSLASH => match bytes.get(pos + 1) {
                Some(&SLASH) | Some(&TILDE) | Some(&BACKSLASH) | Some(&PIPE) => {
                    cursor = pos + 2;
                }
                _ => {
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
}
