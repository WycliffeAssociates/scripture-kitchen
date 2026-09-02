# `sous_core::pass`

The seam every Stage 2 rule hangs on: a chapter map that reads nothing else,
and an ordered book reduce that stitches the seams. The invariants it
implements are charter "Text, seam, and coordinate invariants" 2–5 and
"Observation and cache invariants".

## Map and reduce

```text
book MRK, projected text, chapters in order
  ┌ chapter 1  start 0   ─ map ─→ Observation  ┐
  ├ chapter 2  start 16  ─ map ─→ Observation  ├─ ChapterObs rows, book order
  └ chapter 3  start 41  ─ map ─→ Observation  ┘
                                        │
              Carry::default() ─────────┤ reduce(rows, &mut carry, &mut out)
                                        ↓
                            Findings, projected book coordinates
book GEN
              Carry::default() ← a fresh carry; no state crosses a book
```

`map` receives `ChapterInput`: the chapter's masked projected text, its
verse rows rebased to that text, and its `ChapterKey`. It may read nothing
else, which is what makes chapters independently executable — in any order, on
any thread, or not at all when a host already holds the answer.

`reduce` receives every chapter of one book in order, each paired with the
projected offset `analyze` mapped it at. It rebases to book coordinates and
pushes rows into `Findings`. `Carry` is the only channel between chapters,
and it starts from `Default` at every book.

## Why reduce is provenance-blind

`reduce` takes `&[ChapterObs<Observation>]` and nothing else. There is no
`Some(prior)` argument, no cache handle, and no freshness flag, so a reduce
cannot branch on where an observation came from. A host that retained half the
rows under their chapter content keys and mapped the other half this call hands
reduce a slice indistinguishable from a cold one — which is exactly the
statement "an incremental analysis equals a cold analysis", made structural
instead of tested per rule.

The obligation this puts on a pass author is that `Observation` must be
`Clone + Send + 'static` and chapter-relative. A borrow, an absolute offset, or
a neighbour's fact smuggled into an observation would silently break reuse; the
trait bound refuses the first two.

## Scope: what a pass does not see

Chapters partition only what the producer's chapter rows cover. Under the Onion
adapter chapter 0 is filtered, so front matter never reaches a pass, and a
whole-book scan can therefore report a finding that `analyze` does not. This is
the ruled chapter grain, not a gap to patch here — pinned by
`text_outside_every_chapter_is_not_mapped`.

The same grain makes a run abutting a masked `\c` two findings, one per
chapter, rather than one crossing the seam. A pass that wants the joined run
must declare a `Carry` and merge in reduce; hygiene declares `Carry = ()` and
accepts the split. See [../../rules/hygiene.md](../../rules/hygiene.md).

## `SchemaStamp`

`SCHEMA` is an associated constant, not a field, so it names the pass rather
than an instance of it. A host folds it into every chapter-cache key beside the
projected chapter text's hash. Bump it whenever a map's output shape or meaning
changes; a stale observation under an unchanged stamp is the one failure mode
this contract cannot detect.

## `Findings`

The sink is a thin wrapper over `Vec<PackedFinding>` plus the caller's book
length table and the currently open book. Rows are already the published
representation, so `--publish` hands them straight to `galley::sous` and
`--findings` reads class, run, and span back off them for its debug view. When
a rule needs evidence the 16-byte record cannot carry, that richer row type
joins here and packs on the way out; nothing in this module presumes it stays
a `Vec<PackedFinding>`.

`open_book` before each book's reduce is what lets `push` take a book-relative
span and check it against the right length. A host driving reduce itself
(Galley, in slice B) does the same two calls `analyze` does.

## Passes are not walks

One scalar walk per chapter; byte sweeps as many as are useful. A pass that
needs class bits consumes the substrate walk's observation rather than
decoding text again. Hygiene's byte-level sweep is the exception the rule
allows: it runs at memory speed over a chapter already in cache.
