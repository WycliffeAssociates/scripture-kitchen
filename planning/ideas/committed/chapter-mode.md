# Chapter mode — the two-loop story, as call stacks

STATUS: STORY DRAFTED (2026-08-26, Will + session), v2 — rewritten as
scenario call stacks after Will's asks: (1) the outer-loop hiccup
risk named with its mitigation ladder, (2) galley residency placed in
galley.ts (the TS seam), NOT wasm — rung discipline kept, (3) the
mode footgun killed by handle types, spelled out.

Premise: **the book is the unit of truth; the chapter is the unit of
interaction.** The CM document IS one chapter's text (galley.md item
5's PER-CHAPTER mode, literally). The spike's projection/rules design
survives untouched: when the document is the chapter there is no
window, so "rules are window-independent" holds trivially.

## Where state and encodings live (the one diagram to trust)

```text
editor (CM)          galley.ts                 wasm (onion, later sous)
UTF-16, chapter- ◄── UTF-16, book-absolute ◄── UTF-8, stateless, pure fns
relative             THE ONE STATEFUL SEAM:
handle-typed         Book = a JS string
                     (§9.2's seam; JS strings
                     are UTF-16 natively, so
                     splices and chunk bases
                     are .length arithmetic)
```

- utf8 exists ONLY inside a wasm call: one encodeInto per outer-loop
  run (measured fast). analyze's own one-wall sweep returns UTF-16.
- Resident source in WASM stays rung 3 (vision §9.6) — untouched.
  Residency in galley.ts is a JS string and a few arrays.

## The handle API (the footgun answer: the mode IS the type)

```ts
galley.ingest(key, fileText) -> Book
  // LF-normalize; remember the census (lf/crlf) for write-out;
  // chunk starts both in utf16 (.length sums) and utf8 (pre_scan)

Book.chapters() -> [{n, heading}]          // toc-derived, the picker
Book.chapter(i) -> Chapter                 // a HANDLE, generation-stamped
Book.diagnostics() -> per-chapter buckets  // book-absolute inside,
                                           // handed out per chapter
Book.serialize() -> fileText               // census restores endings
Chapter.text() -> string                   // what CM loads
Chapter.commit(cmDocText) -> Committed | ChapterSplit
```

Footgun rules, enforced by shape not discipline:

- NO function takes `(string, modeFlag)`. Book methods speak
  book-absolute; a Chapter handle speaks chapter-relative. You cannot
  "send the whole file in chapter mode" — the chapter handle's commit
  only accepts its own CM doc's text, and a whole file pasted INTO
  that doc is just a big chapter edit: commit re-chunks and answers
  `ChapterSplit{chapters}` EXPLICITLY; the editor must handle it.
- Handles carry a GENERATION: any commit invalidates the book's other
  outstanding chapter handles; using a stale one THROWS. Wrong-mode
  use is a loud call-site error, never silently-wrong offsets.
- The editor never adds/subtracts a base itself: offsets arrive
  already in the asking handle's coordinates (item 5: galley owns the
  adapter).

## Scenario stacks — MODE B (chapter mode, the new story)

### Open a file

```text
readFile() -> fileText                              (utf16 JS string)
galley.ingest("01-GEN", fileText) -> book
  normalize LF; census {lf, crlf} kept for save     (galley.ts)
  wasm onion.chunk.pre_scan(utf8)  -> byte starts   (one encodeInto)
  utf16 starts = per-chunk .length prefix sums      (galley.ts)
  wasm onion.analyze(book, DIAGNOSTIC_WANTS)        (~4-10ms, once)
  toc -> book.chapters()                            (the picker)
book.chapter(12).text() -> CM EditorState.create    (doc = chapter)
CM StateField decorations:
  wasm onion.analyze(chapterText, STRUCTURAL_WANTS) (chapter-sized)
  project-structural(chapter) -> rules' projection
  buildRendering(chapter)
  buildInner -> {set, atomic, isolates}             (StateFields stay:
                                                     block decos + the
                                                     atomicRanges/bidi
                                                     facets read state)
```

### Keystroke (inner loop — the hot path)

