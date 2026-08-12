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
/// closes); milestones don't participate (`\+zaln-s` isn't a thing), so
/// they carry no dead field.
/// The working enum is 2 bytes (tag + payload — rustc doesn't bit-pack
/// multi-payload enums), so the row does NOT store it directly: it stores
/// the packed u8 from `to_bits`/`from_bits` below — low 3 bits = shape,
/// bit 3 = nested. That pair is THE one place the mapping is defined; any
/// future codec reuses it or it doesn't ship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Marker { nested: bool },
    ClosingMarker { nested: bool },
    Milestone,
    MilestoneEnd,
    Newline,
    OptBreak,
    Pipe,
    Text,
}

// "A byte with only bit 3 set" (= 8). Shapes live in the low 3 bits
// (values 0-7), so OR-ing this flag on top can never collide with a shape.
// Bits 4-7 are unused; if a 9th shape ever lands, slide this flag up to
// bit 4 and the shape field grows to 16 values — still inside the one byte.
const NESTED_BIT: u8 = 0b1000;

impl TokenKind {
    /// Packs to the row's kind byte: low 3 bits = shape, bit 3 = nested.
    pub fn to_bits(self) -> u8 {
        match self {
            // Shape number, with the nested flag OR'd on top when set —
            // e.g. nested ClosingMarker = 1 | 0b1000 = 0b1001. (The `0 |`
            // is a no-op, kept so the two arms read symmetrically.)
            Self::Marker { nested } => 0 | if nested { NESTED_BIT } else { 0 },
            Self::ClosingMarker { nested } => 1 | if nested { NESTED_BIT } else { 0 },
            Self::Milestone => 2,
            Self::MilestoneEnd => 3,
            Self::Newline => 4,
            Self::OptBreak => 5,
            Self::Pipe => 6,
            Self::Text => 7,
        }
    }

    /// Decodes the row's kind byte. The nested bit is only meaningful on the
    /// two marker shapes; on any other shape it would be a scanner bug, so
    /// it is refused loudly rather than ignored.
    pub fn from_bits(bits: u8) -> TokenKind {
        let nested = bits & NESTED_BIT != 0;
        match bits & !NESTED_BIT {
            0 => Self::Marker { nested },
            1 => Self::ClosingMarker { nested },
            other => {
                debug_assert!(!nested, "nested bit set on a non-marker shape");
                match other {
                    2 => Self::Milestone,
                    3 => Self::MilestoneEnd,
                    4 => Self::Newline,
                    5 => Self::OptBreak,
                    6 => Self::Pipe,
                    7 => Self::Text,
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
/// reading the span). Always `0` today: no data tables have been pulled in
/// yet, assignment lands with the table spine.
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
            TokenKind::Milestone,
            TokenKind::MilestoneEnd,
            TokenKind::Newline,
            TokenKind::OptBreak,
            TokenKind::Pipe,
            TokenKind::Text,
        ];
        for kind in all {
            assert_eq!(TokenKind::from_bits(kind.to_bits()), kind);
        }
    }
}
