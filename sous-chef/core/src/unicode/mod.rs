//! Sous's reading of a scalar: the G2 neighbour pools and the grapheme-safe
//! atom rule, over `mise::unicode`'s classification bits.
//!
//! ```text
//! pool_of('\u{964}')                        → Terminal   // danda
//! atoms::widen_to_atoms("qx\u{0301}", 2..4)  → 1..4       // the mark keeps its base
//! ```
//!
//! The bits themselves — `Class`, `class_of`, the two index paths — are
//! `mise::unicode`, the leaf both engines share. What stays here is what only
//! Sous means: the charter's pools, invariant 6's atoms, and the pinned-UCD
//! drift gates over both halves. Bit table, generator, index paths, and gates:
//! README.md.

pub mod atoms;
mod pools;
#[cfg(test)]
mod tests;

/// The classifier itself, re-exported: `substrate::is_nonletter`, `atoms`, and
/// `pool_of` all speak in `Class`, so a caller of those needs the type and the
/// lookup without reaching past sous-core for them. Everything else the
/// classifier owns — the bits, `is_glue`, the two index paths — is
/// `mise::unicode` and only there.
pub use mise::unicode::{Class, class_of};

/// The G2 neighbour category of one scalar: eight pools, first match wins.
///
/// ```text
/// pool_of('\u{ab}')   → Quote      // «, a Quotation_Mark before it is Pi
/// pool_of('\u{964}')  → Terminal   // danda, Sentence_Terminal
/// pool_of('\u{60c}')  → Separator  // Arabic comma, Terminal_Punctuation only
/// pool_of('\u{2212}') → Dash       // MINUS SIGN is Sm, and a Dash first
/// ```
///
/// The precedence is `Quotation_Mark` → [`Pool::Quote`]; GC `Ps | Pe | Pi |
/// Pf` → [`Pool::Bracket`]; `Dash` → [`Pool::Dash`]; `Sentence_Terminal` →
/// [`Pool::Terminal`]; `Terminal_Punctuation` → [`Pool::Separator`]; `Nd` →
/// [`Pool::Digit`]; the `SYMBOL` bit → [`Pool::Symbol`]; else [`Pool::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Pool {
    Quote = 0,
    Bracket = 1,
    Dash = 2,
    Terminal = 3,
    Separator = 4,
    /// A digit is not a run atom, so this never occurs as an in-run
    /// neighbour. It is here so `pool_of` is total over every scalar.
    Digit = 5,
    Symbol = 6,
    Other = 7,
}

impl Pool {
    pub const ALL: [Self; 8] = [
        Self::Quote,
        Self::Bracket,
        Self::Dash,
        Self::Terminal,
        Self::Separator,
        Self::Digit,
        Self::Symbol,
        Self::Other,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Quote => "Quote",
            Self::Bracket => "Bracket",
            Self::Dash => "Dash",
            Self::Terminal => "Terminal",
            Self::Separator => "Separator",
            Self::Digit => "Digit",
            Self::Symbol => "Symbol",
            Self::Other => "Other",
        }
    }

    /// `None` for a discriminant past the table.
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Quote),
            1 => Some(Self::Bracket),
            2 => Some(Self::Dash),
            3 => Some(Self::Terminal),
            4 => Some(Self::Separator),
            5 => Some(Self::Digit),
            6 => Some(Self::Symbol),
            7 => Some(Self::Other),
            _ => None,
        }
    }
}

/// One scalar's pool, off the generated table with two bit shortcuts.
///
/// `Nd` and a bare `S*` need no rows: no scalar carrying one of the four
/// pinned punctuation properties is a digit, so `Digit` may answer before the
/// search, and `Symbol` is the last pool before `Other`, so it answers only
/// where no row claimed the scalar first.
pub fn pool_of(c: char) -> Pool {
    let class = class_of(c);
    if class.is_decimal_digit() {
        return Pool::Digit;
    }
    match pools::POOLS.binary_search_by_key(&(c as u32), |row| row.0) {
        Ok(at) => pools::POOLS[at].1,
        Err(_) if class.is_symbol() => Pool::Symbol,
        Err(_) => Pool::Other,
    }
}
