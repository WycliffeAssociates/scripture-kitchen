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
//! Profiling one of these:
//!     cargo bench -p usfm_onion --profile profiling --no-run
//!     samply record -- target/profiling/deps/pipeline-<hash> \
//!         lex::serial --sample-count 1 --sample-size 200

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

/// The editor wire: all seven reads, including the emit and the UTF-16
/// conversion. `lint::full` is its floor.
mod analyze {
    use super::*;

    #[divan::bench]
    fn full(bencher: divan::Bencher) {
        bencher.counter(bytes()).bench(|| {
            for source in &CORPUS.sources {
                divan::black_box(usfm_onion::analyze::analyze(
                    source,
                    usfm_onion::analyze::wants::ALL,
                    None,
                ));
            }
        });
    }
}
