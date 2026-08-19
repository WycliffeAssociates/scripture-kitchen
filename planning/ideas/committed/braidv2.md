# Braid v2 — the stateful layer (slots, splice, reconcile)

Sketch of everything settled or firmly leaning about the working-store /
editor-session layer. Sources: the retired QUESTIONS log "E — Statefulness and
granularity" + Q9, the onion_editor_chef prototype (tabs A–D, Q1–Q27),
`experiments/chapter_par.rs`. Items marked (leaning) are direction; (open)
needs prototype evidence.

## The inversion that everything else hangs off

- **The library IS the working-files store; the editor owns pixels.**
  Lexical is a disposable lens over the store — windowing is a rendering
  parameter, not a data-model commitment. One store, main thread, no
  worker mirror, no MutationEffect-style negotiation (onion's scar).
- **Lib default stays SPEC**: book in, book resolved, book-absolute
  offsets (structural state — milestones — legitimately crosses `\c`).
  Slot-relative is the STORE's internal representation only.

## Slots

- **`\c` is the hard boundary event.** A book session is a list of SLOTS
  in source order; slot 0 = front matter. Slots are positional containers
  with session-minted EPHEMERAL ids (nothing durable keys on them).
  Chapter labels are display data derived from the slot's own tokens;
  duplicate/out-of-order chapters are just slots whose labels collide.
- **Spans are slot-relative inside the store.** Each slot's base offset
  lives in the run table; edit slot 3 → slots 4..N are byte-identical,
  only bases shift (one add each, or a lazy prefix sum).
- **ParseHeader is the coordinate adapter**: its run-table bases remap
  book-absolute ↔ slot-relative by one subtract/add (run found by binary
  search over bases) — the store adopts ONE spec parse into slots without
  re-lexing. Toc, single-chapter-materialization index, and coordinate
  adapter are the same structure, zero extra fields.
- **Independence is proven, not assumed**: `experiments/chapter_par.rs`
  (`--chunked`) produces token-identical output splitting at line-initial
  `\c` across both corpora — no SCAN state crosses a chapter.

## The backing: a piece table at chapter granularity

- A slot's text is EITHER a range of the originally loaded source OR an
  owned replacement String swapped in by splice. Unedited slots never
  copy; edited slots own their bytes.
- Save = `concat(slot texts)` — lossless by the partition invariant;
  untouched slots contribute original bytes verbatim.
- Search never straddles disk-vs-edited: every slot answers
  `text() -> &str` and search doesn't care which backing. (Content search
  — skip markers, match across ws — is a per-slot projected plane +
  offset map: a cacheable derivation invalidated by splice. Parked.)
- (open) Original held as one String + ranges (true piece table) vs copy
  each slot's text on load (~one book of RAM, no lifetime headache).

## The ONE edit primitive: splice

- `spliceSlots(contiguous slot range, replacement text) -> Effect`.
  The store lexes the replacement and RE-PARTITIONS at whatever `\c` it
  finds. Slot structure is always OUTPUT, never instruction — no
  delete/merge/split operations exist anywhere.
- Case table (all the same call):
  - keystroke: 1 → 1 (updateSlot IS splice)
  - paste a `\c` mid-chapter: 1 → 2
  - drag-delete across 1.5 chapters: 3 → 1 (tail "flows into" the first
    slot by re-partition, not by an operation)
  - insert-chapter button: 0 → 1 (EMPTY RANGE — Vec::splice semantics)
  - select-all + type: N → M
- Deletion anchoring: the dirty range = min..max of containers the editor
  touched; no `\c` before the range start → slot 0; none after → last.
- **Effect shape is small and closed** (contrast MutationEffect's
  book-grain sprawl): re-scanned tokens for touched slots + findings
  delta + slot-list delta (created/removed/changed ids with positions).
  (open) exact shape — same experiment as the container-set diff.

## The protocol: prediction / ruling / reconcile

- **Lexical's tree mutation is a PREDICTION. The store's re-partition is
  the RULING. Reconcile settles the difference.** The store never trusts
  the editor's structure — only its serialized TEXT. Often the prediction
  is right (drag-delete merges containers; store rules 1 slot; no-op).
  When wrong (typed `\c` mid-verse: Lexical thinks "still 1 container",
  store rules 1→2), reconcile conforms the tree: split the container,
  mint the new slot's id, reflow the tail. The slot is created by the
  STORE; the container is created by the reconciler in response.
