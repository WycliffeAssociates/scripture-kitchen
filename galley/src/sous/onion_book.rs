//! Onion's native `Mask + Toc` projection, as a `sous-core` `ProjectedBook`.
//!
//! ```text
//! OnionBook::parse("\\id MRK\n\\c 1\n\\p\n\\v 1 Jesus \\f + \\ft note\\f* wept.\n")
//!   .text()      "Jesus  wept.\n"          // verse text, markup gone
//!   .chapters()  [Chapter 1 at 0..13]
//!   .locate(0..13)                          // -> MRK 1:1, raw runs [21..27, 43..50]
//! ```
//!
//! This lives in galley, not in either engine: Onion remains the USFM
//! authority, while Sous sees only projected ranges and numeric units.
//! [`OnionBook::from_parts`] rebuilds one from a Pantry's retained mask and TOC
//! without lexing again.

use core::ops::Range;

use crate::onion::{Filter, Mask, Sid, Toc, cst, lex, mask, toc};
use sous_core::{
    BookKey, Chapter, InputError, ProjectedBook, TextRange, Verse, VerseKey, validate,
};

pub struct OnionBook {
    text: String,
    mask: Mask,
    toc: Toc,
}

impl OnionBook {
    /// Lex, build, TOC, mask, then [`from_parts`](Self::from_parts).
    pub fn parse(source: &str) -> Result<Self, InputError> {
        let tokens = lex(source);
        let tree = cst::build(&tokens);
        let toc = toc(source.as_bytes(), &tokens);
        let mask = mask(source.as_bytes(), &tokens, &tree, &Filter::verse_text());
        Self::from_parts(source, mask, toc)
    }

    /// The same book from products already derived: gathers the projected text
    /// over the mask ranges and lexes nothing.
    ///
    /// `mask` and `toc` must be the ones derived from exactly `text`.
    pub fn from_parts(text: &str, mask: Mask, toc: Toc) -> Result<Self, InputError> {
        let book = Self {
            text: mask.text(text.as_bytes()),
            mask,
            toc,
        };
        validate(&book)?;
        Ok(book)
    }

    pub fn key(&self) -> BookKey {
        BookKey::new(self.toc.book)
    }

    /// Verse anchors Onion kept for its own lint that carry no numeric key.
    pub fn unkeyed_anchor_count(&self) -> usize {
        self.toc
            .verses
            .iter()
            .filter(|anchor| VerseKey::new(anchor.chapter, anchor.first, anchor.last).is_err())
            .count()
    }

    /// The scripture address of a projected range, and its raw source runs.
    pub fn locate(&self, range: TextRange) -> Option<LocatedRange<'_>> {
        if range.is_empty() || range.to() > self.text.len() as u32 {
            return None;
        }
        let first = self.toc.locate(self.mask.to_source(range.from()));
        let last = self.toc.locate(self.mask.to_source(range.to() - 1));
        Some(LocatedRange {
            first,
            last,
            spans: SourceSpans {
                mask: &self.mask,
                projected: range,
                at: 0,
            },
        })
    }
}

impl ProjectedBook for OnionBook {
    fn key(&self) -> BookKey {
        self.key()
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn chapters(&self) -> impl Iterator<Item = Chapter> {
        self.toc
            .chapters
            .iter()
            .filter(|row| row.number != 0)
            .map(|row| {
                let text = projected(&self.mask, row.span());
                Chapter::new(row.number, text).expect("front matter was filtered above")
            })
    }

    fn verses(&self) -> impl Iterator<Item = Verse> {
        self.toc
            .verses
            .iter()
            .enumerate()
            .filter_map(|(at, anchor)| {
                // Onion retains malformed anchors for its own lint. Sous
                // cannot align one without a numeric key, so that unit
                // abstains and the surrounding book survives.
                let key = VerseKey::new(anchor.chapter, anchor.first, anchor.last).ok()?;
                let chapter_row = self
                    .toc
                    .chapters
                    .partition_point(|chapter| chapter.start <= anchor.at)
                    .saturating_sub(1);
                let chapter_end = self.toc.chapters[chapter_row].end;
                let end = self
                    .toc
                    .verses
                    .get(at + 1)
                    .map_or(chapter_end, |next| next.at.min(chapter_end));
                Some(Verse::new(key, projected(&self.mask, anchor.at..end)))
            })
    }
}

fn projected(mask: &Mask, source: Range<u32>) -> TextRange {
    let range = mask
        .project_source(source)
        .expect("TOC source extents are ordered");
    TextRange::new(range.start, range.end).expect("mask projection is ordered")
}

/// Where a projected range starts and ends in scripture, and the raw runs
/// under it.
pub struct LocatedRange<'a> {
    pub first: Sid,
    pub last: Sid,
    pub spans: SourceSpans<'a>,
}

