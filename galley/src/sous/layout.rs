//! The findings snapshot's layout, for `docs/layouts/sous.json`.
//!
//! The buffer is sous-core's and packed by hand there; galley publishes it,
//! so the docs declaration lives beside the seam and reads sous-core's own
//! offsets. Nothing here is a second source for a number.

use sous_core::layout::*;
use sous_core::{
    RECORD_BOOK_INDEX_OFFSET, RECORD_BOOK_SCOPE_OFFSET, RECORD_CODE_OFFSET, RECORD_FLAGS_OFFSET,
    RECORD_FROM_OFFSET, RECORD_LEN, RECORD_PROJECT_SCOPE_OFFSET, RECORD_TO_OFFSET,
};
use ticket::schema::{Layout, Region, Sample, Shape, Slot};

const fn slot(name: &'static str, at: usize, end: usize, doc: &'static str) -> Slot {
    Slot {
        name,
        at,
        bytes: end - at,
        doc,
    }
}

pub const HEADER: &[Slot] = &[
    slot(
        "magic",
        HEADER_MAGIC_OFFSET,
        HEADER_VERSION_OFFSET,
        "`SOUS`, little-endian.",
    ),
    slot(
        "version",
        HEADER_VERSION_OFFSET,
        HEADER_FLAGS_OFFSET,
        "FORMAT_VERSION. A reader that does not know it stops.",
    ),
    slot(
        "flags",
        HEADER_FLAGS_OFFSET,
        HEADER_BOOK_COUNT_OFFSET,
        "Bit 0: every span is raw-book UTF-16 rather than bytes.",
    ),
    slot(
        "bookCount",
        HEADER_BOOK_COUNT_OFFSET,
        HEADER_RECORD_LEN_OFFSET,
        "Directory rows, in caller order.",
    ),
    slot(
        "recordLen",
        HEADER_RECORD_LEN_OFFSET,
        HEADER_TOTAL_FINDINGS_OFFSET,
        "The record stride, 16.",
    ),
    slot(
        "totalFindings",
        HEADER_TOTAL_FINDINGS_OFFSET,
        HEADER_PATTERN_COUNT_OFFSET,
        "Records across every book's section.",
    ),
    slot(
        "patternCount",
        HEADER_PATTERN_COUNT_OFFSET,
        HEADER_PATTERN_OFFSET_OFFSET,
        "Rows in the pattern table.",
    ),
    slot(
        "patternOffset",
        HEADER_PATTERN_OFFSET_OFFSET,
        HEADER_SNAPSHOT_ID_OFFSET,
        "Where the pattern table starts, absolute.",
    ),
    slot(
        "snapshotId",
        HEADER_SNAPSHOT_ID_OFFSET,
        HEADER_CLUSTER_COUNT_OFFSET,
        "Opaque identity of the snapshot; a finding handle is this plus a row.",
    ),
    slot(
        "clusterCount",
        HEADER_CLUSTER_COUNT_OFFSET,
        HEADER_CLUSTER_OFFSET_OFFSET,
        "Entries in the cluster section.",
    ),
    slot(
        "clusterOffset",
        HEADER_CLUSTER_OFFSET_OFFSET,
        HEADER_TERMINAL_COUNT_OFFSET,
        "Where it starts: the running cursor after the pattern table.",
    ),
    slot(
        "terminalCount",
        HEADER_TERMINAL_COUNT_OFFSET,
        HEADER_TERMINAL_OFFSET_OFFSET,
        "Entries in the terminal section.",
    ),
    slot(
        "terminalOffset",
        HEADER_TERMINAL_OFFSET_OFFSET,
        HEADER_BYTES,
        "Where it starts: the running cursor after the cluster section.",
    ),
];

pub const DIRECTORY_ENTRY: &[Slot] = &[
    slot(
        "key",
        DIRECTORY_KEY_OFFSET,
        DIRECTORY_KEY_TERMINATOR_OFFSET,
        "The book code, ASCII.",
    ),
    slot(
        "terminator",
        DIRECTORY_KEY_TERMINATOR_OFFSET,
        DIRECTORY_LENGTH_OFFSET,
        "NUL.",
    ),
    slot(
        "publishedLength",
        DIRECTORY_LENGTH_OFFSET,
        DIRECTORY_SECTION_OFFSET,
        "The book's length in the header's coordinate space; no span passes it.",
    ),
    slot(
        "sectionOffset",
        DIRECTORY_SECTION_OFFSET,
        DIRECTORY_FINDING_COUNT_OFFSET,
        "Where this book's records start, absolute.",
    ),
    slot(
        "findingCount",
        DIRECTORY_FINDING_COUNT_OFFSET,
        DIRECTORY_ID_OFFSET,
        "How many.",
    ),
    slot(
        "idOffset",
        DIRECTORY_ID_OFFSET,
        DIRECTORY_ENTRY_BYTES,
        "Where the caller's id for this book starts in the id strings.",
    ),
];

