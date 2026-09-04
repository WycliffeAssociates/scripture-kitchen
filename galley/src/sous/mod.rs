//! Onion → Sous findings publication: the coordinate seam galley owns.
//!
//! ```text
//! publish_onion_findings(
//!     vec![OnionInputBook::new("books/mrk.usfm", "\\id MRK\n\\c 1\n\\p\n\\v 1 Jesus \\f + \\ft note\\f* wept.\n".into())],
//!     &[finding],            // projected-book UTF-8: 0..13 over "Jesus  wept.\n"
//!     &[],                   // no pattern fired
//!     snapshot,
//! )
//!   → SOUS corpus buffer, CoordinateSpace::Utf16
//!       directory  MRK  books/mrk.usfm  published_len 50  count 1
//!       record     from 21  to 50                        // raw-book UTF-16
//! ```
//!
//! [`publish_onion_findings`] is the COLD path, deriving mask, TOC, and UTF-16
//! index per invocation and keeping nothing; [`Expediter`] is the resident one,
//! publishing the same bytes from a Pantry's retained products. Both rebase
//! through the same core, so a projected range crossing removed markup
//! publishes the BOUNDING raw range either way. Envelope layout and coordinate
//! contract: `sous-chef/core/src/codec/README.md`.

mod expediter;
mod onion_book;

use core::fmt;
use core::ops::Range;

use rustc_hash::FxHashSet;
use sous_core::{
    BookKey, CodecError, CoordinateSpace, CorpusWireError, InputError, PackedFinding, Pattern,
    PublicationBook, SnapshotId, encode_to_corpus_buffer,
};

use crate::onion::{Filter, Mask, Utf16Index, cst, lex, mask, toc, utf16_index};
use crate::pantry::{BookId, PantryError};

pub use expediter::{Expediter, ObservationKey};
pub use onion_book::{LocatedRange, OnionBook, SourceSpans};

/// One complete raw USFM book under its host id, owned for the duration of the
/// invocation.
pub struct OnionInputBook {
    id: BookId,
    source: String,
}

impl OnionInputBook {
    pub fn new(id: impl Into<BookId>, source: String) -> Self {
        Self {
            id: id.into(),
            source,
        }
    }
}

/// A projected-book span as its BOUNDING raw span, converted by `to_published`.
///
/// First retained byte through last: a span crossing removed markup stays one
/// navigation row rather than pretending the raw bytes were contiguous.
fn rebase_span(mask: &Mask, to_published: impl Fn(u32) -> u32, span: Range<u32>) -> (u32, u32) {
    if span.start == span.end {
        let at = to_published(mask.to_source(span.start));
        return (at, at);
    }
    // `end` is exclusive and char-aligned, so `end - 1` names a byte of the
    // last retained character and `+ 1` lands back on a char boundary.
    (
        to_published(mask.to_source(span.start)),
        to_published(mask.to_source(span.end - 1) + 1),
    )
}

/// Refuses a span [`rebase_span`] cannot carry: past the projection, or
/// splitting one of its characters.
fn check_projected(projected: &str, mask: &Mask, span: Range<u32>) -> Result<(), SpanError> {
    if span.end > mask.len() {
        return Err(SpanError::OutOfBounds {
            projected_len: mask.len(),
        });
    }
    for offset in [span.start, span.end] {
        if !projected.is_char_boundary(offset as usize) {
            return Err(SpanError::NotCharBoundary { offset });
        }
    }
    Ok(())
}

/// Why one projected span is not publishable, before it names its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpanError {
    OutOfBounds { projected_len: u32 },
    NotCharBoundary { offset: u32 },
}

