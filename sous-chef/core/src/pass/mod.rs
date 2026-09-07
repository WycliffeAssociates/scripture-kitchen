//! One chapter in, one detached observation out; one book's observations in,
//! one aggregate out; every book's aggregates in, findings out.
//!
//! ```text
//! analyze(corpus, &HygieneBytes)
//!   MRK chapter 1 at 0   "wept \\ here.\0\0\0"  → map → [StrandedBackslash 5..7, C0Control 13..16]
//!   MRK chapter 2 at 16  "An \0 more."          → map → [C0Control 3..4]
//!   fold([obs at 0, obs at 16])   → [StrandedBackslash 5..7, C0Control 13..16, C0Control 19..20]
//!   judge([&MRK aggregate], &(), out)
//!     → target[0] MRK  5..7   StrandedBackslash run 2
//!       target[0] MRK 13..16  C0Control run 3
//!       target[0] MRK 19..20  C0Control run 1
//! ```
//!
//! [`ChapterPass::map`] reads one chapter and nothing else, so a host may run
//! it in any order or retain its result. Neither fold nor judge can tell a
//! cached input from a fresh one; that is what makes cold and incremental
//! analysis equal. The fold and judge rules in full: pass.md.

use core::ops::Range;

use crate::{
    BookIndex, BookKey, Chapter, CodecError, Corpus, FindingKind, PackedFinding, ProjectedBook,
    TextRange, Verse,
    judge::{Pattern, PatternIndex, TerminalTable},
    proportionality::{LengthConfig, Paired, SourceLengths, TargetLengths, judge_lengths},
    substrate::VerseLength,
    words::{MovedWords, WordTotals, WordVerdicts},
};

mod chapter;
mod drivers;
mod findings;
#[cfg(test)]
mod tests;

pub use chapter::{ChapterInput, ChapterKey, ChapterObs, ChapterPass, CorpusTotals, SchemaStamp};
pub(crate) use drivers::collect_verses;
pub use drivers::{analyze, analyze_paired, analyze_with, for_each_chapter};
pub use findings::Findings;