pub const PATTERN_ROW: &[Slot] = &[
    slot(
        "glyph",
        PATTERN_GLYPH_OFFSET,
        PATTERN_NEIGHBOR_OFFSET,
        "The scalar; u32::MAX is the pooled digit key.",
    ),
    slot(
        "neighbor",
        PATTERN_NEIGHBOR_OFFSET,
        PATTERN_CHANNEL_OFFSET,
        "The neighbouring scalar on ExactNeighbor; 0 on every other channel.",
    ),
    slot(
        "channel",
        PATTERN_CHANNEL_OFFSET,
        PATTERN_KEY_OFFSET,
        "The Channel discriminant, finest grain first.",
    ),
    slot(
        "key",
        PATTERN_KEY_OFFSET,
        PATTERN_BAND_OFFSET,
        "A byte each channel gives its own meaning.",
    ),
    slot(
        "band",
        PATTERN_BAND_OFFSET,
        PATTERN_FLAGS_OFFSET,
        "Staircase step; 0xFF is none.",
    ),
    slot(
        "flags",
        PATTERN_FLAGS_OFFSET,
        PATTERN_NUMERATOR_OFFSET,
        "Bit 0 LOOKALIKE, on a Rarity row only.",
    ),
    slot(
        "numerator",
        PATTERN_NUMERATOR_OFFSET,
        PATTERN_DENOMINATOR_OFFSET,
        "How often the pattern occurred.",
    ),
    slot(
        "denominator",
        PATTERN_DENOMINATOR_OFFSET,
        PATTERN_SHARE_OFFSET,
        "Out of how many chances.",
    ),
    slot(
        "shareBp",
        PATTERN_SHARE_OFFSET,
        PATTERN_BOOKS_OFFSET,
        "That share in basis points, at most 10,000.",
    ),
    slot(
        "books",
        PATTERN_BOOKS_OFFSET,
        PATTERN_FACING_OFFSET,
        "Books holding part of the numerator; books-possible is the header's bookCount.",
    ),
    slot(
        "facing",
        PATTERN_FACING_OFFSET,
        PATTERN_USUAL_OFFSET,
        "ExactNeighbor only: the pair's Facing byte, 0 when it holds no directionless quote.",
    ),
    slot(
        "usual",
        PATTERN_USUAL_OFFSET,
        PATTERN_USUAL_COUNT_OFFSET,
        "What is usual instead; each channel owns the meaning.",
    ),
    slot(
        "usualCount",
        PATTERN_USUAL_COUNT_OFFSET,
        PATTERN_OTHER_COUNT_OFFSET,
        "How often the usual occurred.",
    ),
    slot(
        "otherCount",
        PATTERN_OTHER_COUNT_OFFSET,
        PATTERN_ROW_LEN,
        "The third lane, per channel.",
    ),
];

pub const CLUSTER_ENTRY: &[Slot] = &[
    slot(
        "pattern",
        CLUSTER_PATTERN_OFFSET,
        CLUSTER_ATOM_COUNT_OFFSET,
        "The RunShape row this run is of.",
    ),
    slot(
        "atomCount",
        CLUSTER_ATOM_COUNT_OFFSET,
        CLUSTER_FLAGS_OFFSET,
        "Atoms stored after the entry, 1..=16, a u32 scalar each.",
    ),
    slot(
        "flags",
        CLUSTER_FLAGS_OFFSET,
        CLUSTER_COUNT_OFFSET,
        "Bit 0 RECURRING, bit 1 TRUNCATED, the high nibble the run's Facing.",
    ),
    slot(
        "count",
        CLUSTER_COUNT_OFFSET,
        CLUSTER_ENTRY_BYTES,
        "Corpus occurrences of exactly this run.",
    ),
];

