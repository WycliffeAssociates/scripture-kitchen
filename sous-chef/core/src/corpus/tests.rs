//! Coverage for the corpus wire: encode/decode round trips, the error
//! table, and `reader.ts` staying generated from these same constants.

use super::*;
use crate::codec::{CodecError, HygieneClass, PresenceDigest, PresenceKind, SourceCopyDigest};
use crate::judge::{Channel, PatternKey, Side, Usual};
use crate::substrate::{OuterClass, ScalarKey};
use crate::unicode::Pool;
use crate::words::Form;
use crate::{
    ConventionDigest, FindingKind, HygieneDigest, PatternIndex, ProportionalityDigest,
    QuantizedDeviation, Reasons,
};

const GENERATED: &str = include_str!("../../../reader.ts");
/// The same reader, as the `usfm-galley` package exports it.
const PACKAGED: &str = include_str!("../../../../galley/sous-reader.ts");
/// One book under `books/mrk.usfm`: header, one directory row, a padded
/// 16-byte id table, then the records.
const FIRST_RECORD: usize = HEADER_BYTES + DIRECTORY_ENTRY_BYTES + 16;
/// One of each channel and key shape, in emission order.
fn fixture_patterns() -> Vec<Pattern> {
    vec![
        Pattern {
            glyph: ScalarKey::of('`'),
            channel: Channel::Rarity,
            key: PatternKey::Rarity,
            band: None,
            numerator: 1,
            denominator: 48_213,
            share_bp: 0,
            books: 1,
            usual: Usual::Rarity {
                glyph: Some(ScalarKey::of('~')),
                count: 12,
            },
        },
        Pattern {
            glyph: ScalarKey::of('?'),
            channel: Channel::ExactNeighbor,
            key: PatternKey::ExactNeighbor(ScalarKey::of('.')),
            band: Some(2),
            numerator: 3,
            denominator: 403,
            share_bp: 74,
            books: 1,
            usual: Usual::ExactNeighbor {
                neighbor: ScalarKey::of('"'),
                count: 380,
                reversed: 2,
            },
        },
        Pattern {
            glyph: ScalarKey::of('?'),
            channel: Channel::PooledNeighbor,
            key: PatternKey::PooledNeighbor(Pool::Quote),
            band: Some(2),
            numerator: 5,
            denominator: 403,
            share_bp: 124,
            books: 1,
            usual: Usual::None,
        },
        Pattern {
            glyph: ScalarKey::of(','),
            channel: Channel::RunShape,
            key: PatternKey::RunShape {
                pure: false,
                bucket: 4,
            },
            band: Some(2),
            numerator: 1,
            denominator: 601,
            share_bp: 16,
            books: 1,
            usual: Usual::RunShape {
                pure: true,
                bucket: 1,
                count: 598,
            },
        },
        Pattern {
            glyph: ScalarKey::DIGITS,
            channel: Channel::Placement,
            key: PatternKey::Placement {
                side: Side::Next,
                class: OuterClass::Letter,
            },
            band: Some(3),
            numerator: 12,
            denominator: 9_812,
            share_bp: 12,
            books: 1,
            usual: Usual::Placement {
                class: OuterClass::Space,
                count: 9_700,
            },
        },
        // A word hash rides bytes 0..8, so this row carries no glyph.
        Pattern {
            glyph: ScalarKey::NONE,
            channel: Channel::Casing,
            key: PatternKey::Casing {
                hash: 0x0123_4567_89ab_cdef,
                form: Form::Upper,
            },
            band: Some(1),
            numerator: 2,
            denominator: 40,
            share_bp: 500,
            books: 1,
            usual: Usual::Casing {
                form: Form::Lower,
                count: 38,
            },
        },
        // The other word channel: the same hash lanes, a sigma key byte.
        Pattern {
            glyph: ScalarKey::NONE,
            channel: Channel::WordLength,
            key: PatternKey::WordLength {
                hash: 0x0123_4567_89ab_cdef,
                sigma: 5,
            },
            band: Some(4),
            numerator: 7,
            denominator: 128_000,
            share_bp: 0,
            books: 1,
            usual: Usual::None,
        },
        // The third word channel: the same hash lanes, a key byte of 0 or 1.
        Pattern {
            glyph: ScalarKey::NONE,
            channel: Channel::Doubled,
            key: PatternKey::Doubled {
                hash: 0x0123_4567_89ab_cdef,
                separated: true,
            },
            band: Some(3),
            numerator: 1,
            denominator: 9_000,
            share_bp: 1,
            books: 1,
            usual: Usual::None,
        },
        // Not a word channel: the letter rides the glyph field and the key
        // byte is the run length.
        Pattern {
            glyph: ScalarKey::of('e'),
            channel: Channel::LetterRun,
            key: PatternKey::LetterRun { length: 3 },
            band: Some(3),
            numerator: 1,
            denominator: 4_000,
            share_bp: 2,
            books: 1,
            usual: Usual::None,
        },
        // The glyph is the whole key, so the key byte is zero like Rarity's —
        // and unlike Rarity's the row carries a band.
        Pattern {
            glyph: ScalarKey::of('.'),
            channel: Channel::SentenceStart,
            key: PatternKey::SentenceStart,
            band: Some(4),
            numerator: 6,
            denominator: 33_338,
            share_bp: 1,
            books: 1,
            usual: Usual::None,
        },
    ]
}

