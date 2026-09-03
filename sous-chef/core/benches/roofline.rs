//! Hardware ceilings the hygiene scan is measured against, single core.
//!
//!     cargo bench -p sous-core                     # all corpora, all passes
//!     cargo bench -p sous-core -- hygiene          # one pass
//!
//!     roofline            fastest │ median │ bytes/s
//!     ├─ scalar   ...     x ms    │        │  ~1 GiB/s   dependent byte chain
//!     ├─ vectorized ...   x ms    │        │ ~30 GiB/s   autovectorized compare
//!     ├─ memchr3 ...      x ms    │        │ ~10 GiB/s   three fixed needles
//!     ╰─ hygiene ...      x ms    │        │             the real pass
//!
//! Every pass below is an intentional subtraction from the ceiling above it,
//! not a profile-guided guess. Numbers are recorded in evidence.md;
//! nothing here asserts on them.
//!
//! Byte source: the committed test-tier corpora only. A missing file is a
//! loud failure, never a silent skip.
//!
//! Allocation counts are off by default:
//!
//!     cargo bench -p sous-core --features alloc-counts -- substrate
//!
//! Observation bytes per chapter are NOT reported here — mapping the whole
//! tier to print one table would tax every other row in the file. They come
//! from `examples/observation_size.rs` and from the ignored size oracle in
//! `tests/substrate_reference.rs`.

use std::sync::LazyLock;

use divan::{
    Bencher,
    counter::{BytesCount, ItemsCount},
};
use sous_core::substrate::{ChapterRow, Edge, Substrate, fold_book};
use sous_core::unicode::lookup::{walk, walk_trie, walk_trie_swar};
use sous_core::{BookKey, ChapterInput, ChapterKey, ChapterObs, ChapterPass};

/// See the note in `onion/benches/pipeline.rs`: measured overhead is under
/// the noise floor, so this is safe to leave on when the counts are wanted.
#[cfg(feature = "alloc-counts")]
#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpora/");
const FILES: [&str; 8] = [
    "WA-en-ulb.txt",
    "amh.txt",
    "francl.txt",
    "grcsr.txt",
    "hin2017.txt",
    "nya.txt",
    "spaRV1909.txt",
    "swhulb.txt",
];

static CORPORA: LazyLock<Vec<(&'static str, String)>> = LazyLock::new(|| {
    FILES
        .iter()
        .map(|name| {
            let path = format!("{ROOT}{name}");
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("test-tier corpus {path} must be present: {error}"));
            (*name, text)
        })
        .collect()
});

fn corpus(name: &str) -> &'static str {
    &CORPORA
        .iter()
        .find(|(file, _)| *file == name)
        .expect("bench arg names a listed corpus")
        .1
}

fn main() {
    divan::main()
}

/// A dependent multiply chain: one byte per step, no SIMD possible.
#[divan::bench(args = FILES)]
fn scalar(bencher: Bencher, name: &str) {
    let bytes = corpus(name).as_bytes();
    bencher.counter(BytesCount::of_slice(bytes)).bench(|| {
        bytes.iter().fold(0u64, |acc, &b| {
            acc.wrapping_mul(0x100_0000_01b3).wrapping_add(u64::from(b))
        })
    });
}

/// An autovectorized compare-and-count: the memory/SIMD ceiling.
#[divan::bench(args = FILES)]
fn vectorized(bencher: Bencher, name: &str) {
    let bytes = corpus(name).as_bytes();
    bencher.counter(BytesCount::of_slice(bytes)).bench(|| {
        bytes
            .iter()
            .fold(0u32, |acc, &b| acc + u32::from(b < 0x20 || b == 0x7f))
    });
}

/// The three lead bytes hygiene's needle filter looks for.
#[divan::bench(args = FILES)]
fn memchr3(bencher: Bencher, name: &str) {
    let bytes = corpus(name).as_bytes();
    bencher
        .counter(BytesCount::of_slice(bytes))
        .bench(|| memchr::memchr3_iter(0x5c, 0xc2, 0xef, bytes).count());
}

/// The real pass: both fast filters plus marker search over clean text.
#[divan::bench(args = FILES)]
fn hygiene(bencher: Bencher, name: &str) {
    let text = corpus(name);
    bencher
        .counter(BytesCount::of_str(text))
        .bench(|| sous_core::hygiene::scan(text).len());
}

// ── The classifier walk (Stage 1) ───────────────────────────────────────
//
// One `Class` per scalar over the whole corpus. Read against the ceilings
// above: the walk is a dependent load chain, so the scalar row is its real
// neighbour, not `memchr3`. Rejected candidates live in
// `sous-chef/experiments/`.

fn classified(bencher: Bencher, name: &str, walk: impl Fn(&str) -> u64 + Sync) {
    let text = corpus(name);
    bencher
        .counter(BytesCount::of_str(text))
        .counter(ItemsCount::new(text.chars().count()))
        .bench(|| walk(text));
}

