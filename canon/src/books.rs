//! The book table and the questions asked of it.
//!
//! Two numbers name a book's place, and they are kept apart on purpose:
//! `canonical_index` is its place among the 66 (Matthew is 40) and
//! `usfm_number` is the spec's file number (Matthew is 41, because 40 is left
//! unused between the testaments). About 35 copies of this table across the
//! org's repos disagree on which one "the number" is; here neither is.

use mise::books::BookKey;

use crate::fold_tables::FOLD;
use crate::tables::{BOOKS, NAMES};

/// One USFM book identifier and what the org needs to know about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Book {
    pub code: BookKey,
    /// The spec's number, as it appears in file names: `"01"`, `"41"`, `"A0"`, `"100"`.
    pub usfm_number: &'static str,
    pub division: Division,
    /// Place among the 66, from 1. `None` outside the OT and NT.
    pub canonical_index: Option<u8>,
    /// The English name an app shows: `"Song of Songs"`, `"1 Samuel"`.
    pub english: &'static str,
    /// Other English names `parse_book` accepts: `"Song of Solomon"`.
    pub aliases: &'static [&'static str],
}

impl Book {
    /// The code in lowercase, as Writer project directories spell it (`mat`).
    pub fn slug(&self) -> String {
        self.code.to_string().to_ascii_lowercase()
    }
}

/// The spec's sections of the book table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Division {
    Ot,
    Nt,
    /// 68-87: the books printed in Catholic and Orthodox Bibles.
    Deuterocanon,
    /// A4-C3: Ezra Apocalypse, the Ethiopian and Syriac books, Laodiceans.
    Additional,
    /// Front and back matter, glossaries, indexes: no scripture.
    Peripheral,
    /// XXA-XXG: user-defined material.
    Extra,
}

impl Division {
    /// The spelling `data/books.tsv` and the TypeScript table use.
    pub const fn as_str(self) -> &'static str {
        match self {
            Division::Ot => "ot",
            Division::Nt => "nt",
            Division::Deuterocanon => "deuterocanon",
            Division::Additional => "additional",
            Division::Peripheral => "peripheral",
            Division::Extra => "extra",
        }
    }

    pub fn parse(text: &str) -> Option<Division> {
        Some(match text {
            "ot" => Division::Ot,
            "nt" => Division::Nt,
            "deuterocanon" => Division::Deuterocanon,
            "additional" => Division::Additional,
            "peripheral" => Division::Peripheral,
            "extra" => Division::Extra,
            _ => return None,
        })
    }
}

/// Which books a list holds. Every list is in the spec's row order, which is
/// canonical order within the OT and the NT; the deuterocanon follows the NT
/// rather than being interleaved the way Catholic printings place it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Canon {
    /// Genesis to Revelation.
    Protestant66,
    /// The 66, then the deuterocanon (TOB .. PSS).
    WithDeuterocanon,
    /// Every row: the above, the additional books, peripherals and XXA-XXG.
    All,
}

impl Canon {
    fn holds(self, division: Division) -> bool {
        match self {
            Canon::Protestant66 => matches!(division, Division::Ot | Division::Nt),
            Canon::WithDeuterocanon => {
                matches!(
                    division,
                    Division::Ot | Division::Nt | Division::Deuterocanon
                )
            }
            Canon::All => true,
        }
    }
}

/// The row for a code, byte-exact (`MAT`, not `mat`: [`parse_book`] folds).
pub fn book(code: BookKey) -> Option<&'static Book> {
    BOOKS.iter().find(|book| book.code == code)
}

/// The books of one canon, in order.
pub fn books(canon: Canon) -> impl Iterator<Item = &'static Book> {
    BOOKS.iter().filter(move |book| canon.holds(book.division))
}

/// The book a word names, or `None`.
///
/// Tried in order, and only exactly — prefix matching ("phil") is a
/// navigation box's policy, not the table's:
///
/// 1. a book code in any case: `MAT`, `mat`, `1jn`;
/// 2. an English name or alias, after [`fold`]: `"1samuel"`, `"Song of Solomon"`.
///
/// A code wins over a name, so `"Jon"` is Jonah, never John. Names in other
/// languages are [`parse_book_in`](crate::parse_book_in)'s.
pub fn parse_book(text: &str) -> Option<BookKey> {
    parse_code(text).or_else(|| parse_english(&fold(text)))
}

/// A book code in any case, after trimming.
pub(crate) fn parse_code(text: &str) -> Option<BookKey> {
    let bytes = <[u8; 3]>::try_from(text.trim().as_bytes()).ok()?;
    let upper = BookKey::new(bytes.map(|b| b.to_ascii_uppercase()));
    book(upper).map(|_| upper)
}

