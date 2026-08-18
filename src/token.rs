//! The row format: the compact token representation and THE one mapping
//! between the working enum and the packed byte. No scanning logic lives
//! here — this is the part a future binary codec or JS twin cares about.

/// The set of shapes a token can carry.
///
/// `nested` records the `\+` SPELLING, not a legality judgment: per spec
/// only character markers nest, but the lexer doesn't yet know a marker's
/// class (that's a marker-table fact), so it records the spelling wherever
/// it appears and leaves "was that legal here" to the table/lint. It rides
/// on the two variants where the spelling occurs (`\+w` opens, `\+w*`
/// closes); milestones don't participate (`\+zaln-s` isn't a thing).
///
/// `Milestone.end` records the `-e` SPELLING the same way — `\qt-e`,
/// `\zaln-e` — because the suffix is the only place the fact exists (the
/// row is shared with the `-s` form) and the walker needs it to tell a
/// container OPENER (`\list-s`) from its CLOSER (`\list-e`) without
/// re-reading source bytes. Any suffix that is not exactly `e` (including
/// `-s`) is `end: false`; a weird suffix is the row/lint's problem.
/// The working enum is 2 bytes (tag + payload — rustc doesn't bit-pack
/// multi-payload enums), so the row does NOT store it directly: it stores
/// the packed u8 from `to_bits`/`from_bits` below — low 4 bits = shape,
/// bit 4 = nested. That pair is THE one place the mapping is defined; any
/// future codec reuses it or it doesn't ship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Marker {
        nested: bool,
    },
    ClosingMarker {
        nested: bool,
    },
    /// A NAMED milestone token — `\qt-s`, `\zaln-e`. `end` is which half of
    /// the logical start/end PAIR the `-s`/`-e` spelling names; both halves
    /// are otherwise identical elements (each takes attributes and is closed
    /// by its own `\*`).
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
    /// The one-span payload after `\c`/`\v`: `1`, `12-14a`, junk — the scanner
    /// never looks inside; the designator INTERPRETER judges it against the
    /// spec's `VERSE` pattern.
    Designator,
    /// The note caller after `\f`/`\fe`/`\ef`/`\x`/`\ex`: `+`, `-`, `?`, or a
    /// custom string. Spec pattern is `/[^\\\s]+/`, so the conventional values
    /// are just the common cases of one general run, never an enumeration.
    NoteCaller,
    /// The book identifier after `\id`: `GEN`, `1JN`. One span up to the first
    /// space, so `\id GEN Some description` leaves the description as ordinary
    /// Text. NOT validated here — "3 uppercase characters" and "is a known
    /// code" are both lint's, against an authored books table.
    ///
    /// Deliberately not the id line's whole remainder: the code is what
    /// `ParseHeader.book` wants to point at, and the description is content.
    BookCode,
    /// One attribute list, INCLUDING its delimiting pipe(s): `|lemma="grace"`
    /// (legacy trailing) or `|cat="x"|` (U25001 node-initial, closing pipe and
    /// any HS it absorbs included). A SPAN, never a container — the interior is
    /// never parsed here and the k/v view is the attribute interpreter's, on
    /// demand, exactly like `Designator`. That is what makes attribute
    /// passthrough byte-identical for every shape the spec allows, deformed
    /// ones included.
    ///
    /// The list's FORM is not stored, because position already encodes it:
    /// ends with `|` → node-initial, else trailing. A tree that files
    /// attributes in a named slot forgets stream order and must therefore
    /// derive and record the form itself; a token never carries it.
    ///
    /// Which marker owns the list is likewise not stored — attributes belong
    /// to the last marker, so a consumer reads the owner off the adjacent
    /// marker/closer token (the same adjacency shape as `ca`/`cp`/`va`/`vp`).
    AttrList,
}

// "A byte with only bit 4 set" (= 16). Shapes live in the low 4 bits
// (values 0-15; the 9th shape, Designator, forced the slide from bit 3),
// so OR-ing this flag on top can never collide with a shape. Bits 5-7 are
// unused. Bit 4 is a SPELLING flag whose meaning is per-shape: `\+` nesting
// on the two marker shapes, `-e` on Milestone. They can share the bit
// because no shape carries both spellings.
const NESTED_BIT: u8 = 0b1_0000;
const END_BIT: u8 = NESTED_BIT;

impl TokenKind {
    /// Packs to the row's kind byte: low 4 bits = shape, bit 4 = nested.
    pub fn to_bits(self) -> u8 {
        match self {
            // Shape number, with the nested flag OR'd on top when set —
            // e.g. nested ClosingMarker = 1 | 0b1_0000 = 0b1_0001. (The `0 |`
            // is a no-op, kept so the two arms read symmetrically.)
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
        }
    }

    /// Decodes the row's kind byte. The spelling bit is only meaningful on
    /// the shapes that carry a spelling (`\+` markers, `-e` milestones); on
    /// any other shape it would be a scanner bug, so it is refused loudly
    /// rather than ignored.
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
                    _ => unreachable!("unknown kind bits {bits:#04b}"),
                }
            }
        }
    }
}

/// One compact token row: `start u32 · len u16 · kind_bits u8 · markerIdx u8`,
/// 8 bytes total (asserted in tests). Text is always a slice of the source —
/// tokens never carry strings.
///
/// `start` is an absolute byte offset into the source FOR NOW. NOTE: this may
/// move to chapter-relative spans (treating chapters as hunks/slots, with
/// each hunk's base offset in the header's run table) so that an edit inside
/// one chapter never shifts another chapter's rows. Undecided — see
/// planning/ideas/committed/braidv2.md.
///
/// `marker_idx` indexes the marker table for spec markers; `0` is reserved
/// as "unresolved / not a spec marker" (custom `\z*` markers resolve by
/// reading the span). Stamped by the marker arm since step 4.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub start: u32,
    pub len: u16,
    /// Packed `TokenKind` — read through `kind()`; the mapping lives on the
    /// enum (`to_bits`/`from_bits`).
    pub kind_bits: u8,
    pub marker_idx: u8,
}

impl Token {
    pub fn kind(&self) -> TokenKind {
        TokenKind::from_bits(self.kind_bits)
    }

    pub fn end(&self) -> u32 {
        self.start + self.len as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_row_is_eight_bytes() {
        assert_eq!(core::mem::size_of::<Token>(), 8);
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
        ];
        for kind in all {
            assert_eq!(TokenKind::from_bits(kind.to_bits()), kind);
        }
    }
}
