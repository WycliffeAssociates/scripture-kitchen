//! Corpus-level judging over the substrate's counts: one row per firing
//! pattern, no sites.
//!
//! ```text
//! judge([&MRK, &GEN], &JudgingConfig::default(), out)
//!   ',' occurs 9,812 times, 12 of them before a letter
//!     → Placement { side: Next, class: Letter }   12/9,812   12 bp   band 3
//!   '?' opens 403 in-run pairs, 3 of them before '.'
//!     → ExactNeighbor('.')                         3/403     74 bp   band 2
//!   ',..,' once against 600 lone commas
//!     → RunShape { pure: false, bucket: 4 }        1/601     16 bp   band 2
//!   '`' occurs once in 48,213 scalars
//!     → Rarity                                     1/48,213
//! ```
//!
//! Config reaches judging and nothing else, so a host re-judges without
//! remapping a chapter or refolding a book. The ladder, the entitlement rule,
//! and the emission order: judge.md.

use rustc_hash::FxHashMap;

use crate::pass::Findings;
use crate::proportionality::LengthConfig;
use crate::substrate::{BookAggregate, Case, FollowCounts, OuterClass, RUN_BUCKETS, ScalarKey};
use mise::unicode::class_of;

use crate::unicode::{Pool, pool_of};
use crate::words::{
    DoubleTally, Form, LETTER_RUN_MAX, LETTER_RUN_MIN, MovedWords, RunTally, WordAggregate,
    WordTally, WordTotals, letter_run_lane,
};

mod config;
mod pattern;
mod scalars;
mod terminals;
#[cfg(test)]
mod tests;
mod words;

pub use config::{
    BandStep, Channels, DoublesPolicy, JudgingConfig, LetterRoster, Staircase, config_stamp,
};
pub use pattern::{Channel, Pattern, PatternIndex, PatternKey, Side};
pub use scalars::books_touched;
pub(crate) use scalars::{judge_corpus, pool_of_key, share_bp};
use scalars::{reported_share, saturate};
use terminals::seek;
pub use terminals::{TerminalTable, merged_follows};
pub(crate) use words::{free_of, judge_words, judge_words_for, judges_doubles, word_slot};
