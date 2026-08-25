# Galley — the workflows crate (piece 5 of 5)

STATUS: NOT YET DISCUSSED with Will — this document organizes the raw
material for that conversation. Sources: Will's voice transcript
(2026-08-25, teased apart below), the Sefer v2 vision
(../scripture-editor-proto-2/plans/editor-v2-rewrite-vision.md), and
the braid-v2 sketch this file replaced (Lexical-era; what survives is
compressed at the bottom, the original is in git history as
ideas/committed/braidv2.md).

## The five-crate layout (ruled 2026-08-25)

1. **onion** — the Rust engine (this repo's library).
2. **onion-wasm** — bindgen + .d.ts + JS-environment utilities
   (UTF-16 walls, wrappers). This is `onion-wasm/` in this repo —
   renamed from `galley/` in pass 10 (2026-08-25), which frees the
   name for piece 5 below.
3. **sous** — the proofreading Rust engine (sibling repo).
4. **sous-wasm** — same as 2, for sous.
5. **galley** — an OPINIONATED set of higher workflows over 1/3
   (natively) or composed into one wasm binary with 2/4 (the vision's
   "composed analysis host", §4.4/§9.4). Dirty-marking, normalizing,
   checksumming, ingest recipes, find, onion↔sous coordination —
   anything that might be considered stateful lives here and NEVER in
   the engines (§9.2: "one narrow seam above the pure domain fns").

## The transcript, teased apart

### For sure (stated or strongly implied)

1. **Sous implementation under the new regime is the next major
   scope.** Rule of thumb from measurements: ~2x on wasm depending on
   the work.
2. **Find lives in galley** (confirms ideas/candidates/
   find-and-overlay.md): memmem for first literal search; the recipe
   is mask (verse_text) → search the projected plane → map hits back
   into the caller's coordinate system (utf8/16, chapter-relative or
   absolute). **aho-corasick for multi-needle sweeps** — the hygiene
   sweep / termlist case searches many needles at once, which is
   aho-corasick's exact shape (it rides on memchr, same maintainer).
3. **Sous's corpus input needs a shape proposal.** Lean from the
   transcript: an array of triples — (label, text, …) where label is
   a book identifier / file path used only for association, never
   read by the engine. Everything mask-shaped stays onion's; sous
   consumes projections.
4. **Galley probably re-exports the onion surfaces** and adds the
   higher-level ops on top — open how much is pure re-export vs
   curated.
5. **Coordinate MODES**: galley hands out a handle/mode that fixes
   the coordinate contract — ABSOLUTE (whole-file offsets) or
   PER-CHAPTER (chapter-relative offsets, the chapter assumed to be
   the whole document). The consumer picks; galley owns the adapter;
   any UI choosing per-chapter owns its chapter-swap thinking.

### Maybes (need measurement or a design ruling)

6. **Caching / checksumming / dirtiness.** Performance so far says
   retention may be unnecessary — "always run the whole book" is the
   standing default (vision rung 1). The checksum-keyed KV cache of
   derived products is exactly vision rung 2 (content-addressed
   reuse). Open: is the complexity ever bought? The checksum EXPORT
   itself (xxh3-128-v1, vision §13.4) was deferred out of onion-wasm
   and lands in galley when save/dirty machinery exists.
7. **Monoid / map-reduce shape of the engine products.** Can CST /
   lint / toc be expressed as a fold over chunks so one edited chunk
   recomputes and the rest just shift offsets (prefix sums)? Toc is
   friendly (it tiles every byte). CST and lint are the open
   question. NOTE: this is rung-3 research (vision: "workers, IPC,
   splice replay, monoids before measurement" is on the REJECTED
   list) — it stays a question, not a plan, until a measurement
   fails. The braid-v2 fold-over-slots sketch is the prior art.
