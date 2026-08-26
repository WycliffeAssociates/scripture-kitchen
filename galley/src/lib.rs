//! galley — the workflows crate (piece 5 of the five-crate layout).
//!
//! ```text
//! usfm_onion ◄── onion-wasm          scripture_sous_chef ◄── sous-wasm
//!     ▲              ▲                        ▲                   ▲
//!     │              │ (wasm feature)         │                   │
//!     └────────── galley ◄────────────────────┴───────────────────┘
//! ```
//!
//! The OPINIONATED layer over the engines: checksumming, ingest recipes,
//! find, onion↔sous coordination — anything that might be considered
//! stateful lives here and NEVER in the engines. The engines never see
//! each other; galley is the only place they meet.
//!
//! Design authority: planning/ideas/committed/galley.md (the build plan
//! and its rulings) in the workspace root's planning tree.

/// The whole engine, as a module: `galley::onion::lex`, `galley::onion::
/// analyze` — nothing hidden, so a consumer never needs to reach around
/// galley. Galley's own top-level names are the curated layer on top.
pub use usfm_onion as onion;