```text
CM transaction (utf16, chapter-relative — natively)
settle rules: chromeSpans over ScanLine             (unchanged)
doc.toString() -> chapterText                       (~2-100KB — the doc
                                                     IS the chapter;
                                                     "toString(wholeFile)"
                                                     cannot be written)
wasm onion.analyze(chapterText, STRUCTURAL_WANTS)   (sub-ms to low ms)
project-structural(chapter); buildRendering(chapter)
decorations rebuild (chapter-sized)
  -> NO ViewPlugin split, NO RangeSet.map, no non-overlap hazard:
     those levers become UNNECESSARY, not rejected
```

### Debounced commit (outer loop — the truth path)

```text
idle/debounce fires
chapter.commit(doc.toString())
  book string splice (utf16 concat, galley.ts)
  wasm onion.chunk.pre_scan(book utf8)              (~1ms/5MB)
    -> re-chunk; a pasted \c returns ChapterSplit   (explicit)
  re-checksum changed chunks                        (cache key upkeep)
  wasm onion.analyze(book, DIAGNOSTIC_WANTS)        (~4-10ms ← the
                                                     hiccup, see below)
  bucket diagnostics per chapter; open chapter's
  offsets = subtract its utf16 base                 (galley.ts)
editor: setDiagnostics(chapter bucket)              (CM linter/effect)
results panel: other chapters' buckets, click = swap
```

**The hiccup, named:** the outer loop is main-thread ~4-10ms at
typing pauses. Ladder: (1) ship it, measure feel — a missed frame in
a pause drops no keystroke; (2) if felt, the WORKER seam is natural
here and nowhere else: inputs are chapter-sized strings, outputs are
kilobyte diagnostics, nothing is caret-coupled. galley.ts + its wasm
run in the worker; the main thread keeps its own onion-wasm instance
for the inner loop (two instances of one binary, a few MB). That
failing feel-measurement is the vision's legitimate trigger for the
workers rung — not before.

### Chapter swap

```text
chapter.commit(doc.toString())                      (flush)
book.chapter(n) -> new handle (new generation)
CM: new EditorState from .text()
  -> history is per-state: cross-chapter undo is an OPEN RULING
```

### Save

```text
book.serialize()
  resident string is LF-canonical
  census says dominant_ending; expand \n -> \r\n on the way out iff crlf won
writeFile(fileText)
```

## Scenario stacks — MODE A (whole-book, today's spike, for contrast)

```text
open:      readFile -> CM doc = whole file
keystroke: doc.toString(WHOLE FILE)                 (5MB on ULT)
           analyze(book, COMMIT_WANTS, clip=null)   10.1ms
           project (6 whole-book walks)              6.4ms
           buildRendering                            4.2ms
           decorate                                  6.6ms   ≈ 23-27ms
lint:      second debounced analyze(book, DIAGNOSTIC_WANTS)
save:      doc.toString()                           (the doc IS the file)
```

Mode A stays valid for small/unaligned books and needs NO galley
residency at all (every call stateless). It is also exactly mode B's
outer loop — which is why the two modes share one galley: mode A is
"the book handle with the inner loop skipped".

## What is chapter-level vs whole-book, and why

| Work | Level | Why |
|---|---|---|
| CM doc, offsets, decorations | chapter | the document IS the chapter |
| analyze structural reads | chapter | carry 0–1 (galley.md cache table) |
| project-structural, buildRendering | chapter | inputs shrank; the window-independence property holds trivially |
| format (in-view) | chapter | `format_edits_in` is the scoped transaction |
| diagnostics / lint | book | book facts by ruling (ordering, band, duplicate-id); the DIAGNOSTICS wants-bit is NEVER set on a lone chapter — it would false-fire missing-id |
| toc | book | it is the chapter directory |
| checksums, dirty, save, diff | book | assembly + endings census live at the file |
| find, sous, project(), exports | book/corpus | galley recipes |

## Who builds what

- **Onion: nothing.** wants gate, clip, format_edits_in, chunk
  boundaries, rebase math all exist — chapter mode is consumer
  composition, the five-crate layout working as ruled.
- **galley.ts**: the Book/Chapter handle seam above the stateless
  wasm — splice, re-chunk, checksums, census, per-chapter diagnostic
  buckets, generations.
