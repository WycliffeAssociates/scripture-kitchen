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

/// The editor wire: all seven reads, including the emit and the UTF-16
/// conversion. `lint::full` is its floor.
mod analyze {
    use super::*;

    use usfm_onion::analyze::{Prebuilt, wants};

    /// The tree and tokens supplied, the lint report left to `analyze_from` —
    /// the shape a caller holding ingredients but not products has.
    fn prebuilt<'a>(
        tokens: &'a [usfm_onion::Token],
        cst: &'a usfm_onion::cst::Cst,
    ) -> Prebuilt<'a> {
        Prebuilt {
            tokens,
            cst: Some(cst),
            lint: None,
        }
    }

    #[divan::bench]
    fn full(bencher: divan::Bencher) {
        bencher.counter(bytes()).bench(|| {
            for source in &CORPUS.sources {
                divan::black_box(usfm_onion::analyze::analyze(source, wants::ALL, None));
            }
        });
    }

    /// The ingredients off the clock — what a caller holding a cached tree and
    /// token stream pays. `full` minus this is what caching them can ever buy.
    #[divan::bench]
    fn from_prebuilt(bencher: divan::Bencher) {
        bencher.counter(bytes()).bench(|| {
            for (source, tokens, cst) in built() {
                divan::black_box(usfm_onion::analyze::analyze_from(
                    source.as_bytes(),
                    &prebuilt(tokens, cst),
                    wants::ALL,
                    None,
                ));
            }
        });
    }

    /// …and with the one read the existing fold already caches cleared. The
    /// floor the two cheap moves reach together.
    #[divan::bench]
    fn from_prebuilt_less_diagnostics(bencher: divan::Bencher) {
        bencher.counter(bytes()).bench(|| {
            for (source, tokens, cst) in built() {
                divan::black_box(usfm_onion::analyze::analyze_from(
                    source.as_bytes(),
                    &prebuilt(tokens, cst),
                    wants::ALL & !wants::DIAGNOSTICS,
                    None,
                ));
            }
        });
    }
}
