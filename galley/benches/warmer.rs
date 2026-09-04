//! What one keystroke costs an editor, folded vs fresh.
//!
//!     cargo bench -p usfm_galley
//!     cargo bench -p usfm_galley -- folded          # the cache path alone
//!     # profiling: see the recipe in onion/benches/pipeline.rs
//!
//! The keystroke: one character typed into the middle chapter's first text
//! token. Every call presents a NEVER-SEEN text, so the edited chunk is a
//! miss and every other chunk hits — the shape an editor actually produces.
//! An all-hit call (the same text twice) is not benched here; it measures the
//! pre-scan + checksum floor, not the fold.
//!
//! Per book, fresh against folded:
//!
//! - `analyze_fresh` — `onion::analyze` with every want. What the editor pays
//!   per keystroke TODAY: [`Warmer`] folds lint, not analyze, so this one
//!   is un-foldable as things stand.
//! - `lint_fresh` — the un-folded `lint(lex, build)` pipeline. The fold's
//!   baseline.
//! - `lint_folded` — the same answer with one dirty chunk.
//! - `analyze_folded` — every read, with the lex, the CST build and the lint
//!   walk served from cache for every clean chunk. `analyze_fresh` is what it
//!   replaces.
//! - `verse_text_folded` — the mask a downstream text consumer reads, off the
//!   same assembled ingredients.
//!
//! `bytes/s` is over the WHOLE book on purpose: per keystroke the fold still
//! pre-scans and checksums every byte, so the whole-book rate is the floor
//! the fold cannot go under however few chunks are dirty.

use std::sync::LazyLock;

use divan::counter::BytesCount;
use usfm_galley::{Warmer, onion};

/// See the note in `onion/benches/pipeline.rs`: measured overhead is under the
/// noise floor, so this is safe to leave on when the counts are wanted.
#[cfg(feature = "alloc-counts")]
#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main()
}

/// A size ladder across four decades: the biggest book in the corpus, the
/// same book in a lighter translation, a gospel, and a one-chapter epistle.
const BOOKS: &[&str] = &[
    "stressCorpora/en_ult/19-PSA.usfm",
    "exampleCorpora/en_ulb/19-PSA.usfm",
    "exampleCorpora/en_ulb/41-MAT.usfm",
    "exampleCorpora/en_ulb/66-JUD.usfm",
];

/// One keystroke per entry of `keystrokes`, each landing in the same chapter.
struct Book {
    text: String,
    /// Chunks in the unedited book. At or below galley's gate the cache is
    /// bypassed entirely, so the folded rows measure the fresh path — kept in
    /// the ladder on purpose, as the floor where folding has nothing to reuse.
    chunks: usize,
    /// Successive states of typing into one chapter: `keystrokes[i]` has `i+1`
    /// characters inserted. Long enough that a bench run never wraps and
    /// re-presents a text the cache has already seen.
    keystrokes: Vec<String>,
}

/// One measured call per sample, each a never-seen text — so the ring must
/// outlast the sample count. Derived from it, never written twice.
const SAMPLES: u32 = 50;
const RING: usize = SAMPLES as usize + 8;

