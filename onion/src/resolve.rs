//! An address to its place in one text: found, missing or ambiguous.
//!
//! ```text
//! \c 1\n\p\n\v 1-2 a b\n\v 3 c     LUK 1:2  → Found 8..19, opening 5, covers LUK 1:1-2
//! \c 1\n\p\n\v 1,3 a\n\v 4 b       LUK 1:2  → Missing verse, within 0..24
//! \c 3\n\p a\n\c 3\n\p b\n         LUK 3    → Ambiguous [0..10, 10..20]
//! ```
//!
//! The rules, one example each, and where they depart from Sefer's: `resolve.md`.

use core::ops::Range;

pub use mise::address::{Address, Point};

use crate::toc::Toc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Found {
        span: Range<u32>,
        /// Where the run of openers before the first `\v` starts; `span.start`
        /// mid-paragraph and for every answer that is not a verse.
        opening: u32,
        /// What the span is: `LUK 1:1-2` when `LUK 1:2` landed in a bridge.
        covers: Address,
        /// A segment found as its whole verse, because the text does not divide it.
        coarser: bool,
    },
    Missing {
        missing: Missing,
        /// The chapter a missing verse would be in.
        within: Option<Range<u32>>,
    },
    Ambiguous(Vec<Candidate>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    Intro,
    Chapter,
    Verse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub span: Range<u32>,
    pub opening: u32,
}

impl Toc {
    /// `source` is the text this `Toc` was built from.
    pub fn resolve(&self, _source: &str, _address: &Address) -> Resolution {
        Resolution::Missing {
            missing: Missing::Chapter,
            within: None,
        }
    }
}