- **Editor**: doc-per-chapter swap flow, ChapterSplit handling,
  results panel keyed by chapter, outer-loop debounce (its lint.ts
  pass IS the skeleton).

## Open rulings

- Cross-chapter undo across doc swaps (CM history is per-state).
- ChapterSplit UX: split the view immediately vs at commit.
- Whole-book reading view (scroll across chapters): presentation, a
  consumer choice — the item-9 modal question unchanged.
- Chunk 0 (front matter) edits as its own view; chapters read row 0
  already models it.

## v3 simplification (2026-08-26, late — Will's stress-test resolved)

Two axes were tangled in v2: WHERE WORK RUNS (a cost question) and
WHAT FRAME THE EDITOR LIVES IN (a rendering question). The win is
entirely the second axis, and it needs none of v2's machinery.

- **Truth 1 rules the engine contract: the engine always receives a
  BOOK.** No splice ingestion, no carry, no chapter addressing in
  onion or galley-wasm, ever.
- **Residency is CUT.** Serialization is memcpy-speed, so
  statelessness costs ~1-2ms of encodeInto per DEBOUNCED outer run —
  while a resident copy's failure mode is silently drifting from the
  editor's truth. Two strings must exist anyway (the CM chapter doc,
  the book pre-save); the EDITOR owns the book string, and commit is
  `prefix + chapterDoc + suffix` in one place with one owner. The v2
  Book/Chapter handles survive at most as STATELESS sugar; the
  generation machinery dies with the state it guarded.
- **"Per-chapter mode" dissolves into two projections, no contract:**
  inner loop = analyze(chapterSlice, STRUCTURAL wants) — outputs are
  chapter-relative BECAUSE the input was the chapter, and the address
  is "the slice I'm holding" (caller-side, nothing retained across
  the call); outer loop = analyze(fullBook, DIAGNOSTIC wants),
  absolute, binned by the `chapters` read FROM THAT SAME RUN — the
  fresh address space ships with every answer.

### The TOC-as-UI question (the chapter grid when the toc shifts)

The grid rebinds from each outer run's `chapters` read — it updates
at debounce cadence, never per keystroke. While typing, the editor
tracks exactly two things: `sliceStart` and the CM doc; the suffix
begins at `sliceStart + doc.length`, so the open tile's LENGTH
changing is free arithmetic, not bookkeeping. Identity across a
re-chunk: tiles are addressed by ORDINAL (position — always unique,
duplicates included) and LABELED by designator; a duplicate `\c 3` is
two tiles, one lint finding. After an outer run re-chunks (a `\c`
typed, deleted, or pasted), the editor re-finds "the chapter I'm in"
by byte-overlap with its slice — a small heuristic, not a protocol.

### Typing `\c` inside the chapter doc (the mid-slice case)

The inner loop doesn't care: structural reads are local, so the new
`\c` renders correctly IMMEDIATELY (it's just a chapter marker in the
slice — scan + CST + the blocks/lines reads lay it out). The
BOOKKEEPING catches up at the next outer run: re-chunk, new chapters
read, grid rebinds, and the editor decides (open ruling) whether the
view now shows both chapters or swaps to one.

### What the inner call actually is

analyze(chapterText, wants{blocks, lines, tokens, verseAnchors,
notes}) — i.e. scan + CST + the structural emits, DIAGNOSTICS bit
never set. That is the whole inner-loop engine surface; everything
else in v2's stacks stands.

v3 is not a different plan from v2 — same deliverable (small
chapter-relative CM doc + debounced whole-book truth), minus the
resident string, the generations, and the worker seam it implied.
The worker option remains available later, unchanged, if the outer
loop's ~4-10ms is ever FELT.

## v4 (2026-08-27) — RULED: whole-doc + clip adopted; doc-per-chapter demoted

