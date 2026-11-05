//! Which built-in scheme a text follows, from what it has.
//!
//! ```text
//! sniff(en_ulb)   -> ulb   Decided   250 rules voted: ulb 250-0, eng 249-1, org 146-104, …
//! sniff(en_ult)   -> eng   Decided
//! sniff(bsb)      -> eng   Decided   dissent: 3Jn.14:15, Rev.12:17-13.1a
//! sniff(one book) -> eng   Leaning   7 rules voted
//! ```
//!
//! The rules are TVTMS's (via nida-institute/versification, `data/sniff/`):
//! each names a passage where numbering traditions differ, and per tradition
//! (a column) the tests a text passes when it follows it, such as
//! `Gen.2:25=Last` or `Neh.4:6*2>Neh.4:3`. A rule votes once: the text's
//! passing columns, against the columns each scheme fits. The generator
//! worked out those fits from each scheme's own counts. A scheme that fits a
//! column the text passes agrees; one that fits none disagrees. Rules every
//! scheme agrees on don't count.
//!
//! Three things keep a bad text from voting for a numbering it doesn't have:
//!
//! - **Abstention.** A rule sits out when the text can't answer it honestly: a
//!   book or chapter it lacks, a `Last` or `NotExist` test in a chapter with a
//!   gap, a duplicate or a verse out of order (a missing verse fools exactly
//!   those), a length test on a bridged or missing verse, or a form the
//!   sniffer doesn't read (`Pro.20:20-22`).
//! - **Voting, not building.** The answer is a named scheme. A text that
//!   follows none comes back with its dissent, not as a new scheme made in
//!   its image (which would bake its defects in).
//! - **Confidence.** `Decided` needs 20 votes and the winner wrong on at most
//!   one in ten; `Leaning` is any clear winner; `Undetermined` is a tie or no
//!   votes, which is most single NT books.
//!
//! canon never reads USFM. An [`Inventory`] is plain facts (which verses, how
//! long), which galley builds from onion and anything else can build too.

use std::collections::{BTreeMap, BTreeSet};

use mise::books::BookKey;

use crate::scheme::{Scheme, schemes};
use crate::sniff_tables::RULES;

/// What a text has: its verses, their parts and lengths, per chapter, and
/// which chapters are too damaged to testify about their last verse.
#[derive(Debug, Default, Clone)]
pub struct Inventory {
    chapters: BTreeMap<([u8; 3], u16), Chapter>,
}

#[derive(Debug, Default, Clone)]
struct Chapter {
    verses: BTreeMap<u16, Facts>,
    parts: BTreeSet<(u16, char)>,
    /// The number of the last verse added, for order.
    previous: u16,
    out_of_order: bool,
    duplicate: bool,
}

#[derive(Debug, Default, Clone, Copy)]
struct Facts {
    /// Characters of reader text (no markers, notes or headings).
    chars: u32,
    /// Written as part of a bridge (`\v 1-2`): present, with no length of
    /// its own.
    bridged: bool,
    /// Written as a whole verse, not only as parts.
    whole: bool,
    /// Written in parts (`\v 3a`).
    parted: bool,
}

impl Inventory {
    pub fn new() -> Inventory {
        Inventory::default()
    }

    /// One `\v` (or its equivalent), in source order: verses `first..=last`
    /// (a bridge when they differ), or part `part` of `first` (`3a`), with
    /// `chars` characters of reader text. A malformed designator is
    /// `first == 0` and is skipped.
    pub fn add(
        &mut self,
        book: BookKey,
        chapter: u16,
        first: u16,
        last: u16,
        part: Option<char>,
        chars: u32,
    ) {
        if first == 0 || chapter == 0 {
            return;
        }
        let row = self.chapters.entry((book.as_bytes(), chapter)).or_default();
        if first < row.previous {
            row.out_of_order = true;
        }
        row.previous = first;
        if last > first {
            for verse in first..=last {
                let facts = row.verses.entry(verse).or_default();
                row.duplicate |= facts.whole || facts.bridged || facts.parted;
                facts.bridged = true;
            }
            return;
        }
        let facts = row.verses.entry(first).or_default();
        match part {
            Some(part) => {
                row.duplicate |= !row.parts.insert((first, part)) || facts.whole;
                facts.parted = true;
                facts.chars += chars;
            }
            None => {
                // A whole verse after its parts is the verse twice, as the
                // parts after a whole one are.
                row.duplicate |= facts.whole || facts.bridged || facts.parted;
                facts.whole = true;
                facts.chars += chars;
            }
        }
    }