- This turns the prototype's observed "byte-identical self-healing by
  luck" (select-all destruction restored by store-is-truth) into
  self-healing BY DESIGN.
- (open) Id survival across re-partition: positional-prefix matching
  (first-in keeps its id, rest kill/mint) covers every known case; the
  test that might break it is "selection ate the range's first `\c`".
  Same question as prototype Q18's container-set diff.

## Editor integration (prototype-verified where noted)

- Chapter = Lexical container node; the tree IS the map (which slot =
  ancestor lookup; the only mapping is container id → slot id). Node keys
  are Lexical internals; anything persistent uses minted ids.
- One-way data flow: keystrokes edit text → serialize dirty container
  range → splice → store returns Effect → editor reconciles containers.
  Keystrokes never create/destroy containers directly.
- Attribution is a hint, never correctness: node-in-container → slot;
  ambiguous → bracket both; lost → re-scan the book. Every rung degrades
  to "re-scan wider"; bytes determine position, always.
- **Virtualization: resident collapsed containers (tab D verdict).** All
  containers stay in Lexical's tree; far ones evict children and render
  as fixed-height non-editable stubs (`canBeEmpty() → true` was the one
  Lexical concession). MANDATORY: build collapsed directly, never
  build-then-evict (46.8ms vs 1.1ms, prototype Q23). D buys an exact
  scrollbar, caret-fix-is-a-flag-clear on split, and self-heal; steady
  state is a wash vs swap-window. Untested past 151 containers (whole
  Bible ≈ 8×).
- **Selection rule (next prototype round)**: while a rangeSelection is
  active, do NOT collapse/unmount containers the selection could extend
  into — mount up to worst-case whole-book DOM (Psalms ≈ 23.5k nodes) so
  copy-across-chapters works; resume collapsing when selection drops.
- Deleted vs emptied falls out of bytes: `\c` line gone → slot gone
  (retire id); `\c` survives childless → empty slot (lint's business).
- Undo/redo is STORE-level (already true in the editor for domain
  reasons): (slotId/range, before, after); undo is just another splice.
- `moveSlots([ids], beforeId)` is token-space free — slot-relative spans
  untouched; serialization order + lint fold re-chain.
- Bounds: the BOOK is the rescan bound and the scroll bound — virtual
  scroll within a book, never across books.

## Lint integration

- Incremental lint is a fold over slots:
  `(incoming state) → findings + (outgoing state)`; recompute an edited
  slot, propagate downstream only if outgoing state changed (rare).
  (Superseded: lint is a whole-book pass — see planning/lint-sketch.md;
  if incrementality ever returns it is chunk memoization,
  ideas/committed/chunk-memoization.md.) Per the spec rail, no block state
  legitimately crosses `\c`; the cross-chapter residue (milestone
  pairing, label duplicates, tallies) is the small explicit incoming
  state.

## Performance posture

- Per-keystroke work = rescan one slot + relint one slot + refold: tens
  of µs. This — intelligent slot-level caching — is the real prize; it
  beats any whole-book % win by orders of magnitude (whole-book parse is
  a once-per-open event at ~1.3ms/book serial).
- Main thread plausible again; sync wasm (colorless functions — real
  scars from mutating across async boundaries) IF a latency measurement
  holds. The worker only ever existed because whole-book relint was
  100ms+.
- Chapter-par (rayon inside a book) exists (`experiments/chapter_par.rs`)
  but is a whole-project-open tool above a size threshold — never the
  editing loop.

## The oracle discipline

- The stateless core (send usfm/tokens → get results) remains the
  always-works fallback AND the oracle: scan/interpret/lint of a slot are
  the same pure functions in both paths. Retained-vs-fresh equivalence is
  a standing test, not a hope.

## Open (consolidated)

1. Id survival rule under adversarial splices (prototype).
2. Effect/container-set-diff exact shape (prototype).
3. Piece-table backing vs copy-on-load.
4. Who owns live text between keystrokes — store-owned transactions vs
   Lexical-owned with trailing sync: a LATENCY MEASUREMENT, not an
   argument (E section Q1).
5. Content-search plane details (parked).
6. Scale test past 151 containers.
