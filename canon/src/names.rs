//! Book names in the gateway languages: what a book is called, and which book
//! a localized name means.
//!
//! The names are each translation's own `\toc1`/`\toc2`/`\toc3`, harvested
//! from the org's gateway-language texts and reviewed (`data/names/`). A
//! project's own book names, where it has them, come before these; canon is
//! what stands behind them.

use mise::books::BookKey;

use crate::books::{book, parse_code, parse_english};
use crate::name_tables::LOCALES;
use crate::tables::BOOKS;

/// Which of a book's names: `\toc1`, `\toc2` or `\toc3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// `\toc1`: "The Book of Genesis", "Matthieu".
    Long,
    /// `\toc2`: "Genesis". A locale with no short name answers with its long one.
    Short,
    /// `\toc3`: "Gen". Never stands in for another form, nor they for it.
    Abbrev,
}

/// A name, and where it came from, so a UI can mark one it didn't find in
/// the language it asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Named {
    pub text: &'static str,
    /// The locale that answered: one asked for, or `"en"` as the last resort.
    pub locale: &'static str,
    /// Which form answered: `Long` where `Short` was asked and the locale has none.
    pub form: Form,
}

pub(crate) struct Locale {
    pub(crate) tag: &'static str,
    pub(crate) books: &'static [(u8, [&'static str; 3])],
    pub(crate) index: &'static [(&'static str, u8)],
}

/// The locales canon holds names for, by tag: `"am"`, `"es-419"`, `"fr"`.
pub fn locales() -> impl Iterator<Item = &'static str> {
    LOCALES.iter().map(|locale| locale.tag)
}

/// The names locale a BCP 47 tag means, of those canon holds: the same tag
/// in any case; else a held locale of the same language whose script, if it
/// names one, the tag names too. A macrolanguage reads as the language canon
/// holds for it (`ar` is `arb`, `mg` is `plt`, `fil` is `tl`).
///
/// ```text
/// locale_for("es")       Some("es-419")    a region the tag lacks or differs on is fine
/// locale_for("pt-PT")    Some("pt-br")
/// locale_for("ar-EG")    Some("arb")
/// locale_for("ur")       None              ur-deva is Devanagari; plain ur is Arabic script
/// locale_for("ur-Deva")  Some("ur-deva")
/// ```
pub fn locale_for(tag: &str) -> Option<&'static str> {
    find(tag).map(|locale| locale.tag)
}

/// A tag's language, macrolanguage aliases folded, and its script, lowercased.
fn subtags(tag: &str) -> (String, Option<String>) {
    let mut parts = tag.split(['-', '_']);
    let language = parts.next().unwrap_or("").to_ascii_lowercase();
    let language = match language.as_str() {
        "ar" => "arb".to_owned(),
        "mg" => "plt".to_owned(),
        "fil" => "tl".to_owned(),
        _ => language,
    };
    let script = parts
        .find(|part| part.len() == 4 && part.bytes().all(|b| b.is_ascii_alphabetic()))
        .map(str::to_ascii_lowercase);
    (language, script)
}

/// The table a requested tag means: [`locale_for`]'s rule.
fn find(wanted: &str) -> Option<&'static Locale> {
    let exact = LOCALES
        .iter()
        .find(|locale| locale.tag.eq_ignore_ascii_case(wanted));
    exact.or_else(|| {
        let (language, script) = subtags(wanted);
        if language.is_empty() {
            return None;
        }
        LOCALES.iter().find(|locale| {
            let (held, held_script) = subtags(locale.tag);
            held == language && (held_script.is_none() || held_script == script)
        })
    })
}

/// What a book is called, trying each locale in order, then English.
///
/// Within a locale, `Short` falls back to `Long`. The English of last resort
/// is en_ulb's `\toc` names, then the book table's English (which also covers
/// the books en_ulb doesn't hold). `None` only for a code outside the table,
/// or an `Abbrev` nobody has.
pub fn name(code: BookKey, locales: &[&str], form: Form) -> Option<Named> {
    let row = BOOKS.iter().position(|b| b.code == code)?;
    let row = u8::try_from(row).ok()?;
    let in_locale = |locale: &'static Locale| {
        let (_, names) = locale.books.iter().find(|(r, _)| *r == row)?;
        let pick = |form: Form| {
            let text = names[form as usize];
            (!text.is_empty()).then_some(Named {
                text,
                locale: locale.tag,
                form,
            })
        };
        pick(form).or_else(|| (form == Form::Short).then(|| pick(Form::Long)).flatten())
    };
    locales
        .iter()
        .chain(["en"].iter())
        .filter_map(|tag| find(tag))
        .find_map(in_locale)
        .or_else(|| {
            (form != Form::Abbrev).then(|| Named {
                text: book(code).map_or("", |b| b.english),
                locale: "en",
                form,
            })
        })
}

/// [`parse_book`](crate::parse_book), plus the names of each locale given:
/// a code first, then each locale's names in order (any form, folded,
/// exactly), then English. A code still wins, as there: a locale whose
/// abbreviation is another book's code can't take it.
pub fn parse_book_in(text: &str, locales: &[&str]) -> Option<BookKey> {
    if let Some(code) = parse_code(text) {
        return Some(code);
    }
    let folded = crate::fold(text);
    locales
        .iter()
        .filter_map(|tag| find(tag))
        .find_map(|locale| {
            locale
                .index
                .binary_search_by(|(name, _)| (*name).cmp(folded.as_str()))
                .ok()
                .map(|at| BOOKS[usize::from(locale.index[at].1)].code)
        })
        .or_else(|| parse_english(&folded))
}
