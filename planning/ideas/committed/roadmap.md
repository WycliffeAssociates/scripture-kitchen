DONE (committed): 1. attr interpreter · 2. exports (USJ 187/187, USX
195/195, HTML + ruled tables) · 5. version-family lint (deprecated-marker,
VERSION_ROWS, deprecated-attribute) · 5.1 positional-context lane ·
Masks/projections + TOC (0a22c73 + delimiter-fold passes d44f351..179ef60)
· vref/ebible render (d2299d1).

1. Format — IN FLIGHT (arch settled with Will 2026-08-24, sketch fully
   ruled: planning/sketches/format.md; rulings banked in choices.md).
   The Form channel: severity gains an explicit 5th variant; formatter
   = Form rows + formatter-bit dual citizens + repairs allowlist, all
   through the existing Fix/check_fixes machinery as ONE transaction.
   format_edits() -> Vec<Edit> and format() -> bytes at crate root.
   Options not profiles: verse_breaks, char_marker_breaks, newline
   (normalizes), remove_markers, repairs; block-like derived from the
   tables category. Invariants section is the readable-layer contract
   (idempotence, interior-verse-text sanctity, no invented content,
   conserved diagnostics, determinism, total options).
2. The diff port — sketch + research addendum ready for discussion
   (planning/sketches/diff-port.md). Port from usfm_onion (NOT the
   spike); anchor-cut blocks from the Toc replace String sids; units
   are byte ranges; SpliceEdit is the replay artifact; round-trip law
   asserted on bytes. "diffN" resolved: the planned n-way
   generalization, no code exists — 2-way now, n-way is a later sketch
   (the anchor cut already removes its known blocker). Open: SpliceEdit
   home, `similar` dep, merge-surface timing, rayon gate.
3. wasm analyze() when the editor prototype pulls for it — pure Rust
   until then (galley method list accumulating in the wasm sketch).
4. Braid last — possibly nothing beyond "call the stateless analyze,
   debounced" plus multi-book concerns. Under this world braid is the
   caching/incremental/tiling question, and the engine stays out of it.