fn parse_hex(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

/// `encoded` against a shared hex fixture the JS reader also reads. Under
/// `UPDATE_HEX` it rewrites the file and fails: regenerating is never a
/// passing test, exactly as `UPDATE_GOLDENS` is.
fn assert_hex(encoded: &[u8], golden: &str, name: &str) {
    if std::env::var_os("UPDATE_HEX").is_some() {
        let hex: Vec<String> = encoded.iter().map(|byte| format!("{byte:02x}")).collect();
        let path = format!("{}/../testdata/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::write(path, format!("{}\n", hex.join(" "))).unwrap();
        panic!("UPDATE_HEX rewrote {name}; rerun without it to test it");
    }
    assert_eq!(encoded, parse_hex(golden), "{name}");
}

fn fixture_finding() -> PackedFinding {
    PackedFinding::new(
        0x10,
        0x12,
        BookIndex::new(0).unwrap(),
        FindingKind::LengthProportionality(ProportionalityDigest::new(
            Some(QuantizedDeviation::from_raw(0x0180).unwrap()),
            Some(QuantizedDeviation::from_raw(-0x0180).unwrap()),
            true,
        )),
        &[0x0200],
    )
    .unwrap()
}

#[test]
fn checked_in_reader_is_fresh() {
    let generated = generated_reader_ts();
    assert_eq!(generated, GENERATED);
    assert_eq!(generated, PACKAGED, "galley/sous-reader.ts");
}

#[test]
fn writer_matches_shared_golden_buffer_and_reader_view() {
    let finding = fixture_finding();
    let findings = [finding];
    let section = PublicationBook::new(BookKey::new(*b"MRK"), "books/mrk.usfm", 0x0200, &findings);
    let encoded = encode_to_corpus_buffer(
        SnapshotId::new(core::array::from_fn(|index| index as u8)),
        CoordinateSpace::Utf8,
        &[section],
        &[],
    )
    .unwrap();
    assert_hex(
        &encoded,
        include_str!("../../../testdata/corpus_v2.hex"),
        "corpus_v2.hex",
    );

    let snapshot = CorpusSnapshot::open(&encoded).unwrap();
    assert_eq!(
        snapshot.snapshot_id().as_bytes(),
        core::array::from_fn(|i| i as u8)
    );
    assert_eq!(snapshot.coordinate_space(), CoordinateSpace::Utf8);
    let book = snapshot.book_by_key(BookKey::new(*b"MRK")).unwrap();
    assert_eq!(book.index().get(), 0);
    assert_eq!(book.id(), "books/mrk.usfm");
    assert_eq!(book.published_len(), 0x0200);
    assert_eq!(book.at(0).unwrap(), finding);
    assert_eq!(
        snapshot.book_by_id("books/mrk.usfm").unwrap().index().get(),
        0
    );
    assert!(snapshot.book_by_id("books/gen.usfm").is_none());
}

/// Mixed kinds in one book: a proportionality row between an exact and a
/// saturated hygiene row, then one row of every remaining code.
#[test]
fn mixed_kind_golden_buffer_decodes_in_both_readers() {
    let hygiene = |from, to, class, run| {
        PackedFinding::new(
            from,
            to,
            BookIndex::new(0).unwrap(),
            FindingKind::Hygiene(HygieneDigest::new(class, run).unwrap()),
            &[0x0100],
        )
        .unwrap()
    };
    let findings = [
        hygiene(3, 6, HygieneClass::C0Control, 3),
        PackedFinding::new(
            0x10,
            0x12,
            BookIndex::new(0).unwrap(),
            FindingKind::LengthProportionality(ProportionalityDigest::new(
                Some(QuantizedDeviation::from_raw(0x0180).unwrap()),
                None,
                false,
            )),
            &[0x0100],
        )
        .unwrap(),
        hygiene(0x40, 0xa0, HygieneClass::Delete, 40_000),
        // The widened reasons lane: bit 9 rides the i16 the u8 half could not
        // hold, and it names the doubled row above.
        PackedFinding::new(
            0xb0,
            0xb8,
            BookIndex::new(0).unwrap(),
            FindingKind::Convention(ConventionDigest::new(
                PatternIndex::new(7),
                Reasons::DOUBLED_SEPARATED,
            )),
            &[0x0100],
        )
        .unwrap(),
        // Bit 10, naming the letter-run row: the word the run sits inside.
        PackedFinding::new(
            0xc0,
            0xc5,
            BookIndex::new(0).unwrap(),
            FindingKind::Convention(ConventionDigest::new(
                PatternIndex::new(8),
                Reasons::LETTER_RUN,
            )),
            &[0x0100],
        )
        .unwrap(),
        // Bit 11, naming the sentence-start row: the word after the glyph.
        PackedFinding::new(
            0xd0,
            0xd3,
            BookIndex::new(0).unwrap(),
            FindingKind::Convention(ConventionDigest::new(
                PatternIndex::new(9),
                Reasons::SENTENCE_START,
            )),
            &[0x0100],
        )
        .unwrap(),
        // A whole absent chapter: one row of thirty keys, zero-length at the
        // point they would be inserted.
        PackedFinding::new(
            0xe0,
            0xe0,
            BookIndex::new(0).unwrap(),
            FindingKind::Presence(PresenceDigest::new(PresenceKind::Missing, 30).unwrap()),
            &[0x0100],
        )
        .unwrap(),
        // Four consecutive words of a fourteen-word verse, all held by the
        // paired source verse.
        PackedFinding::new(
            0xf0,
            0xfc,
            BookIndex::new(0).unwrap(),
            FindingKind::SourceCopy(SourceCopyDigest::new(4, 14).unwrap()),
            &[0x0100],
        )
        .unwrap(),
    ];
    let section = PublicationBook::new(BookKey::new(*b"MRK"), "books/mrk.usfm", 0x0100, &findings);
    let encoded = encode_to_corpus_buffer(
        SnapshotId::new(core::array::from_fn(|index| index as u8)),
        CoordinateSpace::Utf8,
        &[section],
        &fixture_patterns(),
    )
    .unwrap();
    assert_hex(
        &encoded,
        include_str!("../../../testdata/corpus_v2_hygiene.hex"),
        "corpus_v2_hygiene.hex",
    );

    let snapshot = CorpusSnapshot::open(&encoded).unwrap();
    assert_eq!(snapshot.patterns().unwrap(), fixture_patterns());
    let book = snapshot.book_by_key(BookKey::new(*b"MRK")).unwrap();
    assert_eq!(book.at(0).unwrap(), findings[0]);
    assert_eq!(book.at(1).unwrap(), findings[1]);
    let FindingKind::Hygiene(digest) = book.at(2).unwrap().kind() else {
        panic!("hygiene kind")
    };
    assert_eq!(digest.class(), HygieneClass::Delete);
    assert!(digest.saturated());
    assert_eq!(digest.run(), 0x7fff);
    let FindingKind::Convention(digest) = book.at(3).unwrap().kind() else {
        panic!("convention kind")
    };
    assert_eq!(digest.pattern().get(), 7);
    assert_eq!(digest.reasons(), Reasons::DOUBLED_SEPARATED);
    let FindingKind::Convention(digest) = book.at(4).unwrap().kind() else {
        panic!("convention kind")
    };
    assert_eq!(digest.pattern().get(), 8);
    assert_eq!(digest.reasons(), Reasons::LETTER_RUN);
    let FindingKind::Convention(digest) = book.at(5).unwrap().kind() else {
        panic!("convention kind")
    };
    assert_eq!(digest.pattern().get(), 9);
    assert_eq!(digest.reasons(), Reasons::SENTENCE_START);
    let FindingKind::SourceCopy(digest) = book.at(7).unwrap().kind() else {
        panic!("source copy kind")
    };
    assert_eq!((digest.run(), digest.eligible()), (4, 14));
    assert!(!digest.saturated());
}

#[test]
fn header_is_48_bytes() {
    assert_eq!(HEADER_BYTES, 48);
    assert_eq!(HEADER_PATTERN_COUNT_OFFSET, 24);
    assert_eq!(HEADER_PATTERN_OFFSET_OFFSET, 28);
    assert_eq!(HEADER_SNAPSHOT_ID_OFFSET, 32);
    assert_eq!(PATTERN_ROW_LEN, 36);
    assert_eq!(PATTERN_BOOKS_OFFSET, 22);
    assert_eq!(PATTERN_RESERVED_OFFSET, 23);
    assert_eq!(PATTERN_USUAL_OFFSET, 24);
    assert_eq!(PATTERN_USUAL_COUNT_OFFSET, 28);
    assert_eq!(PATTERN_OTHER_COUNT_OFFSET, 32);
    let empty =
        encode_to_corpus_buffer(SnapshotId::new([0; 16]), CoordinateSpace::Utf8, &[], &[]).unwrap();
    assert_eq!(empty.len(), HEADER_BYTES);
    assert_eq!(read_u32(&empty, HEADER_PATTERN_COUNT_OFFSET), 0);
    assert_eq!(
        read_u32(&empty, HEADER_PATTERN_OFFSET_OFFSET),
        HEADER_BYTES as u32
    );
}

#[test]
fn pattern_table_round_trips() {
    let patterns = fixture_patterns();
    let books = [PublicationBook::new(
        BookKey::new(*b"MRK"),
        "books/mrk.usfm",
        0,
        &[],
    )];
    let encoded = encode_to_corpus_buffer(
        SnapshotId::new([7; 16]),
        CoordinateSpace::Utf8,
        &books,
        &patterns,
    )
    .unwrap();
    let snapshot = CorpusSnapshot::open(&encoded).unwrap();
    assert_eq!(snapshot.pattern_count(), 10);
    assert_eq!(snapshot.patterns().unwrap(), patterns);
    assert_eq!(
        snapshot.pattern(10),
        Err(CorpusWireError::PatternIndexPastTable {
            index: 10,
            count: 10,
            at: None
        })
    );

    // Every field the decoder refuses, one at a time.
    let start = HEADER_BYTES + DIRECTORY_ENTRY_BYTES + 16;
    for (offset, byte, field) in [
        (PATTERN_FLAGS_OFFSET, 1u8, "flags"),
        (PATTERN_RESERVED_OFFSET, 1, "reserved"),
        (PATTERN_CHANNEL_OFFSET, 10, "channel"),
        (PATTERN_BAND_OFFSET, 0, "band"),
        (PATTERN_KEY_OFFSET, 1, "key"),
        (PATTERN_BOOKS_OFFSET, 2, "books"),
        (PATTERN_BOOKS_OFFSET, 0, "books"),
    ] {
        let mut torn = encoded.clone();
        torn[start + offset] = byte;
        assert_eq!(
            CorpusSnapshot::open(&torn).err(),
            Some(CorpusWireError::InvalidPattern { row: 0, field }),
            "pattern {field} {byte} decoded"
        );
    }
    // Channel 1 is a live channel; its key byte is a `Pool` discriminant.
    const POOLED_ROW: usize = 2;
    let mut bad_pool = encoded.clone();
    bad_pool[start + POOLED_ROW * PATTERN_ROW_LEN + PATTERN_KEY_OFFSET] = Pool::ALL.len() as u8;
    assert_eq!(
        CorpusSnapshot::open(&bad_pool).err(),
        Some(CorpusWireError::InvalidPattern {
            row: POOLED_ROW,
            field: "key"
        })
    );

    let mut moved = encoded;
    moved[HEADER_PATTERN_OFFSET_OFFSET] = 0xff;
    assert!(matches!(
        CorpusSnapshot::open(&moved),
        Err(CorpusWireError::PatternSectionOutOfOrder { .. })
    ));
}

/// The one-book table `pattern_table_round_trips` publishes, and where its
/// pattern rows start.
fn fixture_table() -> (Vec<u8>, usize) {
    let books = [PublicationBook::new(
        BookKey::new(*b"MRK"),
        "books/mrk.usfm",
        0,
        &[],
    )];
    let encoded = encode_to_corpus_buffer(
        SnapshotId::new([7; 16]),
        CoordinateSpace::Utf8,
        &books,
        &fixture_patterns(),
    )
    .unwrap();
    (encoded, HEADER_BYTES + DIRECTORY_ENTRY_BYTES + 16)
}

/// Every channel's usual lanes come back as the variant they left as, and a
/// rarity whose pool holds nothing else says so with zeros.
#[test]
fn usual_lanes_round_trip_per_channel() {
    let (encoded, _) = fixture_table();
    let decoded = CorpusSnapshot::open(&encoded).unwrap().patterns().unwrap();
    let usual: Vec<Usual> = decoded.iter().map(|pattern| pattern.usual).collect();
    assert_eq!(
        usual,
        fixture_patterns()
            .iter()
            .map(|pattern| pattern.usual)
            .collect::<Vec<_>>()
    );
    assert!(matches!(usual[0], Usual::Rarity { glyph: Some(_), .. }));

    let mut alone = fixture_patterns();
    alone[0].usual = Usual::Rarity {
        glyph: None,
        count: 0,
    };
    let books = [PublicationBook::new(BookKey::new(*b"MRK"), "m", 0, &[])];
    let encoded = encode_to_corpus_buffer(
        SnapshotId::new([0; 16]),
        CoordinateSpace::Utf8,
        &books,
        &alone,
    )
    .unwrap();
    assert_eq!(
        CorpusSnapshot::open(&encoded)
            .unwrap()
            .pattern(0)
            .unwrap()
            .usual,
        alone[0].usual
    );
}

/// A value outside its channel's domain, or a lane the channel does not use
/// holding anything, is refused rather than read.
#[test]
fn usual_lanes_refuse_what_the_channel_cannot_mean() {
    const RARITY: usize = 0;
    const EXACT: usize = 1;
    const POOLED: usize = 2;
    const SHAPE: usize = 3;
    const PLACEMENT: usize = 4;
    const CASING: usize = 5;
    let (encoded, start) = fixture_table();
    let lane = |row: usize, offset: usize, value: u32| {
        let mut torn = encoded.clone();
        let at = start + row * PATTERN_ROW_LEN + offset;
        torn[at..at + 4].copy_from_slice(&value.to_le_bytes());
        CorpusSnapshot::open(&torn).err()
    };
    let refused = |row, field| Some(CorpusWireError::InvalidPattern { row, field });
    for (row, offset, value, field) in [
        // Unused lanes.
        (POOLED, PATTERN_USUAL_OFFSET, 1, "usual"),
        (POOLED, PATTERN_USUAL_COUNT_OFFSET, 1, "usual_count"),
        (POOLED, PATTERN_OTHER_COUNT_OFFSET, 1, "other_count"),
        (PLACEMENT, PATTERN_OTHER_COUNT_OFFSET, 1, "other_count"),
        (RARITY, PATTERN_OTHER_COUNT_OFFSET, 1, "other_count"),
        // Out of domain.
        (
            PLACEMENT,
            PATTERN_USUAL_OFFSET,
            OuterClass::Edge as u32,
            "usual",
        ),
        (PLACEMENT, PATTERN_USUAL_OFFSET, 5, "usual"),
        (PLACEMENT, PATTERN_USUAL_COUNT_OFFSET, 9_813, "usual"),
        (EXACT, PATTERN_USUAL_OFFSET, 0xd800, "usual"),
        (SHAPE, PATTERN_USUAL_OFFSET, 0x10, "usual"),
        (SHAPE, PATTERN_USUAL_OFFSET, 0x21, "usual"),
        (RARITY, PATTERN_USUAL_OFFSET, '`' as u32, "usual"),
        (RARITY, PATTERN_USUAL_OFFSET, '.' as u32, "usual"),
        (RARITY, PATTERN_USUAL_OFFSET, 0, "usual"),
        (CASING, PATTERN_USUAL_OFFSET, Form::Uncased as u32, "usual"),
    ] {
        assert_eq!(
            lane(row, offset, value),
            refused(row, field),
            "row {row} lane {offset} = {value:#x}"
        );
    }
}

/// `Pattern::validate` is symmetric: a share that disagrees with its own
/// fraction is refused whether it arrives via the encoder or is tampered
/// with on the wire after a valid encode.
#[test]
fn an_inconsistent_share_is_refused_on_the_way_in_and_out() {
    let mut patterns = fixture_patterns();
    patterns[1].share_bp += 1;
    let books = [PublicationBook::new(
        BookKey::new(*b"MRK"),
        "books/mrk.usfm",
        0,
        &[],
    )];
    assert_eq!(
        encode_to_corpus_buffer(
            SnapshotId::new([0; 16]),
            CoordinateSpace::Utf8,
            &books,
            &patterns,
        ),
        Err(CorpusWireError::InvalidPattern {
            row: 1,
            field: "share_bp"
        })
    );

    let valid = fixture_patterns();
    let encoded = encode_to_corpus_buffer(
        SnapshotId::new([0; 16]),
        CoordinateSpace::Utf8,
        &books,
        &valid,
    )
    .unwrap();
    let start = HEADER_BYTES + DIRECTORY_ENTRY_BYTES + 16 + PATTERN_ROW_LEN;
    let mut torn = encoded;
    torn[start + PATTERN_SHARE_OFFSET] ^= 0xff;
    assert_eq!(
        CorpusSnapshot::open(&torn).err(),
        Some(CorpusWireError::InvalidPattern {
            row: 1,
            field: "share_bp"
        })
    );
}

/// A `Convention` row names a table position, and the envelope is the
/// only place that knows how long the table is.
#[test]
fn pattern_index_past_count_is_refused() {
    let patterns = fixture_patterns();
    let past = patterns.len() as u16;
    let row = |pattern: u16| {
        PackedFinding::new(
            0,
            0,
            BookIndex::new(0).unwrap(),
            FindingKind::Convention(ConventionDigest::new(
                PatternIndex::new(pattern),
                Reasons::RARITY,
            )),
            &[0],
        )
        .unwrap()
    };
    let inside = [row(past - 1)];
    let books = [PublicationBook::new(
        BookKey::new(*b"MRK"),
        "books/mrk.usfm",
        0,
        &inside,
    )];
    let encoded = encode_to_corpus_buffer(
        SnapshotId::new([0; 16]),
        CoordinateSpace::Utf8,
        &books,
        &patterns,
    )
    .unwrap();
    assert_eq!(
        CorpusSnapshot::open(&encoded)
            .unwrap()
            .book(BookIndex::new(0).unwrap())
            .unwrap()
            .at(0)
            .unwrap(),
        inside[0]
    );

    let outside = [row(past)];
    let books = [PublicationBook::new(
        BookKey::new(*b"MRK"),
        "books/mrk.usfm",
        0,
        &outside,
    )];
    let encoded = encode_to_corpus_buffer(
        SnapshotId::new([0; 16]),
        CoordinateSpace::Utf8,
        &books,
        &patterns,
    )
    .unwrap();
    assert_eq!(
        CorpusSnapshot::open(&encoded).err(),
        Some(CorpusWireError::PatternIndexPastTable {
            index: usize::from(past),
            count: usize::from(past),
            at: Some((0, 0)),
        })
    );
}

#[test]
fn a_pattern_table_over_sixty_five_thousand_rows_is_refused() {
    let patterns = vec![fixture_patterns()[0]; usize::from(u16::MAX) + 1];
    assert_eq!(
        encode_to_corpus_buffer(
            SnapshotId::new([0; 16]),
            CoordinateSpace::Utf8,
            &[],
            &patterns
        ),
        Err(CorpusWireError::PatternCountOverflow { count: 65_536 })
    );
}

#[test]
fn empty_corpus_and_empty_books_are_valid() {
    let empty = encode_to_corpus_buffer(SnapshotId::new([0; 16]), CoordinateSpace::Utf16, &[], &[])
        .unwrap();
    assert_eq!(empty.len(), HEADER_BYTES);
    assert!(CorpusSnapshot::open(&empty).unwrap().is_empty());

    let books = [
        PublicationBook::new(BookKey::new(*b"GEN"), "a/gen.usfm", 0, &[]),
        PublicationBook::new(BookKey::new(*b"MRK"), "b/mrk.usfm", 0, &[]),
    ];
    let encoded = encode_to_corpus_buffer(
        SnapshotId::new([1; 16]),
        CoordinateSpace::Utf16,
        &books,
        &[],
    )
    .unwrap();
    let snapshot = CorpusSnapshot::open(&encoded).unwrap();
    assert_eq!(snapshot.len(), 2);
    assert_eq!(
        snapshot
            .book_by_key(BookKey::new(*b"MRK"))
            .unwrap()
            .index()
            .get(),
        1
    );
}

/// The string table is what makes two files of one `\id` addressable.
#[test]
fn duplicate_book_keys_are_legal_and_the_ids_tell_them_apart() {
    let books = [
        PublicationBook::new(BookKey::new(*b"GEN"), "a/gen-copy.usfm", 4, &[]),
        PublicationBook::new(BookKey::new(*b"GEN"), "a/gen.usfm", 8, &[]),
    ];
    let encoded =
        encode_to_corpus_buffer(SnapshotId::new([2; 16]), CoordinateSpace::Utf8, &books, &[])
            .unwrap();
    let snapshot = CorpusSnapshot::open(&encoded).unwrap();

    assert_eq!(snapshot.len(), 2);
    assert_eq!(
        snapshot.book_by_key(BookKey::new(*b"GEN")).unwrap().id(),
        "a/gen-copy.usfm",
        "a key seeks the first of its rows"
    );
    assert_eq!(snapshot.book_by_id("a/gen.usfm").unwrap().index().get(), 1);
    assert_eq!(
        snapshot.book_by_id("a/gen.usfm").unwrap().published_len(),
        8
    );
}

#[test]
fn a_repeated_id_is_refused_on_the_way_in_and_out() {
    let books = [
        PublicationBook::new(BookKey::new(*b"GEN"), "same.usfm", 0, &[]),
        PublicationBook::new(BookKey::new(*b"MRK"), "same.usfm", 0, &[]),
    ];
    assert_eq!(
        encode_to_corpus_buffer(SnapshotId::new([0; 16]), CoordinateSpace::Utf8, &books, &[]),
        Err(CorpusWireError::DuplicateBookId { book: 1 })
    );
}

#[test]
fn a_moved_or_malformed_id_offset_fails_closed() {
    let books = [PublicationBook::new(
        BookKey::new(*b"MRK"),
        "books/mrk.usfm",
        0,
        &[],
    )];
    let encoded =
        encode_to_corpus_buffer(SnapshotId::new([0; 16]), CoordinateSpace::Utf8, &books, &[])
            .unwrap();

    let mut moved = encoded.clone();
    moved[HEADER_BYTES + DIRECTORY_ID_OFFSET] = 0xff;
    assert!(matches!(
        CorpusSnapshot::open(&moved),
        Err(CorpusWireError::BookIdOutOfOrder { book: 0, .. })
    ));

    let mut torn = encoded;
    // The id's second UTF-8 byte, replaced by a continuation byte.
    torn[HEADER_BYTES + DIRECTORY_ENTRY_BYTES + ID_PREFIX_BYTES] = 0x80;
    assert_eq!(
        CorpusSnapshot::open(&torn).err(),
        Some(CorpusWireError::InvalidBookId { book: 0 })
    );
}

#[test]
fn malformed_directory_and_record_fail_closed() {
    let finding = fixture_finding();
    let findings = [finding];
    let section = PublicationBook::new(BookKey::new(*b"MRK"), "books/mrk.usfm", 0x0200, &findings);
    let encoded = encode_to_corpus_buffer(
        SnapshotId::new([0; 16]),
        CoordinateSpace::Utf8,
        &[section],
        &[],
    )
    .unwrap();

    let mut bad_key = encoded.clone();
    bad_key[HEADER_BYTES] = 0xff;
    assert!(matches!(
        CorpusSnapshot::open(&bad_key),
        Err(CorpusWireError::InvalidBookKey { .. })
    ));

    let mut bad_code = encoded;
    bad_code[FIRST_RECORD + 10] = 5;
    assert!(matches!(
        CorpusSnapshot::open(&bad_code),
        Err(CorpusWireError::Record {
            error: CodecError::UnknownRuleCode(5),
            ..
        })
    ));
}