/// Rebase projected-book UTF-8 findings to raw-book UTF-16 and encode one
/// complete corpus publication in caller book order.
pub fn publish_onion_findings(
    books: Vec<OnionInputBook>,
    findings: &[PackedFinding],
    patterns: &[Pattern],
    snapshot: SnapshotId,
) -> Result<Vec<u8>, PublishError> {
    struct Derived<'s> {
        key: BookKey,
        mask: Mask,
        projected: String,
        utf16: Utf16Index<'s>,
    }

    let mut derived = Vec::with_capacity(books.len());
    let mut ids = FxHashSet::with_capacity_and_hasher(books.len(), rustc_hash::FxBuildHasher);
    for (index, book) in books.iter().enumerate() {
        let bytes = book.source.as_bytes();
        let tokens = lex(&book.source);
        let tree = cst::build(&tokens);
        let toc = toc(bytes, &tokens);
        if toc.book_token.is_none() {
            return Err(PublishError::MissingBookKey { book: index });
        }
        if !ids.insert(book.id.clone()) {
            return Err(PublishError::DuplicateBookId {
                id: book.id.clone(),
            });
        }
        let mask = mask(bytes, &tokens, &tree, &Filter::verse_text());
        let projected = mask.text(bytes);
        derived.push(Derived {
            key: BookKey::new(toc.book),
            mask,
            projected,
            utf16: utf16_index(bytes),
        });
    }
    let published_lens: Vec<u32> = derived.iter().map(|book| book.utf16.len_utf16()).collect();

    let mut per_book: Vec<Vec<PackedFinding>> = (0..books.len()).map(|_| Vec::new()).collect();
    for (row, finding) in findings.iter().enumerate() {
        let index = usize::from(finding.book_idx().get());
        let Some(book) = derived.get(index) else {
            return Err(PublishError::BookIndexOutOfRange {
                row,
                index: finding.book_idx().get(),
                book_count: books.len(),
            });
        };
        let (from, to) = (finding.from(), finding.to());
        check_projected(&book.projected, &book.mask, from..to)
            .map_err(|error| PublishError::span(row, from, to, error))?;
        let (published_from, published_to) =
            rebase_span(&book.mask, |byte| book.utf16.to_utf16(byte), from..to);
        let rebased = PackedFinding::new(
            published_from,
            published_to,
            finding.book_idx(),
            finding.kind(),
            &published_lens,
        )
        .map_err(|error| PublishError::Rebase { row, error })?;
        per_book[index].push(rebased);
    }

    let sections: Vec<PublicationBook<'_>> = derived
        .iter()
        .zip(&books)
        .zip(&per_book)
        .zip(&published_lens)
        .map(|(((book, input), findings), &published_len)| {
            PublicationBook::new(book.key, input.id.as_str(), published_len, findings)
        })
        .collect();
    encode_to_corpus_buffer(snapshot, CoordinateSpace::Utf16, &sections, patterns)
        .map_err(PublishError::Wire)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishError {
    /// The book at this caller position has no `\id` line to key it.
    MissingBookKey {
        book: usize,
    },
    /// Two input books under one host id; `\id` may repeat, an id may not.
    DuplicateBookId {
        id: BookId,
    },
    /// The Pantry refused the update behind this publication.
    Pantry(PantryError),
    /// A registered book keeps no text, so the Expediter cannot project it to
    /// map or locate. A `Target` is refused at
    /// [`Expediter::update_with`](Expediter::update_with) rather than reaching
    /// here; this is the refusal a text-less role answers with.
    NoText {
        id: BookId,
    },
    /// The book's projection is not a valid `sous-core` input.
    InvalidBook {
        id: BookId,
        error: InputError,
    },
    BookIndexOutOfRange {
        row: usize,
        index: u16,
        book_count: usize,
    },
    /// The finding's projected range does not fit its book's projection.
    SpanOutOfBounds {
        row: usize,
        from: u32,
        to: u32,
        projected_len: u32,
    },
    /// A range endpoint splits a character of the projected text.
    NotCharBoundary {
        row: usize,
        offset: u32,
    },
    Rebase {
        row: usize,
        error: CodecError,
    },
    Wire(CorpusWireError),
}

impl fmt::Display for PublishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBookKey { book } => {
                write!(f, "book {book} has no \\id line to key it")
            }
            Self::DuplicateBookId { id } => write!(f, "duplicate input book id {id}"),
            Self::Pantry(error) => write!(f, "{error}"),
            Self::NoText { id } => write!(f, "book {id} retains no text to project"),
            Self::InvalidBook { id, error } => write!(f, "book {id} is not analyzable: {error}"),
            Self::BookIndexOutOfRange {
                row,
                index,
                book_count,
            } => write!(
                f,
                "finding {row} names book {index}; the invocation has {book_count} books"
            ),
            Self::SpanOutOfBounds {
                row,
                from,
                to,
                projected_len,
            } => write!(
                f,
                "finding {row} span {from}..{to} exceeds projected length {projected_len}"
            ),
            Self::NotCharBoundary { row, offset } => {
                write!(
                    f,
                    "finding {row} offset {offset} splits a projected character"
                )
            }
            Self::Rebase { row, error } => write!(f, "finding {row} failed to rebase: {error}"),
            Self::Wire(error) => write!(f, "corpus encoding failed: {error}"),
        }
    }
}