/// An English name or alias, already folded.
pub(crate) fn parse_english(folded: &str) -> Option<BookKey> {
    NAMES
        .binary_search_by(|(name, _)| (*name).cmp(folded))
        .ok()
        .map(|at| BOOKS[usize::from(NAMES[at].1)].code)
}

/// `41-MAT.usfm`: the file name Paratext, Writer stitching and the hub's
/// consolidation all give a book. `None` for a code outside the table.
pub fn usfm_file_name(code: BookKey) -> Option<String> {
    book(code).map(|book| format!("{}-{}.usfm", book.usfm_number, book.code))
}

/// A name as `parse_book` compares it: Unicode NFKD with the combining
/// accents (U+0300..U+036F) dropped, trimmed, lowercased, whitespace runs
/// collapsed to one space, and a leading digit separated from the word after
/// it. So `"Éxodo"` is `"exodo"`, and `"1Samuel"`, `"1 samuel"` and
/// `" 1   SAMUEL "` fold alike.
///
/// Sefer's `names.ts` and `ts/canon.ts` fold the same way, and
/// `fixtures/books.json` holds the cases they must agree on.
pub fn fold(text: &str) -> String {
    let mut decomposed = String::with_capacity(text.len());
    for c in text.chars() {
        decompose(c, &mut decomposed);
    }
    let mut collapsed = String::with_capacity(decomposed.len());
    let mut space = false;
    for c in decomposed.trim().chars() {
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space && !collapsed.is_empty() {
            collapsed.push(' ');
        }
        space = false;
        collapsed.push(c);
    }
    // The whole string, not char by char: a final sigma lowercases to ς.
    let mut out = collapsed.to_lowercase();
    let mut chars = out.chars();
    match (chars.next(), chars.next()) {
        (Some(digit), Some(next))
            if digit.is_ascii_digit() && next != ' ' && !next.is_ascii_digit() =>
        {
            out.insert(1, ' ');
            out
        }
        _ => out,
    }
}

/// One character's NFKD less the combining accents, onto `out`.
fn decompose(c: char, out: &mut String) {
    const S_BASE: u32 = 0xAC00;
    const T_COUNT: u32 = 28;
    const N_COUNT: u32 = 21 * T_COUNT;
    let syllable = u32::from(c).wrapping_sub(S_BASE);
    if syllable < 19 * N_COUNT {
        // Hangul: Unicode's arithmetic, so the table needn't hold 11,172 rows.
        let jamo = [
            0x1100 + syllable / N_COUNT,
            0x1161 + (syllable % N_COUNT) / T_COUNT,
            0x11A7 + syllable % T_COUNT,
        ];
        for (at, cp) in jamo.into_iter().enumerate() {
            if at < 2 || cp != 0x11A7 {
                out.extend(char::from_u32(cp));
            }
        }
        return;
    }
    match FOLD.binary_search_by(|(from, _)| from.cmp(&c)) {
        Ok(at) => out.push_str(FOLD[at].1),
        Err(_) => out.push(c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: &[u8; 3]) -> BookKey {
        BookKey::new(*code)
    }

    #[test]
    fn the_table_is_the_specs_116_codes_in_spec_order() {
        let codes: Vec<[u8; 3]> = BOOKS.iter().map(|book| book.code.as_bytes()).collect();
        let spec: Vec<[u8; 3]> = mise::books::BOOK_CODES.iter().map(|code| **code).collect();
        assert_eq!(codes, spec);
    }

    #[test]
    fn matthew_is_forty_by_index_and_forty_one_by_file_number() {
        let mat = book(key(b"MAT")).unwrap();
        assert_eq!(mat.canonical_index, Some(40));
        assert_eq!(mat.usfm_number, "41");
        assert_eq!(usfm_file_name(key(b"MAT")).as_deref(), Some("41-MAT.usfm"));
    }

    #[test]
    fn the_canons_hold_66_and_66_plus_the_deuterocanon() {
        assert_eq!(books(Canon::Protestant66).count(), 66);
        assert_eq!(books(Canon::WithDeuterocanon).count(), 86);
        assert_eq!(books(Canon::All).count(), 116);
        let indexes: Vec<u8> = books(Canon::Protestant66)
            .filter_map(|b| b.canonical_index)
            .collect();
        assert_eq!(indexes, (1..=66).collect::<Vec<u8>>());
    }

    #[test]
    fn a_code_wins_over_a_name() {
        assert_eq!(parse_book("Jon"), Some(key(b"JON")));
        assert_eq!(parse_book("John"), Some(key(b"JHN")));
    }

    #[test]
    fn a_leading_digit_folds_apart_from_its_word() {
        assert_eq!(fold("1Samuel"), "1 samuel");
        assert_eq!(fold(" 1   SAMUEL "), "1 samuel");
        assert_eq!(fold("150"), "150");
        assert_eq!(fold("Song  of\tSongs"), "song of songs");
    }
}
