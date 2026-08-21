//! The books AUXILIARY table: valid `\id` book identifiers, membership only.
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
}
