//! Two-stage structural-index variant (the simdjson trick, sized to us).
//!
//! Stage 1: TWO uninterrupted sweeps of the whole input — `memchr3_iter`
//! collecting every `\` / CR / LF position, `memmem::find_iter` collecting
//! every confirmed `//` — each at full SIMD speed with zero stops.
//! Stage 2: the same lexer loop, but the text arm consults the prebuilt
//! position arrays ("first position >= cursor") instead of issuing a
//! bounded memchr3 + memmem call per text run.
//!
//! What this prices: the per-short-run scan-call overhead that dominates on
//! marker-dense text. Everything else (arms, boundary finders, classify) is
//! verbatim from the real lexer so the delta is only the scan strategy.
//!
//! Position bookkeeping is self-healing: consumed duplicates (the LF of a
//! CRLF, the second `\` of an escaped `\\`) are skipped because lookups are
//! monotone "first >= cursor" — nothing is marked, and positions need no
//! stream tag because the byte AT the position identifies what it is.

use memchr::memchr3_iter;
use memchr::memmem;

use crate::token::{Token, TokenKind};

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

struct ScanMode {
    awaiting_delimiter_ws: bool,
}

/// The structural index: stage-1 output, stage-2 cursors. Indices only move
/// forward (the scan cursor is monotone), so each lookup is amortized O(1).
struct StructuralIndex {
    /// Positions of every `\`, CR, LF, ascending.
    control: Vec<u32>,
    ci: usize,
    /// Positions of every confirmed `//` (non-overlapping), ascending.
    opt_breaks: Vec<u32>,
    oi: usize,
}

impl StructuralIndex {
    fn build(bytes: &[u8]) -> Self {
        // Prealloc like the token vec: control bytes (markers + newlines)
        // land around one per 8-12 source bytes across the corpora; /8 is
        // safely under the floor.
        let mut control: Vec<u32> = Vec::with_capacity(bytes.len() / 8);
        control.extend(memchr3_iter(BACKSLASH, CR, LF, bytes).map(|p| p as u32));
        let opt_breaks: Vec<u32> = memmem::find_iter(bytes, b"//").map(|p| p as u32).collect();
        Self {
            control,
            ci: 0,
            opt_breaks,
            oi: 0,
        }
    }

    fn next_control(&mut self, from: usize) -> Option<usize> {
        while self.ci < self.control.len() && (self.control[self.ci] as usize) < from {
            self.ci += 1;
        }
        self.control.get(self.ci).map(|&p| p as usize)
    }

    fn next_opt_break(&mut self, from: usize) -> Option<usize> {
        while self.oi < self.opt_breaks.len() && (self.opt_breaks[self.oi] as usize) < from {
            self.oi += 1;
        }
        self.opt_breaks.get(self.oi).map(|&p| p as usize)
    }
}

/// Same contract as `crate::lex`, two-stage scan.
pub fn lex(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut index_tables = StructuralIndex::build(bytes);

    let mut tokens: Vec<Token> = Vec::with_capacity(source.len() / 6);
    let mut mode = ScanMode {
        awaiting_delimiter_ws: false,
    };
    let mut index = 0usize;

    while index < bytes.len() {
        index = match bytes[index] {
            SPACE | TAB => whitespace_arm(bytes, index, &mut mode, &mut tokens),
            CR | LF => newline_arm(bytes, index, &mut mode, &mut tokens),
            BACKSLASH => marker_arm(bytes, index, &mut mode, &mut tokens),
            PIPE => pipe_arm(index, &mut mode, &mut tokens),
            _ => text_arm(bytes, index, &mut mode, &mut tokens, &mut index_tables),
        };
    }

    tokens
}

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

fn ws_run_end(bytes: &[u8], from: usize) -> usize {
    let mut index = from;
    while index < bytes.len() && matches!(bytes[index], SPACE | TAB) {
        index += 1;
    }
    index
}

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

fn newline_end(bytes: &[u8], from: usize) -> usize {
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

fn marker_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1;

    if bytes.get(index) == Some(&PLUS) {
        index += 1;
    }

    let name_start = index;
    while index < bytes.len() && bytes[index].is_ascii_alphanumeric() {
        index += 1;
    }

    if index == name_start && bytes.get(index) == Some(&STAR) {
        return index + 1;
    }
    if bytes.get(index) == Some(&HYPHEN)
        && matches!(
            bytes.get(index + 1),
            Some(&MILESTONE_START) | Some(&MILESTONE_END)
        )
    {
        return index + 2;
    }
    if bytes.get(index) == Some(&STAR) {
        return index + 1;
    }
    index
}

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
        return TokenKind::MilestoneEnd;
    }
    if slice.get(after_name) == Some(&HYPHEN) {
        return TokenKind::Milestone;
    }
    if slice.get(after_name) == Some(&STAR) {
        return TokenKind::ClosingMarker { nested };
    }
    TokenKind::Marker { nested }
}

fn marker_arm(bytes: &[u8], index: usize, mode: &mut ScanMode, tokens: &mut Vec<Token>) -> usize {
    let end = marker_end(bytes, index);
    let kind = classify_marker(&bytes[index..end]);
    push_token(tokens, kind, index, end);
    mode.awaiting_delimiter_ws = true;
    end
}

fn pipe_arm(index: usize, mode: &mut ScanMode, tokens: &mut Vec<Token>) -> usize {
    mode.awaiting_delimiter_ws = false;
    let end = index + 1;
    push_token(tokens, TokenKind::Pipe, index, end);
    end
}

/// THE difference from the real text arm: no memchr3/memmem calls — the next
/// interesting position comes from the prebuilt index. Decision logic is
/// otherwise identical.
fn text_arm(
    bytes: &[u8],
    index: usize,
    mode: &mut ScanMode,
    tokens: &mut Vec<Token>,
    idx: &mut StructuralIndex,
) -> usize {
    mode.awaiting_delimiter_ws = false;
    let mut segment_start = index;
    let mut cursor = index;

    loop {
        let control = idx.next_control(cursor);
        let opt_break = idx.next_opt_break(cursor);
        let pos = match (control, opt_break) {
            (Some(c), Some(o)) => c.min(o),
            (Some(c), None) => c,
            (None, Some(o)) => o,
            (None, None) => {
                cursor = bytes.len();
                break;
            }
        };

        match bytes[pos] {
            BACKSLASH => match bytes.get(pos + 1) {
                Some(&SLASH) | Some(&TILDE) | Some(&BACKSLASH) | Some(&PIPE) => {
                    cursor = pos + 2;
                }
                _ => {
                    cursor = pos;
                    break;
                }
            },
            SLASH => {
                if pos > segment_start {
                    push_token(tokens, TokenKind::Text, segment_start, pos);
                }
                push_token(tokens, TokenKind::OptBreak, pos, pos + 2);
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
        push_token(tokens, TokenKind::Text, segment_start, cursor);
    }

    cursor
}