impl PublishError {
    fn span(row: usize, from: u32, to: u32, error: SpanError) -> Self {
        match error {
            SpanError::OutOfBounds { projected_len } => Self::SpanOutOfBounds {
                row,
                from,
                to,
                projected_len,
            },
            SpanError::NotCharBoundary { offset } => Self::NotCharBoundary { row, offset },
        }
    }
}

impl std::error::Error for PublishError {}

#[cfg(test)]
mod tests {
    use super::*;
    use sous_core::{
        BookIndex, CorpusSnapshot, FindingKind, ProportionalityDigest, QuantizedDeviation,
    };

    const MRK: &str = concat!(
        "\\id MRK\n",
        "\\c 1\n\\p\n",
        "\\v 1 Jesus \\f + \\ft note\\f* wept.\n",
        "\\v 2-3 Two and three.\n",
        "\\c 2\n\\p\n",
        "\\v 1 An 🧅.\n",
    );
    const GEN: &str = concat!("\\id GEN\n", "\\c 1\n\\p\n", "\\v 1 In the beginning.\n",);

    fn projected(source: &str) -> String {
        let tokens = lex(source);
        let tree = cst::build(&tokens);
        mask(source.as_bytes(), &tokens, &tree, &Filter::verse_text()).text(source.as_bytes())
    }

    fn kind(book_raw: Option<i16>, project_raw: Option<i16>, saturated: bool) -> FindingKind {
        FindingKind::LengthProportionality(ProportionalityDigest::new(
            book_raw.map(|raw| QuantizedDeviation::from_raw(raw).unwrap()),
            project_raw.map(|raw| QuantizedDeviation::from_raw(raw).unwrap()),
            saturated,
        ))
    }

    fn finding(from: u32, to: u32, book: usize, kind: FindingKind) -> PackedFinding {
        // A generous fake book table keeps these tests about publish's own
        // validation, not the caller's.
        PackedFinding::new(
            from,
            to,
            BookIndex::new(book).unwrap(),
            kind,
            &[u32::MAX; 4],
        )
        .unwrap()
    }

    fn range_of(projected: &str, needle: &str) -> (u32, u32) {
        let from = projected.find(needle).unwrap();
        (from as u32, (from + needle.len()) as u32)
    }

    fn books() -> Vec<OnionInputBook> {
        vec![
            OnionInputBook::new("books/mrk.usfm", MRK.to_string()),
            OnionInputBook::new("books/gen.usfm", GEN.to_string()),
        ]
    }

    /// Caller order MRK before GEN (non-canonical), with one split-mask, one
    /// astral, and one plain finding.
    fn fixture_publication() -> Vec<u8> {
        let mrk = projected(MRK);
        let genesis = projected(GEN);
        let (split_from, split_to) = range_of(&mrk, "Jesus  wept.\n");
        let (astral_from, astral_to) = range_of(&mrk, "🧅.");
        let (plain_from, plain_to) = range_of(&genesis, "beginning");
        let findings = [
            finding(
                split_from,
                split_to,
                0,
                kind(Some(0x0180), Some(-0x0180), true),
            ),
            finding(astral_from, astral_to, 0, kind(None, Some(0x0080), false)),
            finding(plain_from, plain_to, 1, kind(Some(-0x0040), None, false)),
        ];
        publish_onion_findings(
            books(),
            &findings,
            &[],
            SnapshotId::new(core::array::from_fn(|index| index as u8)),
        )
        .unwrap()
    }

    #[test]
    fn split_mask_finding_publishes_the_bounding_utf16_range() {
        // "Jesus " is raw 21..27 and " wept.\n" is raw 43..50, with the
        // removed footnote between. The span bounds both, all ASCII.
        let buffer = fixture_publication();
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        assert_eq!(snapshot.coordinate_space(), CoordinateSpace::Utf16);
        let mark = snapshot.book_by_key(BookKey::new(*b"MRK")).unwrap();
        let row = mark.at(0).unwrap();
        assert_eq!((row.from(), row.to()), (21, 50));
    }

    #[test]
    fn astral_finding_publishes_surrogate_pair_correct_utf16() {
        // The onion is 4 UTF-8 bytes but 2 UTF-16 units, so raw 88..93
        // publishes as 88..91 and MRK's published length is 92.
        let buffer = fixture_publication();
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        let mark = snapshot.book_by_key(BookKey::new(*b"MRK")).unwrap();
        assert_eq!(mark.published_len(), 92);
        let row = mark.at(1).unwrap();
        assert_eq!((row.from(), row.to()), (88, 91));
    }

