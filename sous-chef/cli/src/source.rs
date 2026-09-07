//! The declared source corpus, from USFM through Onion or from a vref file.
//!
//! ```text
//! load("corpora/amh.txt")
//!   MAT 2:1  "ንጉሡ ሄሮድስም ሰምቶ ደነገጠ…"
//!   MAT 2:2  <range>                  ┐ one interval unit, key 2:1-3,
//!   MAT 2:3  <range>                  ┘ beginning at the concrete row
//!   → SourceBook MAT, chapters [2], verses [(2:1-3, 0..71)]
//! ```
//!
//! One struct for both producers, because pairing needs a key and a length and
//! neither producer's storage. A vref line is the honest projection of a lossy
//! export: it cannot recreate the USFM whitespace or markup the export
//! discarded, and it does not pretend to.

use std::collections::BTreeMap;
use std::path::Path;

use sous_core::{BookKey, Chapter, ProjectedBook, TextRange, Verse, VerseKey};
use usfm_galley::sous::OnionBook;

/// The placeholder a vref export writes for every verse after the first of an
/// interval; it is structure, not content.
const RANGE: &str = "<range>";

/// One declared source book: projected text plus the rows over it.
pub struct SourceBook {
    key: BookKey,
    /// What the CLI prints beside it — a path, or a book code inside a vref.
    id: String,
    text: String,
    chapters: Vec<Chapter>,
    verses: Vec<Verse>,
}

impl SourceBook {
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The projected text of one verse row.
    pub fn slice(&self, span: TextRange) -> &str {
        &self.text[span.from() as usize..span.to() as usize]
    }

    /// Onion's own projection, copied into the shape the CLI pairs with.
    fn from_onion(id: String, book: &OnionBook) -> Self {
        Self {
            key: ProjectedBook::key(book),
            id,
            text: book.text().to_string(),
            chapters: book.chapters().collect(),
            verses: book.verses().collect(),
        }
    }
}