    /// How many chapters the text has.
    pub fn chapters(&self) -> usize {
        self.chapters.len()
    }

    /// The chapters that abstain from `Last` and `NotExist` tests: a gap, a
    /// duplicate, or a verse out of order.
    pub fn defective(&self) -> impl Iterator<Item = (BookKey, u16)> + '_ {
        self.chapters
            .iter()
            .filter(|(_, row)| row.defective())
            .map(|((book, chapter), _)| (BookKey::new(*book), *chapter))
    }

    fn chapter(&self, place: &Place) -> Option<&Chapter> {
        self.chapters.get(&(place.book.as_bytes(), place.chapter))
    }
}

impl Chapter {
    fn defective(&self) -> bool {
        let gap = self
            .verses
            .keys()
            .copied()
            .zip(1..)
            .any(|(verse, want)| verse != want);
        self.out_of_order || self.duplicate || gap
    }

    fn last(&self) -> Option<u16> {
        self.verses.keys().next_back().copied()
    }
}

/// How sure a sniff is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// At least 20 rules voted, the winner is wrong on at most one in ten,
    /// and nothing ties it.
    Decided,
    /// A clear winner on fewer votes, or with more dissent.
    Leaning,
    /// No votes, or a tie at the top. Use the project's default (for WA
    /// texts, the source's scheme).
    Undetermined,
}

/// One scheme's tally.
#[derive(Debug, Clone, Copy)]
pub struct Standing {
    pub scheme: &'static Scheme,
    /// Rules where the text passes a column this scheme fits.
    pub agree: u32,
    /// Rules where it passes only columns this scheme doesn't fit.
    pub disagree: u32,
}

/// A rule that voted against the winner: where the text numbers like
/// something else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dissent {
    /// TVTMS's name for the passage, `3Jn.14:15`.
    pub rule: &'static str,
    /// The traditions the text follows there, `"Greek / Latin"`.
    pub text_follows: Vec<&'static str>,
}

#[derive(Debug, Clone)]
pub struct Sniff {
    /// Every built-in scheme, best first: fewest disagreements, then most
    /// agreements.
    pub ranked: Vec<Standing>,
    pub confidence: Confidence,
    /// Rules that told the schemes apart.
    pub voted: u32,
    /// Rules the winner disagrees with.
    pub dissent: Vec<Dissent>,
    /// Rules where the text passed no column at all: it numbers that passage
    /// like no tradition TVTMS knows. Usually a defect lint already shows.
    pub unmatched: Vec<&'static str>,
}

impl Sniff {
    /// The winner, unless the sniff is `Undetermined`.
    pub fn best(&self) -> Option<&'static Scheme> {
        match self.confidence {
            Confidence::Undetermined => None,
            Confidence::Decided | Confidence::Leaning => self.ranked.first().map(|s| s.scheme),
        }
    }
}

/// Which built-in scheme the text follows. See the module doc.
pub fn sniff(inventory: &Inventory) -> Sniff {
    let all = schemes();
    let mut agree = vec![0u32; all.len()];
    let mut disagree = vec![0u32; all.len()];
    let mut votes: Vec<(usize, u8)> = Vec::new();
    let mut unmatched = Vec::new();
    for (at, rule) in RULES.iter().enumerate() {
        let Some(passed) = rule.columns_passed(inventory) else {
            continue;
        };
        if passed == 0 {
            unmatched.push(rule.name);
            continue;
        }
        let fits = |s: usize| rule.fits[s] & passed != 0;
        let sitting: Vec<usize> = (0..all.len()).filter(|&s| rule.fits[s] != 0).collect();
        if sitting.iter().all(|&s| fits(s)) {
            continue;
        }
        for s in sitting {
            if fits(s) {
                agree[s] += 1;
            } else {
                disagree[s] += 1;
            }
        }
        votes.push((at, passed));
    }

    let mut ranked: Vec<Standing> = all
        .iter()
        .enumerate()
        .map(|(s, scheme)| Standing {
            scheme,
            agree: agree[s],
            disagree: disagree[s],
        })
        .collect();
    let order = |s: &Standing| (s.disagree, u32::MAX - s.agree);
    ranked.sort_by_key(order);

    let voted = u32::try_from(votes.len()).unwrap_or(u32::MAX);
    let ahead = match ranked.as_slice() {
        [first, second, ..] => order(first) < order(second),
        _ => true,
    };
    let confidence = match ranked.first() {
        Some(first) if voted > 0 && ahead => {
            if voted >= 20 && first.disagree * 10 <= voted {
                Confidence::Decided
            } else {
                Confidence::Leaning
            }
        }
        _ => Confidence::Undetermined,
    };

    let dissent = match ranked.first() {
        Some(first) => {
            let s = all
                .iter()
                .position(|scheme| core::ptr::eq(scheme, first.scheme))
                .unwrap_or(0);
            votes
                .iter()
                .filter(|(at, passed)| {
                    let fit = RULES[*at].fits[s];
                    fit != 0 && fit & passed == 0
                })
                .map(|&(at, passed)| {
                    let rule = &RULES[at];
                    Dissent {
                        rule: rule.name,
                        text_follows: (0..rule.columns.len())
                            .filter(|c| passed & (1 << c) != 0)
                            .map(|c| rule.columns[c])
                            .collect(),
                    }
                })
                .collect()
        }
        None => Vec::new(),
    };

    Sniff {
        ranked,
        confidence,
        voted,
        dissent,
        unmatched,
    }
}