    #[test]
    fn caller_book_order_and_plain_ascii_rebase_are_preserved() {
        let buffer = fixture_publication();
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        assert_eq!(
            snapshot.book(BookIndex::new(0).unwrap()).unwrap().key(),
            BookKey::new(*b"MRK"),
            "caller order wins over canonical order"
        );
        let genesis = snapshot.book(BookIndex::new(1).unwrap()).unwrap();
        assert_eq!(genesis.key(), BookKey::new(*b"GEN"));
        assert_eq!(genesis.published_len(), 39);
        let row = genesis.at(0).unwrap();
        assert_eq!((row.from(), row.to()), (28, 37));
    }

    #[test]
    fn publication_matches_the_shared_golden_buffer() {
        let golden: Vec<u8> = include_str!("../../../sous-chef/testdata/corpus_v1_utf16.hex")
            .split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).unwrap())
            .collect();
        assert_eq!(fixture_publication(), golden);
    }

    #[test]
    fn empty_span_rebases_to_an_empty_utf16_span() {
        let mrk = projected(MRK);
        let (at, _) = range_of(&mrk, "Two");
        let buffer = publish_onion_findings(
            books(),
            &[finding(at, at, 0, kind(None, None, false))],
            &[],
            SnapshotId::new([0; 16]),
        )
        .unwrap();
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        let row = snapshot
            .book(BookIndex::new(0).unwrap())
            .unwrap()
            .at(0)
            .unwrap();
        assert_eq!((row.from(), row.to()), (57, 57), "raw 'T' of Two");
    }

    /// Two files of one `\id`: the string table, not the key, tells them apart.
    #[test]
    fn one_id_line_under_two_ids_publishes_two_rows() {
        let buffer = publish_onion_findings(
            vec![
                OnionInputBook::new("a/gen-copy.usfm", GEN.to_string()),
                OnionInputBook::new("a/gen.usfm", GEN.to_string()),
            ],
            &[],
            &[],
            SnapshotId::new([0; 16]),
        )
        .unwrap();
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(
            snapshot.book_by_id("a/gen.usfm").unwrap().index().get(),
            1,
            "the second row is reachable only by id"
        );
        assert_eq!(
            snapshot.book_by_key(BookKey::new(*b"GEN")).unwrap().id(),
            "a/gen-copy.usfm"
        );
    }

    #[test]
    fn empty_findings_still_publish_every_book_directory_row() {
        let buffer = publish_onion_findings(books(), &[], &[], SnapshotId::new([7; 16])).unwrap();
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot.book(BookIndex::new(0).unwrap()).unwrap().len(), 0);
    }

    #[test]
    fn validation_failures_are_typed() {
        assert_eq!(
            publish_onion_findings(
                vec![OnionInputBook::new(
                    "scratch.usfm",
                    "\\c 1\n\\p\n\\v 1 keyless\n".to_string()
                )],
                &[],
                &[],
                SnapshotId::new([0; 16]),
            ),
            Err(PublishError::MissingBookKey { book: 0 })
        );

        assert_eq!(
            publish_onion_findings(
                vec![
                    OnionInputBook::new("a/gen.usfm", GEN.to_string()),
                    OnionInputBook::new("a/gen.usfm", GEN.to_string()),
                ],
                &[],
                &[],
                SnapshotId::new([0; 16]),
            ),
            Err(PublishError::DuplicateBookId {
                id: BookId::from("a/gen.usfm")
            })
        );

        assert_eq!(
            publish_onion_findings(
                books(),
                &[finding(0, 0, 3, kind(None, None, false))],
                &[],
                SnapshotId::new([0; 16]),
            ),
            Err(PublishError::BookIndexOutOfRange {
                row: 0,
                index: 3,
                book_count: 2,
            })
        );

        let projected_len = projected(GEN).len() as u32;
        assert_eq!(
            publish_onion_findings(
                books(),
                &[finding(0, projected_len + 1, 1, kind(None, None, false))],
                &[],
                SnapshotId::new([0; 16]),
            ),
            Err(PublishError::SpanOutOfBounds {
                row: 0,
                from: 0,
                to: projected_len + 1,
                projected_len,
            })
        );

        let mrk = projected(MRK);
        let (onion_at, _) = range_of(&mrk, "🧅");
        assert_eq!(
            publish_onion_findings(
                books(),
                &[finding(
                    onion_at + 2,
                    onion_at + 4,
                    0,
                    kind(None, None, false)
                )],
                &[],
                SnapshotId::new([0; 16]),
            ),
            Err(PublishError::NotCharBoundary {
                row: 0,
                offset: onion_at + 2,
            })
        );
    }
}
