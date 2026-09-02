//! Language-agnostic scripture-content analysis.
//!
//! This crate owns evidence and judgment, not USFM parsing, source mapping,
//! I/O, or resident caches. Producers enter through [`ProjectedBook`].

mod alignment;
mod codec;
mod corpus;
pub mod hygiene;
mod input;
mod pass;
pub mod unicode;

pub use alignment::{AlignedSide, AlignedUnit, Alignment, AlignmentFact, align};
pub use codec::{
    CodecError, FindingFlags, FindingKind, HygieneClass, HygieneDigest, PackedFinding,
    ProportionalityDigest, QuantizedDeviation, RECORD_LEN, RuleCode,
};
pub use corpus::{
    CoordinateSpace, CorpusBook, CorpusSnapshot, CorpusWireError, DIRECTORY_ENTRY_BYTES,
    FLAG_UTF16, FORMAT_VERSION, HEADER_BYTES, MAGIC, PublicationBook, SnapshotId,
    encode_to_corpus_buffer, generated_reader_ts,
};
pub use input::{
    BookIndex, BookKey, Chapter, Corpus, InputError, ProjectedBook, TextRange, Verse, VerseKey,
    validate,
};
pub use pass::{ChapterInput, ChapterKey, ChapterObs, ChapterPass, Findings, SchemaStamp, analyze};
