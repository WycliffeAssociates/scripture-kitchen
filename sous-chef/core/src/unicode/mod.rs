//! Unicode classification: one `u16` of bits per scalar, from pinned UCD 17.0.0.
//!
//! ```text
//! class_of('A')       → alphabetic uppercase
//! class_of('\u{0301}')→ mark extender          // COMBINING ACUTE ACCENT
//! class_of('\u{200D}')→ format extender        // ZWJ
//! class_of('\u{FDD0}')→ noncharacter
//! is_glue('\u{094D}') → true                   // DEVANAGARI SIGN VIRAMA
//! ```
//!
//! The bits are the charter's authorized list plus the three refinements
//! [`atoms`] needs to keep a UCD grapheme cluster whole. Each remaining lane —
//! script, quote set, word break, normalization prefilter — returns only with
//! a consumer.
//!
//! `table.rs` is generated and committed; `sous-core` reads no file at
//! runtime. Bit table, generator, index paths, and gates: README.md.

pub mod atoms;
pub mod lookup;
mod table;
#[cfg(test)]
mod tests;

/// One scalar's classification bits. Callers read them through the
/// predicates; the layout belongs to `bin/gen-unicode.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Class(u16);

/// Bit assignment, shared by the generator and the drift test's oracle.
#[doc(hidden)]
pub mod bits {
    /// DerivedCoreProperties `Alphabetic`.
    pub const ALPHABETIC: u16 = 1 << 0;
    /// DerivedCoreProperties `Uppercase`.
    pub const UPPERCASE: u16 = 1 << 1;
    /// DerivedCoreProperties `Lowercase`.
    pub const LOWERCASE: u16 = 1 << 2;
    /// PropList `White_Space`.
    pub const WHITESPACE: u16 = 1 << 3;
    /// General_Category `Nd` — the one pooled digit lane (charter invariant 7).
    pub const DECIMAL_DIGIT: u16 = 1 << 4;
    /// General_Category `Mn | Mc | Me`.
    pub const MARK: u16 = 1 << 5;
    /// General_Category `P*`.
    pub const PUNCTUATION: u16 = 1 << 6;
    /// General_Category `S*`.
    pub const SYMBOL: u16 = 1 << 7;
    /// General_Category `Cc`.
    pub const CONTROL: u16 = 1 << 8;
    /// General_Category `Cf`.
    pub const FORMAT: u16 = 1 << 9;
    /// `U+FDD0..=U+FDEF` and every `U+xxFFFE`/`U+xxFFFF`.
    pub const NONCHARACTER: u16 = 1 << 10;
    /// Grapheme_Cluster_Break `Extend | SpacingMark | ZWJ`.
    pub const EXTENDER: u16 = 1 << 11;
    /// Grapheme_Cluster_Break `Prepend | Control | CR | LF |
    /// Regional_Indicator | L | V | T | LV | LVT`, plus `Extended_Pictographic`.
    pub const COMPLEX: u16 = 1 << 12;
    /// Grapheme_Cluster_Break `Control | CR | LF` — the COMPLEX members that
    /// break on both sides (GB4/GB5) instead of joining.
    pub const GCB_CONTROL: u16 = 1 << 13;
    /// Grapheme_Cluster_Break `Prepend` — joins forward onto a base (GB9b).
    pub const PREPEND: u16 = 1 << 14;
    /// DerivedCoreProperties `InCB; Linker` — the viramas GB9c joins through.
    pub const LINKER: u16 = 1 << 15;
}

impl Class {
    pub(crate) const fn from_bits(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn bits(self) -> u16 {
        self.0
    }

    /// True when no bit is set: an ordinary unclassified scalar.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn is_alphabetic(self) -> bool {
        self.0 & bits::ALPHABETIC != 0
    }

    pub const fn is_uppercase(self) -> bool {
        self.0 & bits::UPPERCASE != 0
    }

    pub const fn is_lowercase(self) -> bool {
        self.0 & bits::LOWERCASE != 0
    }

    pub const fn is_whitespace(self) -> bool {
        self.0 & bits::WHITESPACE != 0
    }

    pub const fn is_decimal_digit(self) -> bool {
        self.0 & bits::DECIMAL_DIGIT != 0
    }

    pub const fn is_mark(self) -> bool {
        self.0 & bits::MARK != 0
    }

    pub const fn is_punctuation(self) -> bool {
        self.0 & bits::PUNCTUATION != 0
    }

    pub const fn is_symbol(self) -> bool {
        self.0 & bits::SYMBOL != 0
    }

    pub const fn is_control(self) -> bool {
        self.0 & bits::CONTROL != 0
    }

    pub const fn is_format(self) -> bool {
        self.0 & bits::FORMAT != 0
    }

    pub const fn is_noncharacter(self) -> bool {
        self.0 & bits::NONCHARACTER != 0
    }

    pub const fn is_extender(self) -> bool {
        self.0 & bits::EXTENDER != 0
    }

    pub const fn is_complex(self) -> bool {
        self.0 & bits::COMPLEX != 0
    }

    pub const fn is_gcb_control(self) -> bool {
        self.0 & bits::GCB_CONTROL != 0
    }

    pub const fn is_prepend(self) -> bool {
        self.0 & bits::PREPEND != 0
    }

    pub const fn is_linker(self) -> bool {
        self.0 & bits::LINKER != 0
    }

    /// Charter invariant 8: "glue is Mark plus grapheme extenders".
    pub const fn is_glue(self) -> bool {
        self.0 & (bits::MARK | bits::EXTENDER) != 0
    }
}

/// The classification of one scalar.
#[inline]
pub fn class_of(c: char) -> Class {
    lookup::class_of(c)
}

/// Charter invariant 8, as a scalar predicate.
#[inline]
pub fn is_glue(c: char) -> bool {
    class_of(c).is_glue()
}
