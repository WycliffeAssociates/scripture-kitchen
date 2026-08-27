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

//! - [`fused`] — the SINGLE-PASS pipeline: the scanner's `push_token` feeds a
//!   sink that builds the CST and runs lint during the scan. Prices fusing
//!   the whole pipeline into one traversal. PARKED 2026-08-27 (contents and
//!   tests/fused_identity.rs commented out): it copies the scan loop's arms,
//!   and the Pad pass paid that copy tax twice.

//! - [`utf16`] — NOT a lexer variant: the byte↔UTF-16 boundary index, in two
//!   shapes (per-drift-change anchors vs fixed-stride anchors + SWAR count).
//!   Prices the editor-session coordinate translation on dense scripts.

pub mod chapter_par;
pub mod fused;
pub mod scalar;
pub mod staged;
pub mod sweeps;
pub mod utf16;
