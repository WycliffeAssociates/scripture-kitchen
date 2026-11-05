//! Where onion meets canon: what a USFM text has, as the plain facts canon's
//! versification reads.
//!
//! ```text
//! inventory([GEN.usfm, EXO.usfm, …])  ->  canon::Inventory   which verses, parts and lengths
//! sniff([GEN.usfm, EXO.usfm, …])      ->  canon::Sniff       which scheme the text follows
//! lint_against(&gen_toc, ulb)         ->  [Observation]      chapter-ends-early, verse-beyond-scheme
//! ```
//!
//! canon never parses USFM, and onion never learns what a versification
//! scheme is: this module is the only place the two meet. A verse's length is
//! its reader text (onion's `verse_text` mask: no markers, notes or
//! headings), in characters, which is what the sniffing rules' length tests
//! compare.

use core::fmt;

use canon::{BookKey, Inventory, Scheme, Sniff};
use rustc_hash::FxHashMap;
use usfm_onion::lint::{Observation, lint_against_counts};
use usfm_onion::{Filter, Toc, cst, lex, mask, toc};

use crate::pantry::BookId;

/// What a registered book says about its numbering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declared {
    /// "I follow this scheme": a built-in name (`"ulb"`) or one defined on
    /// the handle.
    Scheme(String),
    /// "I number like my source": whatever the reference registered under
    /// the same book code declares. A target with no such reference, or one
    /// whose reference declares nothing, has no scheme.
    FollowSource,
}

/// A declaration naming a scheme nobody defined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownScheme(pub String);

impl fmt::Display for UnknownScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "no versification scheme is named {:?}; the built-in ones are {}, and others need defineScheme first",
            self.0,
            canon::schemes()
                .iter()
                .map(Scheme::name)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl std::error::Error for UnknownScheme {}

/// The handle's schemes and each book's declaration. Built-ins are always
/// there; a project adds its own with [`define`](Self::define).
#[derive(Debug, Default)]
pub struct Registry {
    defined: Vec<Scheme>,
    declared: FxHashMap<BookId, Declared>,
    /// Bumped by every `define`: a defined name can mean new counts, so a
    /// pairing cached under it must miss.
    generation: u64,
}

impl Registry {
    /// How many times a scheme has been defined: part of what a publication
    /// is OF, beside each book's declaration.
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// Adds a project's scheme, replacing one defined under the same name.
    /// A built-in name can't be taken (canon's builder refuses it).
    pub fn define(&mut self, scheme: Scheme) {
        self.defined.retain(|seen| seen.name() != scheme.name());
        self.defined.push(scheme);
        self.generation += 1;
    }

    /// The crosswalk a target pairs with its source through, when both
    /// follow a known scheme and the two differ; `None` pairs numbers as
    /// written, as before any declaration. The identity is what the pairing
    /// cache keys on: both names, and for a defined scheme the generation.
    pub fn crosswalk<'a>(
        &'a self,
        target: &BookId,
        source: &BookId,
        book: BookKey,
    ) -> Option<(SchemeWalk<'a>, u64)> {
        let source_scheme = self.named(source)?;
        let target_scheme = match self.declared(target)? {
            Declared::Scheme(_) => self.named(target)?,
            Declared::FollowSource => source_scheme,
        };
        if target_scheme.name() == source_scheme.name() {
            return None;
        }
        let mut hash = rustc_hash::FxHasher::default();
        core::hash::Hash::hash(&(target_scheme.name(), source_scheme.name()), &mut hash);
        if !(target_scheme.is_builtin() && source_scheme.is_builtin()) {
            core::hash::Hash::hash(&self.generation, &mut hash);
        }
        // Never 0: that is "no crosswalk" in the key.
        let identity = core::hash::Hasher::finish(&hash) | 1;
        Some((
            SchemeWalk {
                target: target_scheme,
                source: source_scheme,
                book,
            },
            identity,
        ))
    }

    /// A scheme by name: built-in first, then defined.
    pub fn scheme(&self, name: &str) -> Option<&Scheme> {
        canon::scheme(name).or_else(|| self.defined.iter().find(|s| s.name() == name))
    }

    /// Records (or with `None`, clears) a book's declaration. A named scheme
    /// must exist now, so a typo fails here rather than silently checking
    /// nothing.
    pub fn declare(&mut self, id: BookId, declared: Option<Declared>) -> Result<(), UnknownScheme> {
        match declared {
            Some(Declared::Scheme(name)) if self.scheme(&name).is_none() => {
                Err(UnknownScheme(name))
            }
            Some(declared) => {
                self.declared.insert(id, declared);
                Ok(())
            }
            None => {
                self.declared.remove(&id);
                Ok(())
            }
        }
    }

    pub fn declared(&self, id: &BookId) -> Option<&Declared> {
        self.declared.get(id)
    }

    /// The scheme a book's own declaration names, without following a
    /// source. `None` for no declaration and for `FollowSource`.
    pub fn named(&self, id: &BookId) -> Option<&Scheme> {
        match self.declared.get(id)? {
            Declared::Scheme(name) => self.scheme(name),
            Declared::FollowSource => None,
        }
    }
}

/// One book's verses across a target's scheme and its source's, for sous's
/// pairing: canon's [`counterparts`](canon::counterparts) in each direction,
/// kept to this book (a scheme that folds books together, like `lxx`'s EZR
/// holding `org`'s NEH, pairs per book all the same).
#[derive(Debug, Clone, Copy)]
pub struct SchemeWalk<'a> {
    pub target: &'a Scheme,
    pub source: &'a Scheme,
    pub book: BookKey,
}

impl SchemeWalk<'_> {
    fn across(
        &self,
        from: &Scheme,
        to: &Scheme,
        chapter: u16,
        verse: u16,
        out: &mut Vec<(u16, u16)>,
    ) {
        let v = canon::Verse::new(self.book, chapter, verse);
        out.extend(
            canon::counterparts(from, to, v)
                .into_iter()
                .filter(|t| t.book == self.book)
                .map(|t| (t.chapter, t.verse)),
        );
    }
}

impl sous_core::Crosswalk for SchemeWalk<'_> {
    fn to_source(&self, chapter: u16, verse: u16, out: &mut Vec<(u16, u16)>) {
        self.across(self.target, self.source, chapter, verse, out);
    }

    fn to_target(&self, chapter: u16, verse: u16, out: &mut Vec<(u16, u16)>) {
        self.across(self.source, self.target, chapter, verse, out);
    }
}

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
///
/// Takes the book's table of contents, which a registered book already has
/// (`Pantry::toc`): this lexes nothing. The book's counts are found once, and
/// each verse is then an index into them.
pub fn lint_against(toc: &Toc, scheme: &Scheme) -> Vec<Observation> {
    let Some(counts) = scheme.verse_counts(book_key(toc)) else {
        return Vec::new();
    };
    lint_against_counts(toc, |chapter| {
        counts.get(usize::from(chapter).checked_sub(1)?).copied()
    })
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
