//! The `\id` book identifiers, and the three-byte key that names one.
//!
//! ```text
//! is_book_code(b"1JN")         -> true    membership, byte-exact
//! is_scripture_code(b"FRT")    -> false   a peripheral division
//! upper3(b"1jn")               -> 1JN     then ask again
//! canonical_rank(MRK)          -> 40      spec order, not sorted order
//! ```
//!
//! A flat authored list, not a `MarkerRow` column and not codegen output.
//! Nothing here maps a code to a name, a number, a testament or a
//! versification: lint asks one question ("is this a book identifier"), and the
//! day an export needs English names or Paratext numbers is the day this grows
//! a second column.
//!
//! Source: the USFM 3.1 spec's Book Identifiers table (ubsicap/usfm
//! `docs/identification/books.rst`), kept in spec row order — NOT sorted — so
//! a future spec-diff reads top to bottom. Lookup is therefore a linear scan of
//! 116 three-byte comparisons, run once per document (a book has one `\id`).

use core::fmt;

/// Every 3-character book identifier the spec defines: 39 OT, 27 NT, the
/// deuterocanon and its additions, the peripheral divisions, and XXA-XXG.
/// Grouped by the spec's sections, twelve to a line, so a reviewer can diff it
/// against the spec page by eye.
#[rustfmt::skip]
pub const BOOK_CODES: [&[u8; 3]; 116] = [
    // 01-39: Old Testament.
    b"GEN", b"EXO", b"LEV", b"NUM", b"DEU", b"JOS", b"JDG", b"RUT", b"1SA", b"2SA", b"1KI", b"2KI",
    b"1CH", b"2CH", b"EZR", b"NEH", b"EST", b"JOB", b"PSA", b"PRO", b"ECC", b"SNG", b"ISA", b"JER",
    b"LAM", b"EZK", b"DAN", b"HOS", b"JOL", b"AMO", b"OBA", b"JON", b"MIC", b"NAM", b"HAB", b"ZEP",
    b"HAG", b"ZEC", b"MAL",
    // 41-67: New Testament.
    b"MAT", b"MRK", b"LUK", b"JHN", b"ACT", b"ROM", b"1CO", b"2CO", b"GAL", b"EPH", b"PHP", b"COL",
    b"1TH", b"2TH", b"1TI", b"2TI", b"TIT", b"PHM", b"HEB", b"JAS", b"1PE", b"2PE", b"1JN", b"2JN",
    b"3JN", b"JUD", b"REV",
    // 68-87: the deuterocanon printed in Catholic/Orthodox Bibles.
    b"TOB", b"JDT", b"ESG", b"WIS", b"SIR", b"BAR", b"LJE", b"S3Y", b"SUS", b"BEL", b"1MA", b"2MA",
    b"3MA", b"4MA", b"1ES", b"2ES", b"MAN", b"PS2", b"ODA", b"PSS",
    // A4-C3: the additional books (Ezra Apocalypse, Ethiopian and Syriac
    // canons, the Latin Laodiceans).
    b"EZA", b"5EZ", b"6EZ", b"DAG", b"PS3", b"2BA", b"LBA", b"JUB", b"ENO", b"1MQ", b"2MQ", b"3MQ",
    b"REP", b"4BA", b"LAO",
    // A0-B1: peripheral divisions — no scripture, so ordering lint stays quiet
    // about their missing chapters.
    b"FRT", b"BAK", b"OTH", b"INT", b"CNC", b"GLO", b"TDX", b"NDX",
    // 94-100: extra material, user-defined content.
    b"XXA", b"XXB", b"XXC", b"XXD", b"XXE", b"XXF", b"XXG",
];

/// Is this span EXACTLY a spec book identifier? Byte-exact — case folding is
/// the caller's business, because "in the table but lowercase" is its own
/// finding (`book-code-not-uppercase`) and must not be confused with "unknown".
pub fn is_book_code(span: &[u8]) -> bool {
    let Ok(code) = <&[u8; 3]>::try_from(span) else {
        return false;
    };
    BOOK_CODES.contains(&code)
}

/// A SCRIPTURE identifier — anything before the peripheral-division tail
/// (FRT..NDX) and the user-defined XX codes. Byte-exact, like
/// [`is_book_code`]: casing is the caller's business.
pub fn is_scripture_code(span: &[u8]) -> bool {
    let Ok(code) = <&[u8; 3]>::try_from(span) else {
        return false;
    };
    BOOK_CODES[..SCRIPTURE_COUNT].contains(&code)
}

/// OT + NT + deuterocanon + the additional books: everything ahead of FRT.
const SCRIPTURE_COUNT: usize = 101;

