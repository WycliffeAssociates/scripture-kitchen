DONE (committed): 1. attr interpreter · 2. exports (USJ 187/187, USX
195/195, HTML + ruled tables) · 5. version-family lint (deprecated-marker,
VERSION_ROWS, deprecated-attribute) · 5.1 positional-context lane ·
Masks/projections + TOC (0a22c73 + delimiter-fold passes d44f351..179ef60)
· vref/ebible render (d2299d1).

BUILT, uncommitted, awaiting Will's review:
- Format (pass 5, 2026-08-24): the Form channel (explicit 5th severity
  variant), 11 Form rows + formatter-bit dual citizens + repairs
  allowlist, one transaction through the existing Fix/check_fixes
  machinery. format_edits()/format() at crate root; options not
  profiles (verse_breaks, char_marker_breaks, newline normalizes,
  remove_markers, repairs). Invariants section = the readable-layer
  contract; corpus tests hold all seven. Ledger: choices.md pass 5 +
  review rulings. Dumps: debug/formatting/. Perf: perf-notes §6
  (PSA 1.5ms). Side effects ruled correct: missing-paragraph run
  aggregation resets at row-0 markers (2,865 → 5,434); slow-oracle
  split (#[ignore] = pass-end gate, CLAUDE.md documents the
  --include-ignored finish line).

1. The diff port — IN FLIGHT (dispatched 2026-08-24; sketch fully
   ruled: planning/sketches/diff-port.md). Anchor-cut blocks from the
   Toc (no derive_canonical_sids), Myers via `similar`, byte-range
   units, derived sid unit ids, merge surface + SpliceEdit in edit.rs,
   token-slice classifiers, serial only. 2-way surface on N-ready
   primitives (n-way is a later sketch; identity layer already N-safe).
   Three-law property core (partition totality, byte round-trip,
   unknown-id rejection) over a hand-rolled generator. Divergence
   stance: pause and present, never silently port onion's token
   machinery. Baseline to beat: perf-notes §7.
2. wasm analyze() when the editor prototype pulls for it — pure Rust
   until then (galley method list accumulating in the wasm sketch;
   boundary costs already measured, perf-notes §5).
3. Braid last — possibly nothing beyond "call the stateless analyze,
   debounced" plus multi-book concerns. Under this world braid is the
   caching/incremental/tiling question, and the engine stays out of it.
   Chunk/parallel/monoid-shape questions land here too (diff and format
   stay serial in the engine).