pub const TERMINAL_ENTRY: &[Slot] = &[
    slot(
        "glyph",
        TERMINAL_GLYPH_OFFSET,
        TERMINAL_CONTEXT_OFFSET,
        "The mark, a scalar.",
    ),
    slot(
        "context",
        TERMINAL_CONTEXT_OFFSET,
        TERMINAL_CONTEXT_OFFSET + 1,
        "Bit 0 QUOTED, bit 1 BRACKETED: what stood between mark and word.",
    ),
    slot(
        "pad",
        TERMINAL_CONTEXT_OFFSET + 1,
        TERMINAL_UPPER_OFFSET,
        "Zero.",
    ),
    slot(
        "upper",
        TERMINAL_UPPER_OFFSET,
        TERMINAL_CASED_OFFSET,
        "Handoffs to an uppercase letter.",
    ),
    slot(
        "cased",
        TERMINAL_CASED_OFFSET,
        TERMINAL_ENTRY_BYTES,
        "Handoffs to a cased letter; never 0, at least upper.",
    ),
];

pub const RECORD: &[Slot] = &[
    slot(
        "from",
        RECORD_FROM_OFFSET,
        RECORD_TO_OFFSET,
        "Start, in the coordinate space the header declares.",
    ),
    slot(
        "to",
        RECORD_TO_OFFSET,
        RECORD_BOOK_INDEX_OFFSET,
        "End, exclusive.",
    ),
    slot(
        "bookIndex",
        RECORD_BOOK_INDEX_OFFSET,
        RECORD_CODE_OFFSET,
        "Into the directory; always this section's own book.",
    ),
    slot(
        "code",
        RECORD_CODE_OFFSET,
        RECORD_FLAGS_OFFSET,
        "The rule code: the union tag the two lanes are read by.",
    ),
    slot(
        "flags",
        RECORD_FLAGS_OFFSET,
        RECORD_BOOK_SCOPE_OFFSET,
        "Only SATURATED: a lane was clamped.",
    ),
    slot(
        "laneA",
        RECORD_BOOK_SCOPE_OFFSET,
        RECORD_PROJECT_SCOPE_OFFSET,
        "An i16 whose meaning the rule code owns.",
    ),
    slot(
        "laneB",
        RECORD_PROJECT_SCOPE_OFFSET,
        RECORD_LEN,
        "A second i16 the rule code owns.",
    ),
];

/// The whole snapshot, region by region.
pub fn layout() -> Layout {
    let table = |name, at: &str, entry_bytes, count: &str, slots, doc| Region {
        name,
        at: at.into(),
        shape: Shape::Table {
            entry_bytes,
            count: count.into(),
            slots,
        },
        doc,
    };
    Layout {
        format: "sous",
        magic: MAGIC,
        version: FORMAT_VERSION,
        declared_in: "sous-chef/core/src/corpus/layout.rs",
        reader: "galley/sous-reader.ts",
        generated: false,
        doc: "One complete corpus publication: every finding, the patterns that argue for them, and what the corpus does instead.",
        envelope: vec![
            Region {
                name: "header",
                at: "0".into(),
                shape: Shape::Fixed {
                    bytes: HEADER_BYTES,
                    slots: HEADER,
                },
                doc: "Counts and absolute offsets for every section behind it.",
            },
            table(
                "directory",
                &format!("{HEADER_BYTES}"),
                DIRECTORY_ENTRY_BYTES,
                "header.bookCount",
                DIRECTORY_ENTRY,
                "One row per book, in caller order.",
            ),
            Region {
                name: "ids",
                at: "after the directory".into(),
                shape: Shape::Blob {
                    bytes: "per book: a u16 length, then that many UTF-8 bytes; padded to 4".into(),
                    holds: None,
                },
                doc: "The caller's id for each book, in directory order.",
            },
            table(
                "patterns",
                "header.patternOffset",
                PATTERN_ROW_LEN,
                "header.patternCount",
                PATTERN_ROW,
                "The judge's output, one row per firing pattern; a Convention record names one by index.",
            ),
            table(
                "clusters",
                "header.clusterOffset",
                CLUSTER_ENTRY_BYTES,
                "header.clusterCount",
                CLUSTER_ENTRY,
                "The exact runs each RunShape row lists, each entry followed by atomCount u32 scalars.",
            ),
            table(
                "terminals",
                "header.terminalOffset",
                TERMINAL_ENTRY_BYTES,
                "header.terminalCount",
                TERMINAL_ENTRY,
                "What the corpus hands off to after each mark, one entry per mark and context.",
            ),
            table(
                "records",
                "directory[book].sectionOffset",
                RECORD_LEN,
                "directory[book].findingCount",
                RECORD,
                "Fixed-width findings, one section per book, no padding.",
            ),
        ],
        records: &[],
        sample: Some(Sample {
            usfm: "samples/sample.usfm",
            bin: "samples/sous.bin",
            options: &[("ids", "[\"sample.usfm\"]"), ("role", "\"target\"")],
        }),
    }
}
