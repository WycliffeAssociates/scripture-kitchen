# Chunk fold — CST + lint as per-chunk products, one Carried seam

STATUS: CONVERGED (2026-08-27, Will + session). This PROMOTES what
galley.md's monoid section parked: the fold SHAPE is bought now; what
stays measurement-gated is marked inline. Two engine passes: CST
first, then lint. If nobody ever calls the chunked path, whole-book
`analyze` behaves exactly as today — the fold is additive.

SEQUENCING (ruled): (1) burn down the tiny USER-OPEN items, (2)
discuss ../onion-2-spike/RFC-Lexer-change-8-27.md — it changes tests
and pins here, so it lands BEFORE either pass — (3) CST pass, (4)
lint pass, (5) galley cache wiring.

## Why promoted (the reasoning that moved the line, compressed)

- Neither editor loop needs this on its hot path — the inner loop is
  chapter-sized input by construction (chapter-mode.md v4). The
  customers: frame-budget headroom for the whole-book DIAGNOSTIC pass
  (~3–10ms → ~1ms once other subsystems exist and compete for the
  frame), sous chapter-grain reuse (by charter), mask reuse, and
  native parallel project-open.
- The "permanent tax on lint's growth" objection died: the partition
  is a FIELD, not a folder. Cross-book observations are declared on
  ONE concrete struct (`Carried`); a new chapter-local rule costs
  exactly today's "write the walk", and a new cross-book rule pays
  its declaration visibly, in the type system. **Map observes,
  reduce judges.**
- Locality (slice-built == whole-book restricted) is testable
  black-box today, but the carry types are the STRUCTURAL version of
  that empirical fact; the fold arrives rung by rung with the oracle
  green throughout.

## The laws (both standing tests, non-negotiable)

1. **The oracle**: per-chunk built + folded == fresh whole-book,
   byte for byte, on both corpora — PLUS the adversarial straddles
   (`\esb` across `\c`, unclosed `\f` across `\c`, empty-`\p`/verse
   runs ending exactly at a seam and spanning THREE chunks, chapter
   numbering anomalies across the seam) proving the fusion path
   fires and converges.
2. **Left fold only.** The reduce is sequential in document order.
   Associativity is NOT proven and NOT required — a parallel reduce
   over ~50 tiny structs could never repay its proof. The map side
   (per-chunk lex→cst→lint_local) IS embarrassingly parallel:
   par_iter freely, native only (rayon-gated; wasm has no threads).

## The pipeline (whole-book truth path, one ingest or keystroke)

```text
pre_scan(book)                          ~1ms/5MB — chunk starts + census
per chunk: xxh3-128 checksum → cache hit?
  hit:  reuse {ChapterCST, mask, ChunkSummary}     — the ~49
  miss: lex(chunk) → cst(chunk) → lint_local(chunk) — the ~1
        cst reports boundary stack; non-root at a non-final chunk
        end → FUSE with next chunk, rebuild the pair (repeat while
        open) — fused product cached under the fused span's hash
merge: prefix sums (utf8 + utf16 bases); flat-arena concat =
       shift u32 indices + append (chapter_par.rs rebased() is the
       existing proof of the shape)
carried: left-fold Carried summaries → seam diagnostics; finish()
         on the final accumulator → once-per-book diagnostics
assemble: sorted merge of (rebased local diagnostics ++ carried
          diagnostics) by (anchor, code)
```

Checksums are the ONE dirtiness mechanism (ruled 2026-08-27): the
same code path serves editor keystrokes, `ingestCorpus` baselines,
project open over disk files, and sous. The ~0.9ms/5MB hash pass is
the floor and is inside budget. `tr.changes.iterChangedRanges()`
stays available later as a pure HINT (skip hashing provably
untouched chunks) — nothing may depend on it.

## What is cached, what is not (revises the galley.md table)

Per chunk, keyed by content hash, ALWAYS chunk-relative:

- **ChapterCST** — today's flat CST, chunk-relative offsets;
  mergeable by shift+concat. (The table's old "CST: no, cache
  derivatives" is revised: the byte-budget LRU makes the size
  objection self-limiting, and the fusion path wants it resident.)
- **mask (verse_text)** — feeds sous and find (already a YES).
- **ChunkSummary** — `{ diagnostics: Vec<Diagnostic>, carried:
  Carried }` (see the lint pass below).
- later: **sous findings** (the motivating customer, unchanged).

NOT cached: **lex tokens** (ruled 2026-08-27). CST and lint are
downstream; a clean chunk never needs its tokens back, and the dirty
chunk re-lexes as part of its rebuild. lex_chunk_equivalence.rs is kept as the
PROOF of the rebase-concat pattern and its measured numbers; its
rebasing utils (`extend_rebased`, `concat_absolute`,
`chapter_relative`) survive as the merge primitives. Token caching
machinery does not wire in.

Coordinates: storage is chunk-relative, period. Absolutize and
utf16-ize are OPT-IN maps on the way out (`pos + base[chunk]`,
prefix sums over stored utf8/utf16 chunk lengths — recomputed in
microseconds, the only thing that changes for clean chunks). Cache
entries NEVER mutate; undo/redo are pure hits (the old content hash
is still resident until evicted).

### The LRU (ruled 2026-08-27: handroll)

