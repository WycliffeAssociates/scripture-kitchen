//! Proposed text changes: the byte-splice vocabulary the whole crate speaks.
//!
//! One shape of "here is a change to the text", with three speakers: lint's
//! fixes (a [`crate::lint::Fix`] is a label plus a run of these), the formatter
//! (fix bundles, not a rewriter), and the diff port, whose hunks are the same
//! shape. A CodeMirror session consumes all three the same way, as byte splices
//! through the byte->UTF-16 shim it already owns.

/// Our own tiny inline string — the useful part of `CompactString` without the
/// dependency or the heap path.
///
/// Fix text is always short, engine-generated ASCII: a closer (9 bytes at the
/// table's worst), `\p\n`, an end milestone (`\table-e\*`, 10), renumber digits.
/// Fifteen bytes covers every one, and anything longer splits into ADJACENT
/// SAME-POSITION edits which concatenate — fixed width with no cap. ASCII so a
/// JS consumer decodes with `String.fromCharCode` and needs no `TextEncoder`.
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

/// Splices a fix's edits into a new buffer.
///
/// One forward pass, the same shape as [`apply_splices`]: copy the gap before
/// each edit, push its text, resume after its span. Every `from`/`to` indexes
/// the ORIGINAL text and the cursor only moves forward, so no offset needs
/// rebasing. `edits` must be sorted by `from` and non-overlapping — what
/// [`Fix`] guarantees and [`check_fixes`] re-checks. Edits sharing a position
/// concatenate in list order, which is how text longer than a [`FixStr`] is
/// spliced.
///
/// The library allocates here and only here: this is the serialization boundary
/// the ownership law names — a consumer asked for the proposed TEXT.
///
/// [`Fix`]: crate::lint::Fix
/// [`check_fixes`]: crate::lint::check_fixes
pub fn apply(source: &[u8], edits: &[Edit]) -> Vec<u8> {
    debug_assert!(
        edits
            .windows(2)
            .all(|pair| pair[0].to <= pair[1].from && pair[0].from <= pair[0].to),
        "edits must be sorted by `from` and non-overlapping"
    );
    // The exact output length, from one pass over the edits — the buffer is
    // allocated once and never grows.
    let grown: i64 = edits
        .iter()
        .map(|edit| {
            edit.insert.as_bytes().len() as i64 - (i64::from(edit.to) - i64::from(edit.from))
        })
        .sum();
    let mut out = Vec::with_capacity((source.len() as i64 + grown).max(0) as usize);
    let mut cursor = 0usize;
    for edit in edits {
        let from = (edit.from as usize).min(source.len()).max(cursor);
        out.extend_from_slice(&source[cursor..from]);
        out.extend_from_slice(edit.insert.as_bytes());
        cursor = (edit.to as usize).min(source.len()).max(from);
    }
    out.extend_from_slice(&source[cursor..]);
    out
}

/// One COPY splice: replace `from..to` of the baseline with `insert`, a range
/// of the CURRENT document's bytes. 16 bytes, `Copy`.
///
/// The diff's replay artifact, and the second half of the crate's edit
/// vocabulary: [`Edit`] carries authored micro-text (engine-generated ASCII in
/// a [`FixStr`]), a `SpliceEdit` carries MOVED document text — a whole
/// Devanagari verse the diff never authored, only relocated. Zero owned text,
/// so a replay is lossless by construction: both sides are the user's own
/// bytes.
///
/// `from`/`to` index the baseline, `insert` the current document; an empty
/// `insert` is a pure deletion and `from == to` a pure insertion. An editor
/// session applies one exactly like a fix — the span through its UTF-16 shim,
/// the text read out of the current document it already holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpliceEdit {
    pub from: u32,
    pub to: u32,
    pub insert: core::ops::Range<u32>,
}

/// Replays splices onto a COPY of `baseline`, reading inserted text out of
/// `current`.
///
/// `edits` must be sorted by `from` and non-overlapping (`to <= next.from`) —
/// what [`crate::diff::to_edits`] emits. Built left to right in one pass, which
/// is byte-identical to [`apply`]'s right-to-left splicing and keeps LIST order
/// for several edits at one point (right-to-left splicing preserves that order
/// too: the later edit lands first, the earlier one in front of it).
///
/// The library allocates here and only here — a consumer asked for the merged
/// TEXT.
pub fn apply_splices(baseline: &[u8], current: &[u8], edits: &[SpliceEdit]) -> Vec<u8> {
    debug_assert!(
        edits
            .windows(2)
            .all(|pair| pair[0].to <= pair[1].from && pair[0].from <= pair[0].to),
        "splices must be sorted by `from` and non-overlapping"
    );
    let mut out = Vec::with_capacity(baseline.len());
    let mut cursor = 0usize;
    for edit in edits {
        let from = (edit.from as usize).min(baseline.len()).max(cursor);
        out.extend_from_slice(&baseline[cursor..from]);
        out.extend_from_slice(&current[edit.insert.start as usize..edit.insert.end as usize]);
        cursor = (edit.to as usize).min(baseline.len()).max(from);
    }
    out.extend_from_slice(&baseline[cursor..]);
    out
}

/// The transaction pre-flight [`apply`] assumes: sorted by `from`,
/// non-overlapping, and every offset a real char boundary of `source`.
///
/// Shared by lint's fix oracle ([`check_fixes`]) and the formatter, which merge
/// edits from several rules and must prove the merge before splicing.
///
/// [`check_fixes`]: crate::lint::check_fixes
pub fn check_edits(source: &[u8], edits: &[Edit]) -> Result<(), String> {
    for pair in edits.windows(2) {
        if pair[1].from < pair[0].to || pair[1].from < pair[0].from {
            return Err(format!("edits are out of order or overlap: {pair:?}"));
        }
    }
    if let Some(edit) = edits.iter().find(|edit| {
        edit.to as usize > source.len()
            || edit.from > edit.to
            || !is_char_boundary(source, edit.from as usize)
            || !is_char_boundary(source, edit.to as usize)
    }) {
        return Err(format!(
            "edit is not a valid splice of the source: {edit:?}"
        ));
    }
    Ok(())
}

/// `str::is_char_boundary` over bytes: a continuation byte is `10xxxxxx`, and
/// one past the end is a boundary. Slicing an already-valid UTF-8 buffer here is
/// what keeps the check O(1) per edit instead of O(source).
fn is_char_boundary(source: &[u8], at: usize) -> bool {
    match source.get(at) {
        None => at == source.len(),
        Some(byte) => byte & 0b1100_0000 != 0b1000_0000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::Observation;

    #[test]
    fn the_fix_types_keep_their_fixed_widths() {
        assert_eq!(core::mem::size_of::<Edit>(), 24);
        assert_eq!(FixStr::CAP, 15);
        assert!(FixStr::EMPTY.is_empty());
        assert_eq!(FixStr::new(b"\\table-e\\*").as_str(), "\\table-e\\*");
        // An Observation stays four u32s: the fix link is the side table.
        assert_eq!(core::mem::size_of::<Observation>(), 16);
    }
}
