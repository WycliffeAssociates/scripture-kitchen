//! Benchmark-only lexer variants. NOT part of the design — each exists to
//! price one architectural question in the playground, and each is verified
//! token-for-token against `crate::lex` before timing.
//!
//! - [`scalar`] — the same lexer with the text arm's memchr3/memmem swapped
//!   for a plain byte loop. Prices exactly what SIMD scanning buys.
//! - [`chapter_par`] — memmem pre-scan for `\n\c ` boundaries, then lex each
//!   chapter chunk independently (serially or via rayon) and rebase spans.
//!   Prices the "parallelize one book at chapter granularity" strategy.

//! - [`staged`] — two-stage structural indexing (the simdjson shape): two
//!   uninterrupted sweeps build position arrays, then a stop-free stage 2
//!   walks them. Prices the per-short-run scan-call overhead.

pub mod chapter_par;
pub mod scalar;
pub mod staged;
pub mod sweeps;
