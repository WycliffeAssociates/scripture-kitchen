//! Scalar twin of the real lexer: identical arms EXCEPT the text arm walks
//! byte-by-byte instead of memchr3/memmem sweeps. The delta between this and
//! `crate::lex` in the playground is the whole value of SIMD scanning.
//!
//! Copy-paste of scanner.rs by design — it must drift-check against the real
//! lexer by output equality (the playground verifies before timing), not by
//! sharing code that would hide the difference being measured.

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

/// Same contract as `crate::lex`, scalar text arm.
pub fn lex(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
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
            _ => text_arm(bytes, index, &mut mode, &mut tokens),
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
    // `TokenKind::Pipe` is retired; this frozen variant is verified for
    // PARTITION only, so the span is what matters and `Text` is the honest
    // kind for a byte nobody here interprets.
    push_token(tokens, TokenKind::Text, index, end);
    end
}

/// THE difference: no memchr3, no memmem — one byte at a time, same
/// decisions in the same order as the real text arm.
fn text_arm(bytes: &[u8], index: usize, mode: &mut ScanMode, tokens: &mut Vec<Token>) -> usize {
    mode.awaiting_delimiter_ws = false;
    let mut segment_start = index;
    let mut cursor = index;

    while cursor < bytes.len() {
        match bytes[cursor] {
            BACKSLASH => match bytes.get(cursor + 1) {
                Some(&SLASH) | Some(&TILDE) | Some(&BACKSLASH) | Some(&PIPE) => {
                    cursor += 2;
                }
                _ => break,
            },
            CR | LF => break,
            SLASH if bytes.get(cursor + 1) == Some(&SLASH) => {
                if cursor > segment_start {
                    push_token(tokens, TokenKind::Text, segment_start, cursor);
                }
                push_token(tokens, TokenKind::OptBreak, cursor, cursor + 2);
                segment_start = cursor + 2;
                cursor += 2;
            }
            _ => cursor += 1,
        }
    }

    if cursor > segment_start {
        push_token(tokens, TokenKind::Text, segment_start, cursor);
    }

    cursor
}