impl ProjectedBook for SourceBook {
    fn key(&self) -> BookKey {
        self.key
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn chapters(&self) -> impl Iterator<Item = Chapter> {
        self.chapters.iter().copied()
    }

    fn verses(&self) -> impl Iterator<Item = Verse> {
        self.verses.iter().copied()
    }
}

/// Whether this path is read as USFM rather than as a vref stream: a
/// directory, or a file named `.sfm`/`.usfm`.
pub fn is_usfm(path: &Path) -> bool {
    path.is_dir() || crate::supported_extension(path)
}

/// Every source book at `path`, from either producer.
pub fn load(path: &Path, parallel: bool) -> Result<Vec<SourceBook>, Box<dyn std::error::Error>> {
    if is_usfm(path) {
        let loaded = crate::load_input(path, parallel)?;
        return Ok(loaded
            .paths
            .iter()
            .zip(&loaded.books)
            .map(|(path, book)| SourceBook::from_onion(path.display().to_string(), book))
            .collect());
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let books = parse_vref(&raw);
    if books.is_empty() {
        return Err(format!("{} holds no `BOOK C:V<TAB>text` rows", path.display()).into());
    }
    Ok(books)
}

/// One interval unit, still keyed by its first verse: a concrete row plus the
/// contiguous `<range>` placeholders that follow it.
#[derive(Clone)]
struct Unit {
    chapter: u16,
    first: u16,
    last: u16,
    text: String,
}

/// Groups the file's rows into books in first-seen order, fusing every
/// concrete row with the placeholders behind it.
fn parse_vref(raw: &str) -> Vec<SourceBook> {
    let mut books: BTreeMap<usize, (String, Vec<Unit>)> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for line in raw.lines() {
        let Some((code, chapter, verse, text)) = parse_row(line) else {
            continue;
        };
        let at = match order.iter().position(|seen| seen == code) {
            Some(at) => at,
            None => {
                order.push(code.to_string());
                books.insert(order.len() - 1, (code.to_string(), Vec::new()));
                order.len() - 1
            }
        };
        let units = &mut books.get_mut(&at).expect("just inserted").1;
        if text == RANGE {
            // A placeholder extends the unit it follows; a stray one, with no
            // concrete row before it in this chapter, is dropped rather than
            // read as an empty verse.
            if let Some(open) = units.last_mut()
                && open.chapter == chapter
                && open.last + 1 == verse
            {
                open.last = verse;
            }
            continue;
        }
        units.push(Unit {
            chapter,
            first: verse,
            last: verse,
            text: text.to_string(),
        });
    }

    books
        .into_values()
        .filter_map(|(code, units)| build(&code, &units))
        .collect()
}

/// `BOOK C:V<TAB>text`; `None` for a blank line, a front-matter row whose
/// chapter or verse is not a number, or a row with no tab.
fn parse_row(line: &str) -> Option<(&str, u16, u16, &str)> {
    let (address, text) = line.split_once('\t')?;
    let mut parts = address.split_whitespace();
    let code = parts.next()?;
    let (chapter, verse) = parts.next()?.split_once(':')?;
    Some((code, chapter.parse().ok()?, verse.parse().ok()?, text))
}

/// Lays one book's units out as projected text with chapter and verse rows
/// over it. Each unit keeps the newline after it, exactly as a mask does.
///
/// Rows are laid out in KEY order, not file order: a projection has to be
/// monotone to be a valid producer, and a vref export's line order is not —
/// `nya` writes GEN 37:36 before 37:35. The fusion above happens first, in
/// file order, because a `<range>` placeholder is positional; the sort is
/// stable, so duplicate keys keep their occurrence order.
fn build(code: &str, units: &[Unit]) -> Option<SourceBook> {
    if units.is_empty() {
        return None;
    }
    let mut units = units.to_vec();
    units.sort_by_key(|unit| (unit.chapter, unit.first, unit.last));
    let units = &units[..];
    let mut bytes = [b' '; 3];
    for (slot, byte) in bytes.iter_mut().zip(code.as_bytes()) {
        *slot = byte.to_ascii_uppercase();
    }

    let mut text = String::new();
    let mut verses = Vec::new();
    let mut chapters: Vec<Chapter> = Vec::new();
    let mut open: Option<(u16, u32)> = None;
    for unit in units {
        let Ok(key) = VerseKey::new(unit.chapter, unit.first, unit.last) else {
            continue;
        };
        if open.is_none_or(|(number, _)| number != unit.chapter) {
            if let Some((number, from)) = open.take() {
                chapters.push(chapter_row(number, from, text.len() as u32)?);
            }
            open = Some((unit.chapter, text.len() as u32));
        }
        let from = text.len() as u32;
        text.push_str(&unit.text);
        text.push('\n');
        verses.push(Verse::new(
            key,
            TextRange::new(from, text.len() as u32).expect("a verse grows forward"),
        ));
    }
    let (number, from) = open?;
    chapters.push(chapter_row(number, from, text.len() as u32)?);

    Some(SourceBook {
        key: BookKey::new(bytes),
        id: code.to_string(),
        text,
        chapters,
        verses,
    })
}

fn chapter_row(number: u16, from: u32, to: u32) -> Option<Chapter> {
    Chapter::new(number, TextRange::new(from, to).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: &str = concat!(
        "MAT 1:1\tA book of the genealogy.\n",
        "MAT 2:1\tNow after Jesus was born.\n",
        "MAT 2:2\t<range>\n",
        "MAT 2:3\t<range>\n",
        "MAT 2:4\tAnd gathering together.\n",
        "MRK 1:1\tThe beginning of the good news.\n",
    );

    #[test]
    fn a_concrete_row_and_its_placeholders_are_one_interval_unit() {
        let books = parse_vref(ROWS);
        assert_eq!(books.len(), 2);
        assert_eq!(books[0].key(), BookKey::new(*b"MAT"));

        let keys: Vec<(u16, u16, u16)> = books[0]
            .verses()
            .map(|verse| {
                let key = verse.key();
                (key.chapter(), key.first(), key.last())
            })
            .collect();
        assert_eq!(keys, vec![(1, 1, 1), (2, 1, 3), (2, 4, 4)]);

        let bridge = books[0].verses().nth(1).unwrap();
        assert_eq!(books[0].slice(bridge.text()), "Now after Jesus was born.\n");
    }

    #[test]
    fn chapters_cover_their_own_verses_and_the_book_validates() {
        let books = parse_vref(ROWS);
        let numbers: Vec<u16> = books[0].chapters().map(|row| row.number()).collect();
        assert_eq!(numbers, vec![1, 2]);
        for book in &books {
            assert_eq!(sous_core::validate(book), Ok(()), "{}", book.id());
        }
    }

    #[test]
    fn front_matter_and_untabbed_rows_are_skipped() {
        let raw = concat!(
            "MAT ?:?\tfront matter\n",
            "no tab here\n",
            "\n",
            "MAT 1:1\tA book of the genealogy.\n",
        );
        let books = parse_vref(raw);
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].verses().count(), 1);
    }

    /// A placeholder with nothing to extend is structure with no content; it
    /// is dropped rather than becoming an empty verse that pairs with one.
    #[test]
    fn a_leading_placeholder_is_dropped() {
        let books = parse_vref("MAT 1:1\t<range>\nMAT 1:2\tSecond.\n");
        assert_eq!(books.len(), 1);
        let keys: Vec<u16> = books[0].verses().map(|verse| verse.key().first()).collect();
        assert_eq!(keys, vec![2]);
    }

    /// The whole committed tier parses, which is the only place `<range>` rows
    /// occur in real data.
    #[test]
    fn the_test_tier_parses_and_validates() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpora");
        for name in [
            "WA-en-ulb",
            "amh",
            "francl",
            "grcsr",
            "hin2017",
            "nya",
            "spaRV1909",
            "swhulb",
        ] {
            let path = dir.join(format!("{name}.txt"));
            assert!(path.is_file(), "test-tier corpus at {}", path.display());
            let raw = std::fs::read_to_string(&path).unwrap();
            let books = parse_vref(&raw);
            assert!(!books.is_empty(), "{name}");
            for book in &books {
                assert_eq!(sous_core::validate(book), Ok(()), "{name} {}", book.id());
            }
        }
    }
}
