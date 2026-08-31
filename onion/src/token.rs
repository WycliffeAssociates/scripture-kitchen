//! The row format: the compact token representation and THE one mapping
//! between the working enum and the packed byte. No scanning logic here —
//! this is the part a binary codec or JS twin cares about.

/// The set of shapes a token can carry.
///
/// `nested` records the `\+` SPELLING, not a legality judgment: only
/// character markers may nest, but the lexer doesn't know a marker's class
/// (a table fact), so it records the spelling wherever it appears and leaves
/// "was that legal here" to the table/lint. Milestones don't participate
/// (`\+zaln-s` isn't a thing).
///
/// `Milestone.end` records the `-e` SPELLING the same way, because the
/// suffix is the only place the fact exists (the row is shared with the `-s`
/// form) and the walker needs it to tell `\list-s` from `\list-e` without
/// re-reading source bytes. Any suffix that is not exactly `e` is
/// `end: false`; a weird suffix is the row/lint's problem.
///
/// The working enum is 2 bytes (rustc doesn't bit-pack multi-payload enums),
/// so the row stores the packed u8 from `to_bits`/`from_bits` instead — THE
/// one place the mapping is defined; a future codec reuses it or doesn't ship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Marker {
        nested: bool,
    },
    ClosingMarker {
        nested: bool,
    },
    /// A NAMED milestone token — `\qt-s`, `\zaln-e`. `end` is which half of
    /// the start/end PAIR the spelling names; both halves are otherwise
    /// identical elements (each takes attributes, each closed by its own `\*`).
    Milestone {
        end: bool,
    },
    /// The bare `\*` that terminates a milestone ELEMENT's span — syntax
    /// punctuation, not the `-e` pair-half (that is `Milestone{end: true}`,
    /// which `\*` also terminates: `\qt-e |eid="x"\*`).
    MilestoneTerminator,
    Newline,
    OptBreak,
    Text,
    /// The one-span payload after `\c`/`\v` — `1`, `12-14a`, junk alike. The
    /// scanner never looks inside; the designator interpreter judges it.
    Designator,
    /// The note caller after `\f`/`\fe`/`\ef`/`\x`/`\ex`: `+`, `-`, `?`, or a
    /// custom string. The spec pattern is `/[^\\\s]+/`, so the conventional
    /// values are common cases of one general run, never an enumeration.
    NoteCaller,
    /// The book identifier after `\id`: `GEN`, `1JN`. One span up to the first
    /// space, so `\id GEN Some description` leaves the description as Text —
    /// the code is what `Toc::book_token` points at. Shape and membership are
    /// both lint's, against an authored books table.
    BookCode,
    /// One attribute list, INCLUDING its delimiting pipe(s): `|lemma="grace"`
    /// (legacy trailing) or `|cat="x"|` (U25001 node-initial, closing pipe and
    /// any HS it absorbs included). A SPAN, never a container — the k/v view is
    /// the attribute interpreter's, on demand, which is what makes attribute
    /// passthrough byte-identical for every shape, deformed ones included.
    ///
    /// The list's FORM is not stored: position already encodes it (ends with
    /// `|` → node-initial, else trailing). Nor is its owning marker —
    /// attributes belong to the last marker, so a consumer reads the owner off
    /// the adjacent token (the same adjacency shape as `ca`/`cp`/`va`/`vp`).
    AttrList,
    /// The REDUCIBLE SURPLUS of a structural delimiter run: every horizontal
    /// whitespace byte past the ONE code unit a folding token keeps. Visible,
    /// editable bytes — but never content (text views drop the kind whole) and
    /// never chrome (no paint stands in for them, so they must render).
    ///
    /// ```text
    /// \v   1   x   Marker("\v ") Pad("  ") Designator("1 ") Pad("  ") Text("x")
    /// ```
    ///
    /// Exists only at delimiter positions — after a folding marker/milestone,
    /// a carved payload, or a node-initial attribute list — so a whitespace
    /// run INSIDE a text region can never be one.
    Pad,
}

// Shapes live in the low 4 bits, so OR-ing bit 4 on top can never collide
// with one. Bit 4 is a SPELLING flag whose meaning is per-shape: `\+` nesting
// on the two marker shapes, `-e` on Milestone. They share the bit because no
// shape carries both spellings.
const NESTED_BIT: u8 = 0b1_0000;
const END_BIT: u8 = NESTED_BIT;

