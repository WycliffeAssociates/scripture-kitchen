//! The engine's passes over a whole corpus, one bench per pass.
//!
//!     cargo bench -p usfm_onion                     # everything
//!     cargo bench -p usfm_onion -- lint             # lint::full and lint::pass_only
//!     cargo bench -p usfm_onion --features alloc-counts   # + alloc/dealloc columns
//!     ONION_BENCH_CORPUS=../testData/exampleCorpora/en_ult cargo bench -p usfm_onion
//!
//!     lex                    fastest │ median │ mean   │ bytes/s
//!     ├─ serial              3.1 ms  │ 3.2 ms │ 3.2 ms │ 1.4 GiB/s
//!     ╰─ corpus_rayon        0.4 ms  │ 0.4 ms │ 0.5 ms │ 9.8 GiB/s
//!
//! Two shapes, mirroring what a caller actually pays:
//!
//! - `full` — lex and build on the clock. What a caller that wants this
//!   answer and holds nothing pays.
//! - `pass_only` — pre-lexed AND pre-built, so the timing is the named pass
//!   alone. Its `full` sibling is its ceiling.
//!
//! Profiling one of these — the SAME binary `cargo bench` runs, so a regression
//! is profiled where it was measured, with no second harness to keep in step:
//!
//!     cargo bench --profile profiling --no-run          # prints the path
//!     samply record --save-only --unstable-presymbolicate -o p.json.gz -- \
//!         target/profiling/deps/pipeline-<hash> --bench lex::serial
//!
//! `--bench` is required — without it Divan follows the libtest convention and
//! only LISTS. `--unstable-presymbolicate` writes the `.syms.json` sidecar the
//! `profiling` profile's line tables feed; without it every frame is a raw
//! address. Name one bench: a whole-suite profile mixes passes, and Divan's own
//! frames go from ~2% of samples to ~5%.

use usfm_onion::mask::Filter;

mod common;
use common::{CORPUS, built, bytes, tokens};

/// Allocation counting, off by default. MEASURED overhead on the two
/// alloc-heaviest benches here: none detectable — `analyze::full` 28.18 vs
/// 28.21 ms fastest, `vref::pass_only` 36.54 vs 35.10. vref does ~415k
/// allocator ops in 35 ms, so a counter increment is well under the noise.
/// Off by default because the extra columns are usually just noise on screen,
/// not because the clock is compromised.
#[cfg(feature = "alloc-counts")]
#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main()
}

mod lex {
    use super::*;

    /// The honest per-core number.
    #[divan::bench]
    fn serial(bencher: divan::Bencher) {
        bencher.counter(bytes()).bench(|| {
            for source in &CORPUS.sources {
                divan::black_box(usfm_onion::lex(source));
            }
        });
    }

    /// What the whole corpus costs wall-clock. Embarrassingly parallel over
    /// books, so this mostly reads out core count.
    #[divan::bench]
    fn corpus_rayon(bencher: divan::Bencher) {
        use rayon::prelude::*;
        bencher.counter(bytes()).bench(|| {
            CORPUS.sources.par_iter().for_each(|source| {
                divan::black_box(usfm_onion::lex(source));
            });
        });
    }
}

mod toc {
    use super::*;

    #[divan::bench]
    fn full(bencher: divan::Bencher) {
        bencher.counter(bytes()).bench(|| {
            for source in &CORPUS.sources {
                let tokens = usfm_onion::lex(source);
                divan::black_box(usfm_onion::toc(source.as_bytes(), &tokens));
            }
        });
    }

    #[divan::bench]
    fn pass_only(bencher: divan::Bencher) {
        bencher.counter(bytes()).counter(tokens()).bench(|| {
            for (source, tokens) in CORPUS.sources.iter().zip(&CORPUS.tokens) {
                divan::black_box(usfm_onion::toc(source.as_bytes(), tokens));
            }
        });
    }
}

mod cst {
    use super::*;

    #[divan::bench]
    fn full(bencher: divan::Bencher) {
        bencher.counter(bytes()).bench(|| {
            for source in &CORPUS.sources {
                let tokens = usfm_onion::lex(source);
                divan::black_box(usfm_onion::cst::build(&tokens));
            }
        });
    }

    #[divan::bench]
    fn pass_only(bencher: divan::Bencher) {
        bencher.counter(bytes()).counter(tokens()).bench(|| {
            for tokens in &CORPUS.tokens {
                divan::black_box(usfm_onion::cst::build(tokens));
            }
        });
    }
}