/// ASCII-uppercases a 3-byte span, or `None` if the span is not 3 bytes.
/// Non-letters pass through, so `1jn` → `1JN` and `1-n` → `1-N` (which is then
/// simply not in the table).
pub fn upper3(span: &[u8]) -> Option<[u8; 3]> {
    let code = <&[u8; 3]>::try_from(span).ok()?;
    Some([
        code[0].to_ascii_uppercase(),
        code[1].to_ascii_uppercase(),
        code[2].to_ascii_uppercase(),
    ])
}

/// The stable scripture identity that pairs books across producer inputs — a
/// book identifier's three bytes, uninterpreted.
///
/// Byte-exact, like [`is_book_code`]: a key may hold a code the table does not,
/// and casing is the caller's business.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BookKey([u8; 3]);

impl BookKey {
    pub const fn new(bytes: [u8; 3]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; 3] {
        self.0
    }
}

impl From<[u8; 3]> for BookKey {
    fn from(bytes: [u8; 3]) -> Self {
        Self::new(bytes)
    }
}

impl fmt::Display for BookKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match core::str::from_utf8(&self.0) {
            Ok(code) => f.write_str(code),
            Err(_) => write!(f, "{:02X}{:02X}{:02X}", self.0[0], self.0[1], self.0[2]),
        }
    }
}

/// A book's position in [`BOOK_CODES`], which is the spec's order; a code
/// outside the table ranks after every code inside it.
///
/// Ties there are broken by the caller — `BOOK_CODES.len()` for all of them —
/// so a caller sorting on this compares the key's bytes next.
pub fn canonical_rank(key: BookKey) -> usize {
    let bytes = key.as_bytes();
    BOOK_CODES
        .iter()
        .position(|code| **code == bytes)
        .unwrap_or(BOOK_CODES.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_the_specs_116_codes_with_no_duplicates() {
        assert_eq!(BOOK_CODES.len(), 116);
        let mut sorted: Vec<&[u8; 3]> = BOOK_CODES.to_vec();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), before, "duplicate book code");
        for code in BOOK_CODES {
            assert!(
                code.iter()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()),
                "{} is not an uppercase/digit code",
                std::str::from_utf8(code).unwrap()
            );
        }
        // Spot the anchors of each section, so a mis-paste is caught.
        assert_eq!(BOOK_CODES[0], b"GEN");
        assert_eq!(BOOK_CODES[38], b"MAL");
        assert_eq!(BOOK_CODES[39], b"MAT");
        assert_eq!(BOOK_CODES[65], b"REV");
        assert_eq!(BOOK_CODES[115], b"XXG");
    }

    #[test]
    fn membership_is_byte_exact() {
        assert!(is_book_code(b"GEN"));
        assert!(is_book_code(b"1JN"));
        assert!(is_book_code(b"FRT"));
        assert!(is_book_code(b"XXG"));
        // Case folding is NOT this function's job.
        assert!(!is_book_code(b"gen"));
        assert!(!is_book_code(b"Gen"));
        assert!(!is_book_code(b"ZZZ"));
        assert!(!is_book_code(b"GENESIS"));
        assert!(!is_book_code(b"GE"));
        assert!(!is_book_code(b""));
    }

    #[test]
    fn uppercasing_is_length_checked() {
        assert_eq!(upper3(b"gen"), Some(*b"GEN"));
        assert_eq!(upper3(b"1jn"), Some(*b"1JN"));
        assert_eq!(upper3(b"GEN"), Some(*b"GEN"));
        assert_eq!(upper3(b"GENESIS"), None);
    }

    #[test]
    fn a_key_ranks_by_spec_order_and_unknown_codes_rank_last() {
        assert_eq!(canonical_rank(BookKey::new(*b"GEN")), 0);
        assert_eq!(canonical_rank(BookKey::new(*b"MRK")), 40);
        assert_eq!(canonical_rank(BookKey::new(*b"XXG")), BOOK_CODES.len() - 1);
        // Not the table's order: PSA precedes MAT, which sorting would reverse.
        assert!(canonical_rank(BookKey::new(*b"PSA")) < canonical_rank(BookKey::new(*b"MAT")));
        assert_eq!(canonical_rank(BookKey::new(*b"ZZZ")), BOOK_CODES.len());
        assert_eq!(canonical_rank(BookKey::new(*b"gen")), BOOK_CODES.len());
    }

    #[test]
    fn a_key_prints_its_code_and_falls_back_to_hex() {
        assert_eq!(BookKey::new(*b"MRK").to_string(), "MRK");
        assert_eq!(BookKey::from(*b"1JN").as_bytes(), *b"1JN");
        assert_eq!(BookKey::new([0xFF, 0x00, 0x41]).to_string(), "FF0041");
    }
}