8. **Chapter chunking.** Current speeds say pre-split/pre-scan is
   overhead for onion — but sous may WANT chapter-grain inputs, and
   chunking is a galley concern either way. Sub-question: loop
   overhead of feeding lex one chapter at a time + chapter offsets +
   a file-level prefix. The chapter-independence proof exists
   (experiments/chapter_par.rs: token-identical split at line-initial
   `\c` across both corpora).
9. **Diff granularity + wire.** Always diff the whole book, or mark
   chapter segments dirty? And: should a whole-book diff RETURN
   unchanged segments? (Partially answered already: Unchanged units
   cost ZERO replay splices — a one-verse change returns ~a few
   splices, never the book — but the SKELETON does narrate unchanged
   units; whether the wire should elide them per-chapter is a
   consumer question for the modal.)
10. **Product caching for sous and find** — the recurring tension:
    "performance has suggested we don't have to retain, and maybe
    that's true" — non-retained until a real consumer measurement
    says otherwise.

### Pain points galley exists to solve

- The editor's dirty/save machinery needs checksums and a dirty
  answer that doesn't retain a second full string (§13.4).
- Coordinate multiplicity: utf8 vs utf16 × absolute vs
  chapter-relative — one adapter, owned once (§6.2, §10.3).
- Sous needs corpus-shaped ingest + mask recipes (the "chop it,
  mask verse text, feed sous, map back" pipeline).
- Whole-project operations: analyze every target book on open,
  aggregate ProjectDiagnostics (§11.1 / Horizon 2).

## Recipes (the concrete v1 surface the transcript sketches)

- `ingest(text)` → LF-normalize (→ maybe checksum) → products.
- `proofread(book | corpus)` → mask verse_text (untrimmed) → sous →
  findings mapped back to source bytes → caller's coordinates.
- `find(query, scope)` → mask → memmem/regex (aho-corasick for
  needle sets) → hits mapped back; replacement = Edits at the mapped
  (possibly discontiguous) spans.
- `project(triples)` → per-book pipeline, aggregated diagnostics,
  deterministic book ordering.

## How this addresses the vision, directly

- §4.4/§9.4 one composed wasm module = galley IS that module (deps:
  onion + sous, later linked as one binary).
- §9.2's "narrow stateful seam above pure functions" = galley's
  charter; the engines stay stateless forever.
- §9.6 complexity ladder: rung 1 = today's stateless galley (all
  evidence says it suffices); rung 2 = the checksum KV of item 6;
  rung 3 = resident source + splice replay, whose prior art is the
  braid-v2 sketch below. Promotion requires a recorded failing
  measurement.
- §13.4 checksum utility: galley's, deferred until save/dirty pulls.
- §10.1's "typed common projections + a small composable selector":
  onion's Filter already is this; galley curates the recipes.
- §11.1 whole-project diagnostics = the `project()` recipe.
- §12 project-wide Find = the `find()` recipe (no engine gap —
  see ideas/candidates/find-and-overlay.md).

## Prior art kept from braid v2 (the rest is superseded)

The braid-v2 sketch designed a Lexical-era stateful store (slots at
`\c` boundaries, piece-table backing, splice-as-the-one-primitive,
prediction/ruling/reconcile against the editor tree). The CodeMirror
pivot + the vision's canonical-document model replaced its editor
half wholesale, and the vision's rung discipline demotes its store
half to a measured promotion. Still live:

- **Chapter independence is PROVEN** (experiments/chapter_par.rs) —
  the factual basis for any future chunking, memoization, or
  parallel-open work.
- **The stateless core is the oracle**: retained-vs-fresh equivalence
  as a standing test is the law any rung-2/3 galley must keep.
- **Slot/splice mechanics** as rung-3 prior art: slot-relative spans
  + run-table bases + re-partition-on-splice is a worked design if
  resident source ever promotes. (Its Lexical reconcile protocol is
  dead; CM's canonical funnel replaced it.)
- Superseded outright: incremental lint as a fold (lint is ruled a
  whole-book pass; chunk memoization is its own parked idea), store
  undo (CM canonical history owns it), resident collapsed containers
  (Lexical rendering detail).
