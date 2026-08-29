//! The three serializations, pre-lexed and pre-built — the pass alone.
//!
//!     cargo bench -p usfm_onion --bench export
//!     # profiling: see the recipe in pipeline.rs
//!
//! Each is behind its own default-on feature, so a build that dropped one has
//! no bench for it either. Rates are over SOURCE bytes and tokens, the two
//! things all three passes share; emitted size differs per format and is a
//! fact for a dump, not a rate.

mod common;
use common::{built, bytes, tokens};

#[cfg(feature = "alloc-counts")]
#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main()
}

#[cfg(feature = "usj")]
#[divan::bench]
fn usj(bencher: divan::Bencher) {
    bencher.counter(bytes()).counter(tokens()).bench(|| {
        for (source, tokens, cst) in built() {
            divan::black_box(usfm_onion::usj::usj(source.as_bytes(), tokens, cst));
        }
    });
}

#[cfg(feature = "usx")]
#[divan::bench]
fn usx(bencher: divan::Bencher) {
    bencher.counter(bytes()).counter(tokens()).bench(|| {
        for (source, tokens, cst) in built() {
            divan::black_box(usfm_onion::usx::usx(source.as_bytes(), tokens, cst));
        }
    });
}

#[cfg(feature = "html")]
#[divan::bench]
fn html(bencher: divan::Bencher) {
    bencher.counter(bytes()).counter(tokens()).bench(|| {
        for (source, tokens, cst) in built() {
            divan::black_box(usfm_onion::html::html(source.as_bytes(), tokens, cst));
        }
    });
}
