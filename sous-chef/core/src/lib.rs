//! Language-agnostic scripture-content analysis.
//!
//! This crate owns evidence and judgment, not USFM parsing, source mapping,
//! I/O, or resident caches. Producers enter through [`ProjectedBook`].

mod alignment;
mod codec;
mod corpus;
pub mod hygiene;
mod input;
pub mod judge;
mod pass;
pub mod sites;
pub mod substrate;
pub mod unicode;
pub mod words;

pub use alignment::{AlignedSide, AlignedUnit, Alignment, AlignmentFact, align};
pub use codec::{
    CodecError, ConventionDigest, FindingFlags, FindingKind, HygieneClass, HygieneDigest,
    PackedFinding, ProportionalityDigest, QuantizedDeviation, RECORD_LEN, Reasons, RuleCode,
};
pub use corpus::{
    CoordinateSpace, CorpusBook, CorpusSnapshot, CorpusWireError, DIRECTORY_ENTRY_BYTES,
    DIRECTORY_ID_OFFSET, FLAG_UTF16, FORMAT_VERSION, HEADER_BYTES, ID_PREFIX_BYTES, MAGIC,
    PATTERN_ROW_LEN, PublicationBook, SECTION_ALIGNMENT, SnapshotId, encode_to_corpus_buffer,
    generated_reader_ts,
};
pub use input::{
    BookIndex, BookKey, Chapter, Corpus, InputError, ProjectedBook, TextRange, Verse, VerseKey,
    validate,
};
pub use judge::{
    BandStep, Channel, Channels, JudgingConfig, LetterRoster, Pattern, PatternIndex, PatternKey,
    Side, Staircase, books_touched,
};
pub use pass::{
    ChapterInput, ChapterKey, ChapterObs, ChapterPass, CorpusTotals, Findings, SchemaStamp,
    analyze, analyze_with, for_each_chapter,
};
pub use sites::Site;
pub use substrate::{
    BookAggregate, Case, ChapterRow, Edge, FollowCounts, OuterClass, PairKey, RUN_BUCKETS,
    RunLengths, ScalarKey, Substrate,
};
pub use words::{Form, WordAggregate, WordCount, WordRow, WordTally, WordTotal, WordTotals, Words};

/// The product pass: hygiene's byte sweeps, the substrate walk, and the word
/// walk over the same chapters, judged into one span-ordered row set per book.
///
/// Its config is `((), JudgingConfig, JudgingConfig)`. `Substrate` and `Words`
/// judge under the same struct, so a host places the same `Copy` value in both
/// slots; sharing one config across passes without the copy is later plumbing.
pub type Brigade = (hygiene::HygieneBytes, substrate::Substrate, words::Words);
