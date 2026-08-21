DONE (committed): 1. attr interpreter · 2. exports (USJ 187/187, USX
195/195, HTML + ruled tables) · 5. version-family lint (deprecated-marker,
VERSION_ROWS, deprecated-attribute) · 5.1 positional-context lane.

1. Masks/projections + the TOC index (reframed 2026-08-21, voice memo).
   ONE mechanism — an in-order CST walk with subtree skipping (CST, not
   tokens: "text is not verse text" is a property of the enclosing scope,
   and skipping a footnote is O(1) at its node) — feeding TWO artifacts:
   - MASK: sorted kept byte ranges + a small Filter (coarse token-kind
     over fine CST-scope; include/exclude notes, character markup,
     milestones, unknowns; presets like verse_text()/structure() are just
     documented Filter constructors, so no presets-vs-options schism).
     Consumers pick their view: .ranges() borrows for offset-preserving
     callers (editor, diff), .text() materializes for readers (sous
     proofreading), and offset mapping goes both ways so a finding on the
     materialized string maps back to source bytes. Use cases driving it:
     copy-a-Bible's-structure-minus-footnotes for new projects;
     proofread-verse-text-only.
   - TOC: the chapter/verse anchor index (ParseHeader grown to tile the
     file). locate(byte or utf16) -> human sid for diagnostics and scroll
     sync; chapter ranges give the CodeMirror chapter-window clamp. vref
     is RENDERED FROM the TOC (resolves the old "maybe a mask instead?"
     note — it's the verse-granularity index serialized, not a filter).
   - Editor caret-validity/clamping stays CLIENT-side: it is a mask
     consumed as ranges + TOC chapter bounds; braid/editor config picks
     the filter. Nothing stateful in the engine; both artifacts built per
     call (walks are ns/token — no caching until a measurement demands).
2. Format (as usfm_onion's): remove extra line breaks, paras to their own
   lines, none between verses, etc.
3. The diff port — offset-world question settled before porting: port the
   trusted algorithm, re-speak its boundary as crate::edit::Edit lists
   over byte offsets, addressed by TOC coordinates (why diff sits after
   item 1: addressing for free). Fallback: port as-is, adapt later.
4. wasm analyze() when the editor prototype pulls for it — pure Rust
   until then.
5. Braid last — possibly nothing beyond "call the stateless analyze,
   debounced" plus multi-book concerns. Under this world braid is the
   caching/incremental/tiling question, and the engine stays out of it.