static CORPUS: LazyLock<Vec<(&'static str, Book)>> =
    LazyLock::new(|| BOOKS.iter().map(|name| (*name, load(name))).collect());

fn load(name: &str) -> Book {
    let path = format!("{}/../testData/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));

    // A middle chapter, never the self-scanned chunk 0 — chunk 0's carry-outs
    // key every later chunk, so editing it dirties the whole book and measures
    // a different thing. A one-chapter epistle has exactly one candidate.
    let starts = onion::chunk::pre_scan(text.as_bytes()).starts;
    assert!(starts.len() >= 2, "{name} has no chapter chunk to edit");
    let chunk = (starts.len() / 2).max(1);
    let from = starts[chunk];
    let to = starts.get(chunk + 1).copied().unwrap_or(text.len() as u32);

    // The first text token inside it — a keystroke lands in prose, not in a
    // marker, so exactly one chunk's content hash moves.
    let at = onion::lex(&text)
        .iter()
        .find(|t| {
            t.kind() == onion::TokenKind::Text && t.start >= from && t.start < to && t.len > 0
        })
        .map(|t| t.start as usize)
        .unwrap_or_else(|| panic!("{name}: no text token in chapter chunk {chunk}"));

    let keystrokes = (1..=RING)
        .map(|n| {
            let mut typed = text.clone();
            typed.insert_str(at, &"x".repeat(n));
            typed
        })
        .collect();

    Book {
        chunks: starts.len(),
        text,
        keystrokes,
    }
}

fn book(name: &str) -> &'static Book {
    &CORPUS
        .iter()
        .find(|(n, _)| *n == name)
        .expect("a listed book")
        .1
}

/// A cache warm on the unedited book, plus the proof that one keystroke is
/// exactly one miss. Asserted here rather than inside the timed closure.
fn warmed(book: &Book, name: &str) -> Warmer {
    // A budget, not an allocation — `new` builds an empty map and only compares
    // this against `resident_bytes()` when deciding to evict. Sized so nothing
    // evicts during a run: MEASURED worst case is en_ult PSA at 3.27 MB warm and
    // 4.62 MB after 50 keystrokes, so 16 MB is 3.5x headroom.
    let mut cache = Warmer::new(24 << 20);
    cache.lint(&book.text);
    let before = cache.misses();
    cache.lint(&book.keystrokes[0]);
    let dirty = cache.misses() - before;
    if cache.is_empty() {
        // Below galley's chunk gate: nothing is cached, so every call recomputes
        // every chunk. The invariant here is that the gate held, not that one
        // chunk went dirty.
        assert_eq!(
            dirty, book.chunks as u64,
            "{name}: a gated book recomputes every chunk"
        );
    } else {
        assert_eq!(
            dirty, 1,
            "{name}: one keystroke should dirty one chunk, dirtied {dirty}"
        );
    }
    cache
}

#[divan::bench(args = BOOKS, sample_count = SAMPLES, sample_size = 1)]
fn lint_folded(bencher: divan::Bencher, name: &str) {
    let book = book(name);
    let mut cache = warmed(book, name);
    // keystrokes[0] went into the cache during the warm-up assert; start past it.
    let mut next = 1;
    bencher
        .counter(BytesCount::new(book.text.len()))
        .bench_local(|| {
            let typed = &book.keystrokes[next];
            next += 1;
            divan::black_box(cache.lint(divan::black_box(typed)))
        });
}

#[divan::bench(args = BOOKS, sample_count = 20)]
fn lint_fresh(bencher: divan::Bencher, name: &str) {
    let book = book(name);
    let typed = &book.keystrokes[0];
    bencher.counter(BytesCount::new(book.text.len())).bench(|| {
        let tokens = onion::lex(typed);
        let cst = onion::cst::build(&tokens);
        divan::black_box(onion::lint::lint(typed.as_bytes(), &tokens, &cst))
    });
}

/// The whole dish per keystroke, folded — the number an editor actually pays.
#[divan::bench(args = BOOKS, sample_count = SAMPLES, sample_size = 1)]
fn parse_folded(bencher: divan::Bencher, name: &str) {
    let book = book(name);
    let mut cache = warmed(book, name);
    let mut next = 1;
    bencher
        .counter(BytesCount::new(book.text.len()))
        .bench_local(|| {
            let typed = &book.keystrokes[next];
            next += 1;
            divan::black_box(cache.parse(divan::black_box(typed), EDITOR))
        });
}

/// What an editor asks for: everything, in the addressing CodeMirror counts in.
const EDITOR: onion::wire::ParseOptions = onion::wire::ParseOptions {
    diagnostics: true,
    toc: true,
    utf16: true,
};

/// The verse-text mask off the cached ingredients, against building it fresh.
#[divan::bench(args = BOOKS, sample_count = SAMPLES, sample_size = 1)]
fn verse_text_folded(bencher: divan::Bencher, name: &str) {
    let book = book(name);
    let mut cache = warmed(book, name);
    let filter = onion::mask::Filter::verse_text();
    let mut next = 1;
    bencher
        .counter(BytesCount::new(book.text.len()))
        .bench_local(|| {
            let typed = &book.keystrokes[next];
            next += 1;
            divan::black_box(cache.masked(divan::black_box(typed), &filter))
        });
}

#[divan::bench(args = BOOKS, sample_count = 20)]
fn verse_text_fresh(bencher: divan::Bencher, name: &str) {
    let book = book(name);
    let typed = &book.keystrokes[0];
    let filter = onion::mask::Filter::verse_text();
    bencher.counter(BytesCount::new(book.text.len())).bench(|| {
        let tokens = onion::lex(typed);
        let tree = onion::cst::build(&tokens);
        divan::black_box(onion::mask(typed.as_bytes(), &tokens, &tree, &filter))
    });
}

#[divan::bench(args = BOOKS, sample_count = 20)]
fn parse_fresh(bencher: divan::Bencher, name: &str) {
    let book = book(name);
    let typed = &book.keystrokes[0];
    bencher
        .counter(BytesCount::new(book.text.len()))
        .bench(|| divan::black_box(onion::wire::plate(&onion::wire::parse(typed, EDITOR))));
}