/// A verse a rule tests, or a part of one (`b` for TVTMS's `.2`).
#[derive(Debug)]
pub(crate) struct Place {
    pub(crate) book: BookKey,
    pub(crate) chapter: u16,
    pub(crate) verse: u16,
    pub(crate) part: Option<char>,
}

#[derive(Debug)]
pub(crate) enum Test {
    Last(Place),
    Exist(Place),
    NotExist(Place),
    /// `left * left_factor` is longer (`>`) or shorter (`<`) than
    /// `right * right_factor`.
    Compare {
        left: Place,
        left_factor: u8,
        longer: bool,
        right: Place,
        right_factor: u8,
    },
    /// A form the sniffer doesn't read; the rule abstains. Kept, as written,
    /// so the table shows what was skipped.
    #[allow(dead_code)]
    Unsupported(&'static str),
}

#[derive(Debug)]
pub(crate) struct Rule {
    pub(crate) name: &'static str,
    pub(crate) columns: &'static [&'static str],
    pub(crate) tests: &'static [&'static [Test]],
    /// Per built-in scheme, a bit per column it fits; 0 sits the rule out.
    pub(crate) fits: &'static [u8],
}

impl Rule {
    /// A bit per column the text passes, or `None` when the rule abstains.
    fn columns_passed(&self, inventory: &Inventory) -> Option<u8> {
        let mut passed = 0;
        for (column, tests) in self.tests.iter().enumerate() {
            let mut all = true;
            for test in *tests {
                all &= test.passes(inventory)?;
            }
            if all {
                passed |= 1 << column;
            }
        }
        Some(passed)
    }
}

impl Test {
    /// Whether the text passes, or `None` when it can't say honestly.
    fn passes(&self, inventory: &Inventory) -> Option<bool> {
        match self {
            Test::Last(place) => {
                let chapter = inventory.chapter(place)?;
                (!chapter.defective()).then(|| chapter.last() == Some(place.verse))
            }
            Test::Exist(place) => has(inventory, place),
            Test::NotExist(place) => {
                let chapter = inventory.chapter(place)?;
                if place.part.is_none() && chapter.defective() {
                    return None;
                }
                has(inventory, place).map(|has| !has)
            }
            Test::Compare {
                left,
                left_factor,
                longer,
                right,
                right_factor,
            } => {
                let a = u64::from(chars(inventory, left)?) * u64::from(*left_factor);
                let b = u64::from(chars(inventory, right)?) * u64::from(*right_factor);
                Some(if *longer { a > b } else { a < b })
            }
            Test::Unsupported(_) => None,
        }
    }
}

fn has(inventory: &Inventory, place: &Place) -> Option<bool> {
    let chapter = inventory.chapter(place)?;
    Some(match place.part {
        Some(part) => chapter.parts.contains(&(place.verse, part)),
        None => chapter.verses.contains_key(&place.verse),
    })
}