/// The plain per-scalar walk: decode, then one two-level lookup.
#[divan::bench(args = FILES)]
fn class_two_level(bencher: Bencher, name: &str) {
    classified(bencher, name, walk);
}

/// The byte trie walking raw UTF-8, no scalar decode.
#[divan::bench(args = FILES)]
fn lane_trie_bytes(bencher: Bencher, name: &str) {
    classified(bencher, name, walk_trie);
}

/// The shipped lane: an eight-byte SWAR ASCII chunk over the byte trie.
#[divan::bench(args = FILES)]
fn lane_trie_swar(bencher: Bencher, name: &str) {
    classified(bencher, name, walk_trie_swar);
}

// ── The Level 1b substrate (Stage 3) ────────────────────────────────────
//
// The classifier walk above is this walk's floor: `substrate_map` adds an
// inventory count, a pair triple, a run atom, and a word transition to every
// scalar the trie already classified. `substrate_reduce` is the other half of
// a publication — the sorted merges and the seam fixes over one whole book.

/// One corpus as books of chapter strings, grouped from its vref lines the
/// way `examples/observation_size.rs` groups them.
type Books = Vec<Vec<String>>;
/// The same shape, already mapped: one row per chapter, books in order.
type Mapped = Vec<Vec<ChapterRow>>;

static CHAPTERS: LazyLock<Vec<(&'static str, Books)>> = LazyLock::new(|| {
    CORPORA
        .iter()
        .map(|(name, text)| (*name, group(text)))
        .collect()
});

static ROWS: LazyLock<Vec<(&'static str, Mapped)>> = LazyLock::new(|| {
    CHAPTERS
        .iter()
        .map(|(name, books)| {
            let mapped = books
                .iter()
                .map(|book| book.iter().map(|text| map(text)).collect())
                .collect();
            (*name, mapped)
        })
        .collect()
});

fn group(raw: &str) -> Books {
    let mut books: Vec<Vec<String>> = Vec::new();
    let mut seen: Option<(&str, u32)> = None;
    for line in raw.lines() {
        let Some((refpart, text)) = line.split_once('\t') else {
            continue;
        };
        let mut parts = refpart.split_whitespace();
        let (Some(book), Some(cv)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Some(chapter) = cv.split_once(':').and_then(|(c, _)| c.parse::<u32>().ok()) else {
            continue;
        };
        match seen {
            Some((held, _)) if held == book => {}
            _ => books.push(Vec::new()),
        }
        let chapters = books.last_mut().expect("just pushed");
        if seen != Some((book, chapter)) {
            chapters.push(String::new());
        }
        let held = chapters.last_mut().expect("just pushed");
        if !held.is_empty() {
            held.push(' ');
        }
        held.push_str(text);
        seen = Some((book, chapter));
    }
    assert!(!books.is_empty(), "a tier corpus has books");
    books
}

fn map(text: &str) -> ChapterRow {
    Substrate.map(ChapterInput {
        text,
        verses: &[],
        key: ChapterKey::new(BookKey::new(*b"MRK"), 1),
    })
}

fn books(name: &str) -> &'static Books {
    &CHAPTERS
        .iter()
        .find(|(file, _)| *file == name)
        .expect("bench arg names a listed corpus")
        .1
}

fn rows(name: &str) -> &'static Mapped {
    &ROWS
        .iter()
        .find(|(file, _)| *file == name)
        .expect("bench arg names a listed corpus")
        .1
}

/// One walk per chapter over the whole corpus; ns/scalar reads against
/// `lane_trie_swar`, which is the same walk with the counting removed.
#[divan::bench(args = FILES)]
fn substrate_map(bencher: Bencher, name: &str) {
    let books = books(name);
    let bytes: usize = books.iter().flatten().map(|text| text.len()).sum();
    let scalars: usize = books
        .iter()
        .flatten()
        .map(|text| text.chars().count())
        .sum();
    bencher
        .counter(BytesCount::new(bytes))
        .counter(ItemsCount::new(scalars))
        .bench(|| {
            let mut total = 0u64;
            for book in books {
                for text in book {
                    total += u64::from(map(text).scalar_count());
                }
            }
            total
        });
}

/// The book fold: sorted merges of every lane, then one seam fix per chapter.
/// Counted per chapter, since a reduce reads rows and never the text.
#[divan::bench(args = FILES)]
fn substrate_reduce(bencher: Bencher, name: &str) {
    let rows = rows(name);
    let chapters: usize = rows.iter().map(Vec::len).sum();
    bencher.counter(ItemsCount::new(chapters)).bench(|| {
        let mut total = 0u64;
        for book in rows {
            let view: Vec<ChapterObs<&ChapterRow>> = book
                .iter()
                .map(|obs| ChapterObs { start: 0, obs })
                .collect();
            total += fold_book(&view, &mut Edge::default()).scalar_count();
        }
        total
    });
}