```rust
struct ChunkCache {
    map: HashMap<Hash128, Entry>,   // Entry { products, bytes, last_tick }
    tick: u64, bytes: usize, budget: usize,  // budget ~8-16MB
}
// hit: bump last_tick. insert: add; while bytes > budget evict
// min(last_tick) — linear scan over ~100s of entries, microseconds.
```

~40 lines, zero deps, byte-weighted natively. Crates considered and
declined: `lru` caps by count not bytes; `quick_cache` has the right
weighter but is a sharded concurrent design (galley's seam is
single-threaded; wasm has no threads); `mini-moka` is heavier still
(wasm binary size). Graduation path if the handroll grows warts:
`quick_cache` with a byte weighter. Eviction sweep piggybacks on the
merge step (mark hashes referenced this fold, evict by budget).

## Pass 1 — CST

`cst(chunk_slice)` → `ChapterCST`: structurally today's flat CST,
offsets chunk-relative. Merge = shift node indices/offsets by prefix
sums + concat arenas.

**The seam detector vs the fix** (do not conflate): "suppress
Eof-recovery unless this is the last chunk" is the DETECTOR — a
non-root stack at a non-final chunk end means the slice tree is
WRONG (an `\esb` spanning the seam truly extends into the next
chunk; no closure-code renaming fixes the extents). The FIX is
fusion: rebuild `chunk[n] ∪ chunk[n+1]` as one slice, repeating
while the boundary stays open. On well-formed text the detector
never fires; when it fires, it fires on exactly the inputs lint
already flags as broken structure.

**Chunk 0 is the special one**: it owns `\id`/headers/intro,
establishes the usfm version and the positional band the chapters
inherit — its products feed Carried like any chunk, but adversarial
tests should hit it specifically (edits to headers must dirty only
chunk 0).

**Locality test lands with this pass** (black-box, no fold needed to
state it): for every chunk of both corpora,
`analyze(slice, STRUCTURAL)` rebased == `analyze(book, STRUCTURAL)`
restricted to the chunk — the standing corpus test that pins the
editor inner loop's assumption (chapter-mode.md v4), with the
straddle cases as its named expected-failures routing to fusion.

## Pass 2 — lint

The four walk files KEEP their names and loops (ordering, ancestry,
structure, flat). The change per walk: stop JUDGING cross-chunk
facts in-walk; RECORD them into `carried`. Judgment concentrates in
`reduce` + `finish`.

```rust
struct ChunkSummary {
    diagnostics: Vec<Diagnostic>, // chunk-local, judged in-walk,
                                  // chunk-relative, pre-sorted
    carried: Carried,             // observations only, no judgments
}
struct Carried {
    // CST seam (pass 1's detector bit — ONE seam artifact, not two)
    cst_open_at_end: bool,
    // ordering — a textbook first/last semigroup
    first_chapter: Option<(u32, Anchor)>,   // value + chunk-rel anchor
    last_chapter:  Option<(u32, Anchor)>,
    // flat — pure lattice math
    band: u8,                               // monotonic max
    family_masks: [Mask; FAMILIES],         // OR
    first_seen:   [u32;  FAMILIES],         // min; mixed re-judged at merge
    has_id: bool, has_usfm: bool,
    // ancestry — one bit
    verse_run_reported: bool,               // paragraph-less run straddling \c
    // structure — THE ONLY REAL WORK: run stitching
    open_run_at_end:   Option<RunState>,    // empty-\p run in progress at seam
    open_run_at_start: Option<RunState>,    // …resumed at chunk start
}
```

- **Reduce also emits**: `reduce(acc, next) -> (Carried,
  Vec<Diagnostic>)` — seam judgments (duplicate/out-of-order/gap
  chapters, runs straddling `\c`) anchor at `carried_anchor + base`.
  `finish(final)` emits the once-per-book rules (missing chapter,
  no-`\id`, verse-before-first-chapter, empty-verse collapse).
- Three of four machines contribute COUNTING (max/OR/min/first/last/
  bits). The one genuinely fiddly part is structure's RUN STITCHING
  (a run ending exactly at the seam; a run spanning three chunks) —
  one field pair, one reduce arm, every edge case already named in
  the oracle's adversarial list.
- Migration is incremental: move ONE machine's cross-chunk judgments
  into reduce at a time; the oracle stays green throughout.
- Future rules route themselves: chapter-local → write the walk,
  today's cost, cacheable and parallel for free; cross-book → add a
  Carried field + a reduce/finish arm — the declaration IS the field.

## What this plan does NOT include (still measurement-gated)

- Parallel reduce / associativity proofs (left fold is the law).
- Any wasm-side threading (native-only par map, rayon-gated).
- Editor-transaction dirtiness (checksum is the mechanism; the
  transaction hint is a parked optimization).
- Resident source / splice replay (rung 3, untouched).
- ChunkSummary-only carried pass replacing the merged-CST walk for
  carried rules is NOT a gate: v1 may run carried rules as a cheap
  whole-book walk over the merged product if that's less code; the
  Carried struct is the v2 refinement if that walk ever measures.
  (Will's "opt1 as v1, opt2 as the shape" — pick at implementation
  time, oracle guards both.)

## Consumers, once landed

- galley `analyze`-equivalent keeps its signature; chunked path is
  internal. Editor (chapter-mode.md v4) sees only latency drop.
- sous chapter-grain recipe (galley.md rung 0) rides the same cache.
- Native parallel project-open: par map over books × chunks.
- USER-OPEN item 14's proptest trigger ("rung-2 chunk reuse actually
  promotes") is now MET when this lands — bank the candidate note.
