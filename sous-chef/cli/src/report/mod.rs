//! The `--report` page: one self-contained HTML file, the v1 "Punctuation &
//! Symbol Inventory" page fed by v2 counts.
//!
//! ```text
//! sous --report debug/inventory-en_ulb.html testData/exampleCorpora/en_ulb
//!   -> debug/inventory-en_ulb.html
//!      "Character by character" tab: one card per non-letter glyph, plain
//!      English, amber wherever the current `JudgingConfig` fired.
//! ```
//!
//! [`templates/inventory.html`](../../templates/inventory.html) is the v1
//! artifact with its host wrapper stripped and `const CORPORA = [...]`
//! replaced by `const CORPORA = @@CORPORA@@;`, which [`render`] substitutes
//! with one corpus record for the target corpus. The "Tune the judge" and
//! "Capitalization" tabs (and their `const CAP` data and JS judge) are gone;
//! amber now means "a row in `Findings::patterns()` fired", carried as a
//! `flag` field, never recomputed in JS.
//!
//! Key mapping (template key read -> v2 source), everything else copied
//! from the v1 record shape verbatim:
//!
//! | key | source |
//! |---|---|
//! | `name` | the corpus path's file/dir name |
//! | `judging` | [`JudgingConfig::default`], as text |
//! | `glyphs[].g`/`cp`/`uname` | [`ScalarKey::scalar`], `"digits"` for [`ScalarKey::DIGITS`] |
//! | `glyphs[].total`/`books` | merged [`BookAggregate::scalars`] |
//! | `glyphs[].side.{start,end}` | merged [`BookAggregate::pairs`] (letter/space/edge/digit), split by [`Pool`] from [`BookAggregate::runs`] for a `Nonletter` neighbor |
//! | `glyphs[].topo.*.n`/`.books` | merged [`BookAggregate::pairs`], by (prev, next) combination |
//! | `glyphs[].topo.*.flag` | a [`PatternKey::Placement`] row on the matching side/class, or (in-run) any [`PatternKey::RunShape`] row |
//! | `glyphs[].pairs[].{p,n,books}` | merged [`BookAggregate::runs`], the atom immediately after the glyph in a run |
//! | `glyphs[].pairs[].flag` | a [`PatternKey::ExactNeighbor`] row for that partner |
//! | `glyphs[].pairs[].pool` | [`pool_of`] on the partner — a heading only, groups the table, no new numbers |
//! | `glyphs[].runlen.pure` / `.mixed` | merged [`BookAggregate::runs`], runs made entirely of the glyph / runs holding it beside other marks, by length (6 = 6+) |
//! | `glyphs[].runlen.*[].flag` | the [`PatternKey::RunShape`] row with that purity at that bucket |
//! | `glyphs[].rarity.flag` | a [`PatternKey::Rarity`] row for the glyph |
//! | `glyphs[].rarity.samples` | every occurrence, capped at 8, so a glyph whose sole claim is [`PatternKey::Rarity`] still shows one — the gap this closes: `sites::locate` headlines a run by its finest matched pattern, so a rare glyph inside a run some commoner glyph's pattern also headlines listed nowhere before |
//! | `*.samples` | up to 8 per bucket, from one text scan per glyph per book, classified with [`sous_core::sites::Cursor`] |
//!
//! `uname` has no Unicode name lookup (no `unicode_names2` dependency exists
//! in this workspace): it repeats `cp` beside the scalar itself.
//!
//! The **Capitalization** tab reads one more key, `cap[]`, one entry per
//! [`PatternKey::Casing`], [`PatternKey::Doubled`] or [`PatternKey::LetterRun`]
//! row: `kind` says which, `w` is the word (or the pair) its first site landed
//! on, `form` the flagged minority form, `bare`/`separated`, or the letter and
//! its run length, `n`/`d` the fraction, `books` the dispersion, and `samples`
//! up to 8 of that row's sites in the tuple shape the glyph cards use. Amber
//! is the row's own existence — the Rust judge fired it — never a JS
//! recomputation.

use rustc_hash::{FxHashMap, FxHashSet};

use sous_core::judge::{Channel, PatternKey, Side};
use sous_core::sites::Cursor;
use sous_core::substrate::{BookAggregate, ChapterRow, OuterClass, PairKey, ScalarKey};
use sous_core::unicode::{Pool, class_of, pool_of};
use sous_core::{
    Chapter, ChapterObs, ChapterPass, Corpus, FindingKind, JudgingConfig, PackedFinding, Pattern,
    ProjectedBook, Staircase, Substrate, Verse, for_each_chapter,
};
use usfm_galley::sous::OnionBook;

mod corpus;
mod glyphs;
mod page;
mod samples;
#[cfg(test)]
mod tests;

use corpus::{AfterRow, Merged, Sample, corpus_json, pool_name};
use glyphs::glyph_object;
pub use page::{CopyUnit, Paired, PairedUnit, PresenceUnit, render};
use page::{SAMPLE_CAP, SNIPPET_CONTEXT, TOPO, copies_json, lengths_json, presence_json};
use samples::{build_sample, harvest_samples, json_str, samples_json};
