//! Proposed text changes: the byte-splice vocabulary the whole crate speaks.
//!
//! One shape of "here is a change to the text", with three intended speakers:
//! lint's fixes today (a [`crate::lint::Fix`] is a label plus a run of these),
//! the formatter — already ruled to be fix bundles rather than a rewriter —
//! and the diff port, whose hunks are the same shape. The CodeMirror session
//! consumes all three the same way, as byte splices it maps through the
//! byte->UTF-16 shim it already owns.
//!
//! [`FixStr`]'s name predates this module: it was lint's inline string before
//! the primitives moved out here, and renaming it is not this move's business.

/// Our own tiny inline string — the useful part of `CompactString` without the
/// dependency or the heap path.
///
/// Fix text is always short, engine-generated ASCII: a closer (`\add*`, `\+nd*`
/// — 9 bytes at the table's worst), `\p\n`, an end milestone (`\table-e\*`, 10),
/// a handful of renumber digits. Fifteen bytes covers every one of them, and
/// anything longer (a custom `\z` closer, if configuration ever gives row 0 real
/// rows) splits into ADJACENT SAME-POSITION edits which concatenate — fixed
/// width with no cap. It stays ASCII so a JS consumer decodes with
/// `String.fromCharCode` and needs no `TextEncoder`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FixStr {
    len: u8,
    bytes: [u8; FixStr::CAP],
}

impl FixStr {
    /// The inline capacity. Longer text is the caller's to split.
    pub const CAP: usize = 15;

    /// Zero length is a PURE DELETION — an edit that inserts nothing.
    pub const EMPTY: Self = Self {
        len: 0,
        bytes: [0; Self::CAP],
    };

    /// Debug-asserts both invariants rather than truncating: an over-long or
    /// non-ASCII proposal is an engine bug, and silently shortening it would
    /// put damaged text in front of a user.
    pub fn new(text: &[u8]) -> Self {
        debug_assert!(text.len() <= Self::CAP, "fix text longer than a FixStr");
        debug_assert!(text.is_ascii(), "fix text is not ASCII");
        let mut bytes = [0u8; Self::CAP];
        let len = text.len().min(Self::CAP);
        bytes[..len].copy_from_slice(&text[..len]);
        Self {
            len: len as u8,
            bytes,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(self.as_bytes()).expect("ASCII by construction")
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl core::fmt::Debug for FixStr {
    /// As the text it is — the derived form would print fifteen bytes of
    /// padding into every failing assertion.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self.as_str(), f)
    }
}

/// One byte splice: replace `from..to` with `insert`. 24 bytes, `Copy`.
///
/// Offsets are absolute byte offsets into the SOURCE the report was built
/// from — the same coordinates `Token::start` uses, so an editor session runs
/// them through the byte→UTF-16 shim it already owns. Equal ends is a pure
/// insertion; an empty `insert` is a pure deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edit {
    pub from: u32,
    pub to: u32,
    pub insert: FixStr,
}

/// Splices a fix's edits into a COPY of the source.
///
/// Right to left, which is the whole trick: every `from`/`to` is an offset into
/// the ORIGINAL text, and applying the last edit first means no earlier offset
/// has moved by the time it is used. `edits` must be sorted by `from` and
/// non-overlapping — which is what [`Fix`] guarantees, and what
/// [`check_fixes`] re-checks before it trusts one.
///
/// The library allocates here, and only here, because this is the serialization
/// boundary the ownership law names: a consumer asked for the proposed TEXT.
///
/// [`Fix`]: crate::lint::Fix
/// [`check_fixes`]: crate::lint::check_fixes
pub fn apply(source: &[u8], edits: &[Edit]) -> Vec<u8> {
    let mut fixed = source.to_vec();
    for edit in edits.iter().rev() {
        fixed.splice(
            edit.from as usize..edit.to as usize,
            edit.insert.as_bytes().iter().copied(),
        );
    }
    fixed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::Observation;

    #[test]
    fn the_fix_types_are_the_ruled_widths() {
        assert_eq!(core::mem::size_of::<Edit>(), 24);
        assert_eq!(FixStr::CAP, 15);
        assert!(FixStr::EMPTY.is_empty());
        assert_eq!(FixStr::new(b"\\table-e\\*").as_str(), "\\table-e\\*");
        // The observation row is untouched by the fix model — the link is the
        // side table, which is why this number is still four u32s.
        assert_eq!(core::mem::size_of::<Observation>(), 16);
    }
}