impl TokenKind {
    /// Packs to the row's kind byte: low 4 bits = shape, bit 4 = nested.
    #[allow(clippy::identity_op)] // the `0 |` keeps the marker arms symmetric
    pub fn to_bits(self) -> u8 {
        match self {
            // The `0 |` is a no-op, kept so the two marker arms read
            // symmetrically.
            Self::Marker { nested } => 0 | if nested { NESTED_BIT } else { 0 },
            Self::ClosingMarker { nested } => 1 | if nested { NESTED_BIT } else { 0 },
            Self::Milestone { end } => 2 | if end { END_BIT } else { 0 },
            Self::MilestoneTerminator => 3,
            Self::Newline => 4,
            Self::OptBreak => 5,

            Self::AttrList => 6,
            Self::Text => 7,
            Self::Designator => 8,
            Self::NoteCaller => 9,
            Self::BookCode => 10,
            Self::Pad => 11,
        }
    }

    /// Decodes the row's kind byte. The spelling bit is meaningful only on the
    /// shapes that carry a spelling (`\+` markers, `-e` milestones); anywhere
    /// else it is a scanner bug, refused loudly rather than ignored.
    pub fn from_bits(bits: u8) -> TokenKind {
        let nested = bits & NESTED_BIT != 0;
        match bits & !NESTED_BIT {
            0 => Self::Marker { nested },
            1 => Self::ClosingMarker { nested },
            2 => Self::Milestone { end: nested },
            other => {
                debug_assert!(!nested, "spelling bit set on a shape without one");
                match other {
                    3 => Self::MilestoneTerminator,
                    4 => Self::Newline,
                    5 => Self::OptBreak,
                    6 => Self::AttrList,
                    7 => Self::Text,
                    8 => Self::Designator,
                    9 => Self::NoteCaller,
                    10 => Self::BookCode,
                    11 => Self::Pad,
                    _ => unreachable!("unknown kind bits {bits:#04b}"),
                }
            }
        }
    }
}

/// One compact token row: `start u32 · len u16 · kind_bits u8 · markerIdx u8 ·
/// level u8`, 12 bytes total (asserted in tests). Text is always a slice of the
/// source —
/// tokens never carry strings. `start` is an absolute byte offset.
///
/// `marker_idx` indexes the marker table for spec markers; `0` is reserved as
/// "unresolved / not a spec marker" (custom `\z*` markers resolve by reading
/// the span).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub start: u32,
    pub len: u16,
    /// Packed `TokenKind` — read through `kind()`; the mapping lives on the
    /// enum (`to_bits`/`from_bits`).
    pub kind_bits: u8,
    pub marker_idx: u8,
    /// The trailing number as SPELLED: `2` for `\q2`, `1` for `\tc1`, `0` for
    /// a bare `\q` and for every token that is not a numbered marker. The row
    /// carries the CAP (`numbering`), never the number — `\q1` and `\q2` share
    /// a row, so this is its only home outside the span.
    ///
    /// What it MEANS is the row's: a nesting level under `Numbering::UpTo`, a
    /// column index under `TableColumns`. Zero when the row admits no digits,
    /// which an illegal suffix (`\s7`) reaches by resolving to row 0.
    ///
    /// It does not reproduce the author's spelling — a milestone's `-s`/`-e`
    /// is on `kind_bits`, and a saturated `\liv999` reads 255. Exports that
    /// need the spelling still slice (`export::marker_name`).
    pub level: u8,
}

impl Token {
    pub fn kind(&self) -> TokenKind {
        TokenKind::from_bits(self.kind_bits)
    }

    pub fn end(&self) -> u32 {
        self.start + self.len as u32
    }

    /// This token's length minus the trailing whitespace the scanner folded
    /// onto it. Never empties the span — a token that is only whitespace keeps
    /// its bytes.
    ///
    /// The wire trims a finding's anchor this way, so a squiggle stops at the
    /// marker rather than at the delimiter after it.
    pub fn trimmed_len(&self, source: &[u8]) -> u32 {
        let span = &source[self.start as usize..self.end() as usize];
        let label = crate::scanner::payload_label(span);
        if label.is_empty() {
            u32::from(self.len)
        } else {
            label.len() as u32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_row_is_twelve_bytes() {
        assert_eq!(core::mem::size_of::<Token>(), 12);
    }

    #[test]
    fn kind_bits_round_trip_every_shape() {
        let all = [
            TokenKind::Marker { nested: false },
            TokenKind::Marker { nested: true },
            TokenKind::ClosingMarker { nested: false },
            TokenKind::ClosingMarker { nested: true },
            TokenKind::Milestone { end: false },
            TokenKind::Milestone { end: true },
            TokenKind::MilestoneTerminator,
            TokenKind::Newline,
            TokenKind::OptBreak,
            TokenKind::AttrList,
            TokenKind::Text,
            TokenKind::Designator,
            TokenKind::NoteCaller,
            TokenKind::BookCode,
            TokenKind::Pad,
        ];
        for kind in all {
            assert_eq!(TokenKind::from_bits(kind.to_bits()), kind);
        }
    }
}
