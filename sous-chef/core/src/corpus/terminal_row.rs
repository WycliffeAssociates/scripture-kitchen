//! Encodes and decodes one terminal entry behind the cluster section. Layout:
//! codec/README.md.
//!
//! ```text
//! TerminalCount { key: (';', bare), upper: 482, cased: 4_677 }
//!   → 3b 00 00 00 · 00 · 00 00 00 · e2 01 00 00 · 45 12 00 00
//! ```

use super::*;
use crate::judge::TerminalCount;
use crate::substrate::{FollowKey, ScalarKey};

pub(super) fn encode_terminal(entry: &TerminalCount, out: &mut Vec<u8>) {
    let glyph = entry.key.glyph().raw();
    out.extend_from_slice(&glyph.to_le_bytes());
    out.push(entry.key.context() as u8);
    out.extend_from_slice(&[0; 3]);
    out.extend_from_slice(&entry.upper.to_le_bytes());
    out.extend_from_slice(&entry.cased.to_le_bytes());
}

/// One entry at `at`; the section's length was checked by the caller.
pub(super) fn decode_terminal(
    bytes: &[u8],
    at: usize,
    entry: usize,
) -> Result<TerminalCount, CorpusWireError> {
    let bad = |field| CorpusWireError::InvalidTerminal { entry, field };
    let glyph = char::from_u32(read_u32(bytes, at + TERMINAL_GLYPH_OFFSET)).ok_or(bad("glyph"))?;
    let context = bytes[at + TERMINAL_CONTEXT_OFFSET];
    if context & !(TERMINAL_QUOTED | TERMINAL_BRACKETED) != 0 {
        return Err(bad("context"));
    }
    if bytes[at + TERMINAL_CONTEXT_OFFSET + 1..at + TERMINAL_UPPER_OFFSET] != [0; 3] {
        return Err(bad("pad"));
    }
    let terminal = TerminalCount {
        key: FollowKey::in_context(ScalarKey::of(glyph), usize::from(context)),
        upper: read_u32(bytes, at + TERMINAL_UPPER_OFFSET),
        cased: read_u32(bytes, at + TERMINAL_CASED_OFFSET),
    };
    validate(&terminal, entry)?;
    Ok(terminal)
}

/// What a typed entry can get wrong: counts that do not nest. Order is the
/// caller's, since it spans entries.
pub(super) fn validate(terminal: &TerminalCount, entry: usize) -> Result<(), CorpusWireError> {
    let bad = |field| CorpusWireError::InvalidTerminal { entry, field };
    if terminal.key.glyph().is_digits() || terminal.key.glyph().scalar().is_none() {
        return Err(bad("glyph"));
    }
    if terminal.cased == 0 {
        return Err(bad("cased"));
    }
    if terminal.upper > terminal.cased {
        return Err(bad("upper"));
    }
    Ok(())
}

/// Entries ascend strictly by `(glyph, context)`, the [`FollowKey`] order, so
/// a key appears once and a reader may binary search.
pub(super) fn in_order(
    last: Option<FollowKey>,
    terminal: &TerminalCount,
    entry: usize,
) -> Result<(), CorpusWireError> {
    match last {
        Some(last) if last >= terminal.key => Err(CorpusWireError::InvalidTerminal {
            entry,
            field: "order",
        }),
        _ => Ok(()),
    }
}