/// A whole, unbridged verse's length.
fn chars(inventory: &Inventory, place: &Place) -> Option<u32> {
    let facts = inventory.chapter(place)?.verses.get(&place.verse)?;
    (!facts.bridged).then_some(facts.chars)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEN: BookKey = BookKey::new(*b"GEN");

    /// An inventory with every verse `scheme` counts for the given books,
    /// each `chars` long.
    fn as_numbered_by(scheme: &str, books: &[&[u8; 3]]) -> Inventory {
        let scheme = crate::scheme(scheme).unwrap();
        let mut inventory = Inventory::new();
        for code in books {
            let book = BookKey::new(**code);
            for (at, &last) in scheme.verse_counts(book).unwrap().iter().enumerate() {
                let chapter = u16::try_from(at + 1).unwrap();
                for verse in 1..=last {
                    inventory.add(book, chapter, verse, verse, None, 100);
                }
            }
        }
        inventory
    }

    #[test]
    fn a_text_numbered_like_org_sniffs_as_org_and_like_eng_as_eng() {
        let ot: &[&[u8; 3]] = &[
            b"GEN", b"EXO", b"LEV", b"NUM", b"1SA", b"PSA", b"JOL", b"MAL",
        ];
        let org = sniff(&as_numbered_by("org", ot));
        assert_eq!(org.ranked[0].scheme.name(), "org", "{:?}", org.ranked);
        let eng = sniff(&as_numbered_by("eng", ot));
        assert!(
            ["eng", "ulb"].contains(&eng.ranked[0].scheme.name()),
            "{:?}",
            eng.ranked
        );
        assert_eq!(eng.ranked[0].disagree, 0);
    }

    #[test]
    fn an_empty_text_is_undetermined() {
        let none = sniff(&Inventory::new());
        assert_eq!(none.confidence, Confidence::Undetermined);
        assert_eq!(none.voted, 0);
        assert!(none.best().is_none());
    }

    #[test]
    fn a_gap_out_of_order_or_duplicate_makes_a_chapter_defective() {
        let mut inventory = Inventory::new();
        for verse in [1, 2, 4] {
            inventory.add(GEN, 1, verse, verse, None, 10);
        }
        for verse in [1, 3, 2] {
            inventory.add(GEN, 2, verse, verse, None, 10);
        }
        for verse in [1, 2, 2] {
            inventory.add(GEN, 3, verse, verse, None, 10);
        }
        for (first, last, part) in [
            (1, 1, None),
            (2, 3, None),
            (4, 4, Some('a')),
            (4, 4, Some('b')),
        ] {
            inventory.add(GEN, 4, first, last, part, 10);
        }
        // A whole verse 3 after its parts, in both orders.
        for (verse, part) in [
            (1, None),
            (2, None),
            (3, Some('a')),
            (3, Some('b')),
            (3, None),
        ] {
            inventory.add(GEN, 5, verse, verse, part, 10);
        }
        for (verse, part) in [(1, None), (2, None), (3, None), (3, Some('a'))] {
            inventory.add(GEN, 6, verse, verse, part, 10);
        }
        let defective: Vec<u16> = inventory.defective().map(|(_, c)| c).collect();
        assert_eq!(
            defective,
            [1, 2, 3, 5, 6],
            "chapter 4's bridge and parts are fine"
        );
    }

    /// GEN 31:55 ends chapter 31 in eng; org numbers it 32:1. A text that
    /// drops it has no gap to see (nothing follows it), so the chapter isn't
    /// defective and the rule "is 31:55 last?" votes against eng. One rule
    /// out of many: the text still sniffs as eng, with that rule as dissent.
    #[test]
    fn a_missing_last_verse_is_dissent_not_a_new_scheme() {
        let pentateuch: &[&[u8; 3]] = &[b"GEN", b"EXO", b"LEV", b"NUM"];
        let mut inventory = Inventory::new();
        let eng = crate::scheme("eng").unwrap();
        for code in pentateuch {
            let book = BookKey::new(**code);
            for (at, &last) in eng.verse_counts(book).unwrap().iter().enumerate() {
                let chapter = u16::try_from(at + 1).unwrap();
                for verse in 1..=last {
                    if (book, chapter, verse) != (GEN, 31, 55) {
                        inventory.add(book, chapter, verse, verse, None, 100);
                    }
                }
            }
        }
        let sniff = sniff(&inventory);
        assert!(["eng", "ulb"].contains(&sniff.ranked[0].scheme.name()));
        assert!(sniff.ranked[0].disagree <= 1, "{:?}", sniff.ranked);
    }

    #[test]
    fn every_rule_has_one_fit_mask_per_scheme_within_its_columns() {
        for rule in RULES {
            assert_eq!(rule.fits.len(), schemes().len(), "{}", rule.name);
            assert_eq!(rule.tests.len(), rule.columns.len(), "{}", rule.name);
            let all = (1u16 << rule.columns.len()) - 1;
            for &fit in rule.fits {
                assert_eq!(u16::from(fit) & !all, 0, "{}", rule.name);
            }
        }
    }
}