Will's call after weighing both as fleshed call stacks: the CM doc is
the WHOLE BOOK; "chapter mode" is a CLIP — two block folds plus an
edit fence — not a different document. One coordinate system, one
loop, no ChapterSplit protocol, cross-chapter undo free (one CM
history), infinite-scroll/read view = clip off, "± a verse of
context" = bump two numbers. v3's engine truths all stand (engine
receives a book; statelessness; TOC-as-UI heuristics); what flips is
only the rendering frame. Doc-per-chapter (v2/v3's MODE B framing)
stays in this file as contrast, not as the plan.

### Debounce ≠ epochs (the clarification that settled it)

Epoch/generation bookkeeping comes from ASYNC + two coordinate
systems, not from debouncing. A debounced pass in v4 reads
`view.state.doc` at fire time and computes + dispatches in ONE
synchronous task — the doc cannot change underneath it (nothing
yields). Zero compose, zero mapPos of stale results, zero
generations. So the shape is TWO CADENCES, BOTH SYNCHRONOUS:

- keystroke: `analyze(clipSlice, STRUCTURAL)` — sub-ms, offsets
  slice-relative, absolute = `+ clip.from`
- timer/idle: `analyze(book, DIAGNOSTIC)` — ~4-10ms, absolute
  natively, no mapping of anything

"Everything every keystroke" (no timer at all) stays as the retreat
position; its cost is the ~10ms whole-book floor per keystroke on
the worst books (over budget at 120Hz). The escape ladder if the
whole-book floor is ever FELT is ENGINE-side (chunk-fold.md), never
frontend epochs.

### The composed editor design (the whole thing)

```text
FENCE (edit/selection protection — ~30 lines)
EditorState.changeFilter.of(tr => {
  const clip = tr.startState.field(clipField)
  if (!clip || tr.annotation(trustedEdit)) return true
  return [0, clip.from, clip.to, tr.startState.doc.length]
  //     ^ pairs = ranges where changes are SUPPRESSED
})
EditorState.transactionFilter.of(tr => /* clamp selection into clip */)

StateField (the sparse MAPPED residue — everything a facet reads)
├─ clip bounds: {from: tr.changes.mapPos(f, 1), to: mapPos(t, -1)}
├─ 2 block folds: Decoration.replace({block:true})   (CM requires
│                  block decos from state anyway)
├─ atomicRanges / bidi isolates                      (map, few ranges)
└─ no overlap hazard: nothing dense lives here

ViewPlugin (everything DENSE — the thousands of inline marks)
└─ update(u): if (u.docChanged || u.viewportChanged)
     rebuild ALL visible marks from current analysis over
     u.view.visibleRanges — fresh every time, no map, no patch,
     no non-overlap cleanup (a from-scratch viewport build cannot
     produce collisions)
```

The spike's two levers resolve as: lever 1 (viewport building)
ADOPTED for all purely-visual marks — anything whose class derives
from token/node kind; lever 2 (RangeSet.map + patch) survives ONLY
in the StateField residue, where it was already mandatory and
already safe (sparse). The split line is "consumed by a facet vs
purely visual" — and "purely visual" == "derivable from NodeKind at
the position", which is what makes the viewport rebuild safe.
Diagnostics ride their own channel (setDiagnostics), book-derived.

### The locality assumption, named + guarded (the inner loop's debt)

`analyze(clipSlice, STRUCTURAL)` assumes the slice read in isolation
equals the whole-book read RESTRICTED to the slice. Weaker than the
fold's laws (no composition — just agreement on the overlap), proven
for tokens (chapter_par.rs), NOT yet proven for the CST-derived
reads (blocks/lines/notes/spans), and known-false exactly on scope
straddles (`\esb`/unclosed `\f` across `\c`). Guard: the chunk-fold
detector (boundary stack non-root) → this keystroke falls back to
`analyze(book, STRUCTURAL)` or the clip widens a chapter. The
standing corpus test lands with chunk-fold pass 1 (slice rebased ==
whole restricted, both corpora; straddle cases route to fusion).
Failure mode is self-announcing: inputs that break locality are the
ones lint flags on screen.

### Open rulings — v4 resolutions

- Cross-chapter undo: RESOLVED — one doc, one CM history, free.
- ChapterSplit UX: DISSOLVED — a pasted `\c` is just text; the next
  outer run re-chunks and the clip bounds move (tile identity by
  ordinal, the v3 heuristic unchanged).
- Whole-book reading view: RESOLVED — clip off; the ViewPlugin never
  knew the difference.
- Chunk 0 (front matter): a clip like any other.