/// Iterates only the retained raw runs behind a projected finding. A finding
/// can cross removed markup, so one contiguous USFM span would be lossy at
/// exactly the boundary this adapter preserves.
pub struct SourceSpans<'a> {
    mask: &'a Mask,
    projected: TextRange,
    at: usize,
}

impl Iterator for SourceSpans<'_> {
    type Item = Range<u32>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(source) = self.mask.ranges.get(self.at) {
            let projected_start = self.mask.starts[self.at];
            let projected_end = projected_start + source.end - source.start;
            self.at += 1;

            let from = projected_start.max(self.projected.from());
            let to = projected_end.min(self.projected.to());
            if from < to {
                return Some(
                    source.start + from - projected_start..source.start + to - projected_start,
                );
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USFM: &str = concat!(
        "\\id MRK\n",
        "\\c 1\n\\p\n",
        "\\v 1 Jesus \\f + \\ft note\\f* wept.\n",
        "\\v 2-3 Two and three.\n",
        "\\c 2\n\\p\n",
        "\\v 1 An 🧅.\n",
    );

    #[test]
    fn mask_and_toc_form_the_neutral_book_without_a_second_index() {
        let book = OnionBook::parse(USFM).unwrap();
        let chapters: Vec<_> = book.chapters().collect();
        let verses: Vec<_> = book.verses().collect();

        assert_eq!(book.key().as_bytes(), *b"MRK");
        assert_eq!(chapters.len(), 2);
        assert_eq!(verses.len(), 3);
        assert_eq!(verses[1].key(), VerseKey::new(1, 2, 3).unwrap());
        assert_eq!(
            &book.text()[verses[0].text().from() as usize..verses[0].text().to() as usize],
            "Jesus  wept.\n"
        );
    }

    #[test]
    fn locate_returns_sid_and_each_retained_source_run() {
        let book = OnionBook::parse(USFM).unwrap();
        let verse = book.verses().next().unwrap();
        let located = book.locate(verse.text()).unwrap();
        let first = located.first;
        let last = located.last;
        let spans: Vec<_> = located.spans.collect();

        assert_eq!(
            first,
            Sid {
                book: *b"MRK",
                chapter: 1,
                first: 1,
                last: 1,
            }
        );
        assert_eq!(last, first);
        assert_eq!(spans.len(), 2, "the removed footnote splits raw source");
        assert_eq!(
            spans
                .iter()
                .map(|span| &USFM[span.start as usize..span.end as usize])
                .collect::<String>(),
            "Jesus  wept.\n"
        );
    }

    #[test]
    fn malformed_verse_abstains_without_consuming_its_neighbors() {
        let source = concat!(
            "\\id ACT\n",
            "\\c 8\n\\p\n",
            "\\v 1 one\n",
            "\\v + unkeyed\n",
            "\\v 2 two\n",
        );
        let book = OnionBook::parse(source).unwrap();
        let verses: Vec<_> = book.verses().collect();

        assert_eq!(book.unkeyed_anchor_count(), 1);
        assert_eq!(
            verses.iter().map(|verse| verse.key()).collect::<Vec<_>>(),
            vec![
                VerseKey::new(8, 1, 1).unwrap(),
                VerseKey::new(8, 2, 2).unwrap(),
            ]
        );
        assert_eq!(
            &book.text()[verses[0].text().from() as usize..verses[0].text().to() as usize],
            "one\n"
        );
        assert_eq!(
            &book.text()[verses[1].text().from() as usize..verses[1].text().to() as usize],
            "two\n"
        );
        assert!(book.text().contains("unkeyed"));
    }

    #[test]
    fn markup_backslashes_are_silent_and_a_content_pair_locates_to_its_verse() {
        use sous_core::{HygieneClass, hygiene};

        // Every marker backslash is masked out, including the footnote's.
        let clean = OnionBook::parse(USFM).unwrap();
        assert!(hygiene::scan(clean.text()).is_empty());

        // Onion lexes `\ ` and `\b` as markers, malformed or not, so only a
        // `\\` pair reaches Sous as content.
        let source = concat!(
            "\\id MRK\n",
            "\\c 1\n\\p\n",
            "\\v 1 Jesus \\f + \\ft note\\f* wept \\ a\\b \\\\ here.\n",
        );
        let book = OnionBook::parse(source).unwrap();
        let findings = hygiene::scan(book.text());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].class(), HygieneClass::StrandedBackslash);
        assert_eq!(findings[0].run(), 2);
        let located = book.locate(findings[0].span()).unwrap();
        assert_eq!((located.first.chapter, located.first.first), (1, 1));
        let raw: Vec<_> = located.spans.collect();
        assert_eq!(raw.len(), 1, "the pair is one retained run");
        assert_eq!(&source[raw[0].start as usize..raw[0].end as usize], "\\\\");
    }
}
