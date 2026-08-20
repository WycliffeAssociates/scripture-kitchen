# Chunk memoization (and chapter-relative coordinates generally)

Ruled 2026-08-18/19; extracted from NEXT-STEPS so that file stays an open
queue. Nothing here is scheduled — this is the SHAPE incrementality takes
IF a real device/corpus measurement ever demands it. The stateless
`analyze(text)` contract (see NEXT-STEPS "Settled facts") is unchanged by
everything below: memoization can only make a call faster, never change
its answer.

## The ruling

**If incrementality is ever needed, it is memoization INSIDE the
stateless call, never the splice** (Will, 2026-08-18). No `apply_edits`,
no stateful coordinate updates, no keeping the USFM string in sync
piecemeal — that whole family (the braid's buffer-sync layer, the
demoted `BookSession`) is what this replaces.

## Mechanics

Full string in, every call:

1. **Pre-chunk**: memchr `\c` scan (the chapter_par splitter already
   exists) → fresh chunk boundary list.
2. **Hash** each chunk's bytes (content hash = identity).
3. **Memo lookup** per chunk: artifacts (tokens, CST fragment, lint,
   UTF-16 drift anchors) cached by content hash, stored in
   chunk-RELATIVE coordinates — the chapter-relative-spans idea
   token.rs has carried since braidv2.
4. **Rebase on the way out**: absolute from/to = chunk-relative + base,
   where bases are prefix sums of the FRESH chunk list, recomputed per
   call and never stored. The `+ base` rides the emit loop that already
   exists — reads are typed arrays copied out fresh per call, so the
   rebase is an add inside a write you were doing anyway; zero extra
   passes. UTF-16 indexes memoize per chunk the same way, so no
   stateful coordinate machinery exists anywhere.

## No invalidation machinery

Every call re-chunks the current string from scratch and hashes.

- Edit inside a chapter → that chunk's bytes miss, every other chunk
  hits.
- Edit a `\c` itself (`\c 99` → `\c 9`) → that chunk's bytes changed →
  miss; delete a `\c` → the merged range is a new hash → miss. The
  "reset on structural edits" behavior falls out; no reset logic is
  written.
- Stale entries fall to mark-and-sweep (entries untouched for N calls).

Chunk identity = fresh boundaries × content hash, so no stored chunking
can ever disagree with the document. State you recompute can't be stale;
a memo table can't be wrong, only cold. Pre-scan boundaries need only be
DETERMINISTIC, not semantically perfect — a pathological `\c` costs
extra misses, never correctness.

## Two questions resolved 2026-08-19 (design only — zero-cache stands)

**"Should the lexer record chunk-relative offsets?" — the question
dissolves.** Relativity is a property of the SLICE you hand the lexer:
`lex(&source[chunk])` yields chunk-relative offsets by construction
(the slice starts at 0); `lex(whole_file)` yields absolutes. Same
lexer, no chunk-awareness (which would couple it to `\c` semantics the
lexer is forbidden to know), and the TOC bridges the two frames in one
subtraction (`absolute − base`). Nothing to build.

**The lint cross-chunk monoid inventory** (the machines are already
feedable structs, so per-chunk = `(incoming) → findings + (outgoing)`):
Ordering's verse state resets at `\c` — chunk-local by construction;
only the chapter sequence crosses (prev number + token, trivial).
numbering-mix is a true monoid (bitmask OR, first-seen min; the
finding is PLACED at compose time). missing-id/declared-version are
chunk-0 facts broadcast forward. The CST seam (section above) remains
the only genuinely unsolved crossing.

**Budget check keeping all of this parked**: full staged pipeline
~31 ns/token; heaviest aligned book ~7 ms native, ~15-20 ms at wasm's
2-3×, behind a 150 ms debounce — order-of-magnitude headroom with zero
cache. The subsystem that can't afford recompute is proofreading/sous,
whose caching is ruled SOUS-SIDE (masked checksums over the slab);
the engine takes no cache until a real device measurement demands it.

## Priced caveat: chunks are not perfectly independent for the CST

A sidebar spanning `\c`; an unclosed frame at a chunk boundary is
Recovery whole-file but Eof per-chunk. Needs sous's seam-monoid
treatment (per-chunk summary of frames open at each edge, composed
across the seam) or the blunter
boundary-crossing-frame-poisons-both-chunks. Decide when building, by
measuring how often real corpora cross.

## Chapter-relative coordinates are useful WITHOUT caching (Will, 2026-08-19)

The coordinate scheme stands on its own, independent of the memo table:

- **Partial loading**: our prototype keeps the full book in scope, but a
  consumer might load only ONE chapter into an editor (CodeMirror doc =
  one chapter's text). It then wants chapter-relative offsets natively —
  which is exactly what per-chunk artifacts already are, before the
  rebase step. Skipping the `+ base` IS the feature.
- **The inverse is one subtraction**: given a Token/Node in whole-file
  absolute offsets and the chapter's base (a prefix-sum lookup),
  absolute − base = chapter-relative. Both directions are arithmetic
  against the same fresh prefix sums; no second representation is
  stored.

So chunk-relative is the natural STORAGE form and absolute is a
per-call VIEW — which consumer gets which is just whether the emit loop
adds the base.

## Relation to sous-chef

Sous's incrementality is the same principle one level up (see the slab
bullet in NEXT-STEPS): the stateless producer emits the whole TOC slab
per call; sous's content-addressed stats store skips its own expensive
walk on checksum hits. The checksum IS the protocol, and it only works
because the producer is pure and total per call. The old chapter_par
verdict ("split doesn't pay") measured parallelizing the SAME work —
memoization's win is hits × skipped cost, so it can be architecture for
sous (expensive walk) while staying an escape hatch for the engine
(microsecond passes).
