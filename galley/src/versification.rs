//! Where onion meets canon: what a USFM text has, as the plain facts canon's
//! versification reads.
//!
//! ```text
//! inventory([GEN.usfm, EXO.usfm, …])  ->  canon::Inventory   which verses, parts and lengths
//! sniff([GEN.usfm, EXO.usfm, …])      ->  canon::Sniff       which scheme the text follows
//! lint_against(GEN.usfm, ulb)         ->  [Observation]      chapter-ends-early, verse-beyond-scheme
//! ```
//!
//! canon never parses USFM, and onion never learns what a versification
//! scheme is: this module is the only place the two meet. A verse's length is
//! its reader text (onion's `verse_text` mask: no markers, notes or
//! headings), in characters, which is what the sniffing rules' length tests
//! compare.

use canon::{BookKey, Inventory, Scheme, Sniff};
use usfm_onion::lint::{Observation, lint_against_counts};
use usfm_onion::{Filter, Toc, cst, lex, mask, toc};

/// Every verse of every book, in one inventory. Give it the whole text:
/// sniffing a single book has few rules to vote with.
pub fn inventory<'a>(books: impl IntoIterator<Item = &'a str>) -> Inventory {
    let mut inventory = Inventory::new();
    for source in books {
        add_book(&mut inventory, source);
    }
    inventory
}

/// Which built-in scheme the text follows.
pub fn sniff<'a>(books: impl IntoIterator<Item = &'a str>) -> Sniff {
    canon::sniff(&inventory(books))
}

/// One book judged against a scheme's counts: onion's `chapter-ends-early`
/// and `verse-beyond-scheme`, anchored on its tokens like any lint finding.
/// A book the scheme doesn't number gets none.
pub fn lint_against(source: &str, scheme: &Scheme) -> Vec<Observation> {
    let tokens = lex(source);
    let toc = toc(source.as_bytes(), &tokens);
    let book = book_key(&toc);
    lint_against_counts(&toc, |chapter| scheme.last_verse(book, chapter))
}

/// The `\id` code, uppercased: canon's keys are the spec's spelling.
fn book_key(toc: &Toc) -> BookKey {
    BookKey::new(toc.book.map(|b| b.to_ascii_uppercase()))
}

fn add_book(inventory: &mut Inventory, source: &str) {
    let bytes = source.as_bytes();
    let tokens = lex(source);
    let tree = cst::build(&tokens);
    let toc: Toc = toc(bytes, &tokens);
    let reader = mask(bytes, &tokens, &tree, &Filter::verse_text());
    let book = book_key(&toc);

    for (at, verse) in toc.verses.iter().enumerate() {
        let chapter_end = toc
            .chapters
            .iter()
            .find(|row| row.start <= verse.at && verse.at < row.end)
            .map_or(bytes.len() as u32, |row| row.end);
        let end = toc
            .verses
            .get(at + 1)
            .map_or(chapter_end, |next| next.at.min(chapter_end));
        let chars = reader_chars(source, &reader.ranges, verse.at..end);

        let members = toc.members_of(verse);
        for (n, member) in members.iter().enumerate() {
            let segment =
                &source[member.from_segment_start as usize..member.from_segment_end as usize];
            let part = match segment.as_bytes() {
                [letter] if letter.is_ascii_lowercase() => Some(char::from(*letter)),
                _ => None,
            };
            // A list (`\v 1,3,5`) gives its text to its first member: the
            // others are present, with no length of their own to compare.
            let chars = if n == 0 { chars } else { 0 };
            inventory.add(book, verse.chapter, member.from, member.to, part, chars);
        }
    }
}

/// Characters of reader text inside `extent`. Mask ranges are token spans,
/// so clipping one at a verse marker's first byte stays on a char boundary.
fn reader_chars(
    source: &str,
    ranges: &[core::ops::Range<u32>],
    extent: core::ops::Range<u32>,
) -> u32 {
    let first = ranges.partition_point(|r| r.end <= extent.start);
    let mut chars = 0usize;
    for range in &ranges[first..] {
        if range.start >= extent.end {
            break;
        }
        let start = range.start.max(extent.start) as usize;
        let end = range.end.min(extent.end) as usize;
        chars += source[start..end].chars().count();
    }
    u32::try_from(chars).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_verse_counts_its_reader_text_only() {
        let source = "\\id GEN\n\\c 1\n\\p\n\\v 1 In the \\f + \\ft a note\\f*beginning\n\\s Heading\n\\v 2 Then\n";
        let inventory = inventory([source]);
        assert_eq!(inventory.chapters(), 1);
        assert_eq!(inventory.defective().count(), 0);
    }

    #[test]
    fn parts_bridges_and_gaps_reach_the_inventory() {
        let source = "\\id GEN\n\\c 1\n\\p\n\\v 1a one\n\\v 1b two\n\\v 2-3 bridge\n\\v 5 gap\n";
        let defective: Vec<_> = inventory([source]).defective().collect();
        assert_eq!(defective, [(BookKey::new(*b"GEN"), 1)], "4 is missing");
    }
}
