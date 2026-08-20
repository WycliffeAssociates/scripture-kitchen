# Braid sketch (roadmap 7 — an inventory, deliberately not an architecture)

Braid is last on purpose (Will, 2026-08-17: "least well designed, needs
most thoughtfulness"), and it has collapsed twice already. This sketch
answers Will's roadmap note — "ATM UNDER THIS WORLD BRAID WOULD BE,
CACHING WORK? INCREMENTAL UPDATES / TILING BINARY?" — by inventorying
what dissolved, what might genuinely remain, and what the honest math
says about each remnant. Nothing here is a commitment to build.

## What dissolved, and into what (all RULED elsewhere)

| old braid responsibility | where it went |
|---|---|
| buffer sync / splice replay | demoted `BookSession` (unbuilt, measured-someday; the utf16→byte + right-to-left recipe is fully priced in settled-facts) |
| id stability across edits | no-minted-identity law: content-derived addresses, `ChangeDesc.mapPos` in-session |
| incremental re-lex / re-lint | chunk-memoization.md (parked; zero-cache budget holds at ~31 ns/token) |
| chapter TOC / structure index | the TOC (sketches/toc-vref-slab.md — ParseHeader grown) |
| keeping the USFM string in sync piecemeal | dead: the editor's buffer IS the truth; `analyze(text)` per call |
| incremental lint slot fold (braidv2.md §lint) | superseded: whole-book lint at ~9 ns/token; the monoid inventory is recorded in chunk-memoization.md for the day it's needed |

What braidv2.md still describes that nothing supersedes: nothing at the
single-book level. The file stays as history.

## Candidate remnants — each with its honest math

### (a) Caching work — mostly NOT braid's

Three caches exist in the design, and none of them lives in a braid:

- Engine chunk memoization: INSIDE the stateless call, parked until a
  device measurement demands it (chunk-memoization.md).
- Sous stats cache: SOUS-side, keyed by the slab's masked checksums.
- The UTF-16 index: rebuilt per call, never patched.

What could be braid's: a PROJECT-SCOPED memo table shared across books
(one editor session, 66 books). But the math is thin: whole-66-book
en_ulb re-analysis is ~8.5 ms native, ~25 ms wasm — a cold open, once.
VERDICT: caching is not a braid justification today.

### (b) Incremental updates — already answered elsewhere

Per-keystroke: `analyze(text)`, debounced 150 ms, worst book ~20 ms
wasm. Cross-book edits don't exist (a keystroke touches one book).
VERDICT: "incremental updates" as a braid concern is empty until a
measurement says the stateless call is too slow somewhere real.

### (c) The tiling binary — a durable chunk store (PROPOSED shape, if wanted)

Reading Will's note as: persist per-chunk artifacts so a project opens
without re-analyzing everything. The shape, if built, is exactly the
chunk-memoization store made durable:

```text
sidecar file per project (or per book):
  entry = { chunk content-hash (xxh3-128)
          , artifact kind (tokens | cst | lint | toc row)
          , chunk-RELATIVE artifact bytes }
  open: pre-chunk current text → hash → hits load, misses compute
  write-back: misses appended; mark-and-sweep on save
  identity: fresh boundaries × content hash — the store can be stale
    only in the sense of COLD; it can never be WRONG (same law as the
    in-memory memo)
```

The honest question attached: recompute is ~10 ms/project native. A
sidecar beats that only when (wasm startup + 66 books) matters on a
weak device, or when the persisted artifact is EXPENSIVE derived work —
which is sous's stats, which already has its own checksum store.
VERDICT: park; re-open only with a startup measurement from a real
device. The design costs nothing to keep on this page.

### (d) What seems genuinely braid's: the multi-book session

The one layer nothing else claims:

- project format (which books, which order, manifest ↔ files);
- cross-book operations: whole-project lint sweep, search across 66
  books (the mask + prefix-sum search artifact per book, iterated),
  navigation (book/chapter/verse jump — the TOC per book, held);
- the aligned-corpus proofing workflows (sous integration: feeding
  slabs, receiving stats);
- diff/merge orchestration across books (the diff port is per-book;
  braid would drive it project-wide).

This is coordination, not computation — a thin layer OVER `analyze`
and the exports, holding no derived state the pure calls don't mint.

## Recommendation

Braid v3 = (d) only: a multi-book session/coordination layer, designed
AFTER wasm ships and the editor's real usage shows which cross-book
operations exist. (a) and (b) stay dissolved. (c) stays parked with its
shape recorded above. If (d) also turns out to be expressible as "a JS
loop over per-book `analyze` calls" — a live possibility — braid
finishes its collapse into nothing and this page becomes its tombstone.

## Open

1. Does (c)'s tiling binary match what Will meant by the note? If he
   meant something else (e.g. a TILED WIRE FORMAT — per-chunk typed
   arrays so the editor uploads/downloads only changed chapters), that
   is a different sketch and worth a sentence from him.
2. Is the project manifest braid's or the app's? (Lean: the app's —
   braid then shrinks further.)
3. Revisit trigger: first real multi-book workflow in the CM editor, or
   first sous integration — whichever lands first.
