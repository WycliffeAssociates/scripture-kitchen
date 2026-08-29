//! The two passes that REWRITE rather than read: format and diff.
//!
//!     cargo bench -p usfm_onion --bench rewrite
//!     # profiling: see the recipe in pipeline.rs
//!     cargo bench -p usfm_onion --bench rewrite -- variants
//!
//! These carry the measurements the playground's `--format-trace` and
//! `--diff-trace` headers used to print best-of-20 into `debug/`.
//!
//! `diff::with_text_words` in particular is measured NOWHERE else: the diff
//! dump renders from plain `diff`, and the words mode only ever appeared as a
//! number in that header.

use std::sync::LazyLock;

use divan::counter::BytesCount;
use usfm_onion::{CharBreaks, FormatOptions, VerseBreaks};

mod common;
use common::{CORPUS, bytes};

#[cfg(feature = "alloc-counts")]
#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main()
}

mod format {
    use super::*;

    /// The whole corpus, default options — `format_edits` alone, which is what
    /// a caller that only wants to KNOW the edits pays.
    #[divan::bench]
    fn edits_corpus(bencher: divan::Bencher) {
        let opts = FormatOptions::default();
        bencher.counter(bytes()).bench(|| {
            for source in &CORPUS.sources {
                divan::black_box(usfm_onion::format_edits(source.as_bytes(), &opts));
            }
        });
    }

    /// The same plus the splice — what a caller that wants the bytes pays. The
    /// gap against `edits_corpus` is the cost of applying them.
    #[divan::bench]
    fn apply_corpus(bencher: divan::Bencher) {
        let opts = FormatOptions::default();
        bencher.counter(bytes()).bench(|| {
            for source in &CORPUS.sources {
                divan::black_box(usfm_onion::format(source.as_bytes(), &opts));
            }
        });
    }

    /// The option variants against each other, on one book — the four the
    /// `debug/formatting/` dumps cover. Whole-corpus would drown the
    /// difference between them in the shared floor.
    const VARIANTS: &[&str] = &["default", "keep-verse-breaks", "remove-s5", "join-chars"];

    /// JON, the book `debug/formatting/` dumps all four variants of.
    static ONE_BOOK: LazyLock<String> = LazyLock::new(|| {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../testData/exampleCorpora/en_ulb/32-JON.usfm"
        );
        std::fs::read_to_string(path).expect("JON is mounted")
    });

    #[divan::bench(args = VARIANTS)]
    fn variants(bencher: divan::Bencher, variant: &str) {
        let opts = options(variant);
        let source: &String = &ONE_BOOK;
        bencher
            .counter(BytesCount::new(source.len()))
            .bench(|| divan::black_box(usfm_onion::format_edits(source.as_bytes(), &opts)));
    }

    fn options(variant: &str) -> FormatOptions<'static> {
        match variant {
            "default" => FormatOptions::default(),
            "keep-verse-breaks" => FormatOptions {
                verse_breaks: VerseBreaks::Keep,
                ..FormatOptions::default()
            },
            "remove-s5" => FormatOptions {
                remove_markers: &["s5"],
                ..FormatOptions::default()
            },
            "join-chars" => FormatOptions {
                char_marker_breaks: CharBreaks::Join,
                ..FormatOptions::default()
            },
            other => panic!("unknown variant {other}"),
        }
    }
}

mod diff {
    use super::*;
    use usfm_onion::diff::TextDiffMode;

    /// The two pairs `debug/diff/` covers: a small book against an unrelated
    /// large one (all added/deleted), and one book against its own retranslation
    /// (nearly all modified — the case that actually exercises the unit matcher).
    const PAIRS: &[&str] = &["jon.ulb-vs-rom.bdf", "mrk.ulb-vs-ult"];

    struct Pair {
        baseline: String,
        current: String,
    }

    static LOADED: LazyLock<Vec<(&'static str, Pair)>> = LazyLock::new(|| {
        PAIRS
            .iter()
            .map(|name| {
                let (b, c) = match *name {
                    "jon.ulb-vs-rom.bdf" => ("en_ulb/32-JON.usfm", "bdf_reg/46-ROM.usfm"),
                    "mrk.ulb-vs-ult" => ("en_ulb/42-MRK.usfm", "en_ult/42-MRK.usfm"),
                    other => panic!("unknown pair {other}"),
                };
                (
                    *name,
                    Pair {
                        baseline: read(b),
                        current: read(c),
                    },
                )
            })
            .collect()
    });

    fn read(rel: &str) -> String {
        let path = format!(
            "{}/../testData/exampleCorpora/{rel}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn pair(name: &str) -> &'static Pair {
        &LOADED
            .iter()
            .find(|(n, _)| *n == name)
            .expect("a listed pair")
            .1
    }

    /// Both sides' bytes: the matcher's work scales with the pair, not one side.
    fn both(p: &Pair) -> BytesCount {
        BytesCount::new(p.baseline.len() + p.current.len())
    }

    #[divan::bench(args = PAIRS, sample_count = 20)]
    fn plain(bencher: divan::Bencher, name: &str) {
        let p = pair(name);
        bencher
            .counter(both(p))
            .bench(|| divan::black_box(usfm_onion::diff(&p.baseline, &p.current)));
    }

    /// Adds a CST + mask per side and the UAX-29 word diff inside each changed
    /// unit — on the retranslation pair that dominates everything else.
    #[divan::bench(args = PAIRS, sample_count = 20)]
    fn with_text_words(bencher: divan::Bencher, name: &str) {
        let p = pair(name);
        bencher.counter(both(p)).bench(|| {
            divan::black_box(usfm_onion::diff_with_text(
                &p.baseline,
                &p.current,
                TextDiffMode::Words,
            ))
        });
    }
}