mod lint {
    use super::*;

    #[divan::bench]
    fn full(bencher: divan::Bencher) {
        bencher.counter(bytes()).bench(|| {
            for source in &CORPUS.sources {
                let tokens = usfm_onion::lex(source);
                let cst = usfm_onion::cst::build(&tokens);
                divan::black_box(usfm_onion::lint::lint(source.as_bytes(), &tokens, &cst));
            }
        });
    }

    #[divan::bench]
    fn pass_only(bencher: divan::Bencher) {
        bencher.counter(bytes()).counter(tokens()).bench(|| {
            for (source, tokens, cst) in built() {
                divan::black_box(usfm_onion::lint::lint(source.as_bytes(), tokens, cst));
            }
        });
    }
}

mod mask {
    use super::*;

    /// Both recipes, as a caller that wants a masked view pays for them.
    #[divan::bench]
    fn full(bencher: divan::Bencher) {
        bencher.counter(bytes()).bench(|| {
            for source in &CORPUS.sources {
                let tokens = usfm_onion::lex(source);
                let cst = usfm_onion::cst::build(&tokens);
                for filter in [Filter::verse_text(), Filter::structure()] {
                    divan::black_box(usfm_onion::mask(source.as_bytes(), &tokens, &cst, &filter));
                }
            }
        });
    }

    #[divan::bench]
    fn pass_only(bencher: divan::Bencher) {
        bencher.counter(bytes()).counter(tokens()).bench(|| {
            for (source, tokens, cst) in built() {
                for filter in [Filter::verse_text(), Filter::structure()] {
                    divan::black_box(usfm_onion::mask(source.as_bytes(), tokens, cst, &filter));
                }
            }
        });
    }
}

mod vref {
    use super::*;

    /// toc + verse_text mask + the string assembly a keys/lines pair costs.
    #[divan::bench]
    fn pass_only(bencher: divan::Bencher) {
        bencher.counter(bytes()).bench(|| {
            for (source, tokens, cst) in built() {
                let bytes = source.as_bytes();
                let toc = usfm_onion::toc(bytes, tokens);
                let m = usfm_onion::mask(bytes, tokens, cst, &Filter::verse_text());
                divan::black_box(usfm_onion::vref::keys(&toc, &m, bytes));
                divan::black_box(usfm_onion::vref::lines(&toc, &m, bytes, true));
            }
        });
    }
}

/// The boundary: parse, then plate. `lint::full` is the floor of the first and
/// the second is pure serialization, so the pair says what crossing costs over
/// what computing costs.
mod wire {
    use super::*;

    use usfm_onion::wire::{self, ParseOptions};

    const ALL: ParseOptions = ParseOptions {
        diagnostics: true,
        toc: true,
        utf16: false,
    };

    /// Everything: lex, build, lint, toc, and the buffer.
    #[divan::bench]
    fn full(bencher: divan::Bencher) {
        bencher.counter(bytes()).bench(|| {
            for source in &CORPUS.sources {
                let parsed = wire::parse(source, ALL);
                divan::black_box(wire::plate(&parsed));
            }
        });
    }

    /// The same, in UTF-16 — the one sorted sweep over every offset in the
    /// dish. `full` subtracted from this is what an editor's addressing costs.
    #[divan::bench]
    fn full_utf16(bencher: divan::Bencher) {
        let opts = ParseOptions { utf16: true, ..ALL };
        bencher.counter(bytes()).bench(|| {
            for source in &CORPUS.sources {
                let parsed = wire::parse(source, opts);
                divan::black_box(wire::plate(&parsed));
            }
        });
    }

    /// The tree alone — what a consumer that only renders structure pays.
    #[divan::bench]
    fn tree_only(bencher: divan::Bencher) {
        bencher.counter(bytes()).bench(|| {
            for source in &CORPUS.sources {
                let parsed = wire::parse(source, ParseOptions::default());
                divan::black_box(wire::plate(&parsed));
            }
        });
    }

    /// PLATING ALONE, off an already-parsed document: the writer, and the
    /// price of the boundary with the engine subtracted out.
    #[divan::bench]
    fn plate_only(bencher: divan::Bencher) {
        let parsed: Vec<_> = CORPUS
            .sources
            .iter()
            .map(|source| wire::parse(source, ALL))
            .collect();
        bencher.counter(bytes()).bench(|| {
            for p in &parsed {
                divan::black_box(wire::plate(p));
            }
        });
    }
}
