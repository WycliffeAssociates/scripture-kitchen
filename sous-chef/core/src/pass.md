# `sous_core::pass`

The seam every Stage 2 rule hangs on: a chapter map that reads nothing else,
an ordered book fold that stitches the seams, and one corpus-level judgment
over every book's fold product. The invariants it implements are charter
"Text, seam, and coordinate invariants" 2–5 and "Observation and cache
invariants".

## Map, fold, judge

```text
book MRK, projected text, chapters in order
  ┌ chapter 1  start 0   ─ map ─→ Observation  ┐
  ├ chapter 2  start 16  ─ map ─→ Observation  ├─ ChapterObs rows, book order
  └ chapter 3  start 41  ─ map ─→ Observation  ┘
                                        │ fold(rows)
                                        ↓
                                    Aggregate, book coordinates
book GEN ─ map, fold ─→ Aggregate      ← no state crosses a book
                                        │
   judge([&MRK, &GEN], &config, &mut out) ─→ Findings ─→ finish()
```

`map` receives `ChapterInput`: the chapter's masked projected text, its
verse rows rebased to that text, and its `ChapterKey`. It may read nothing
else, which is what makes chapters independently executable — in any order, on
any thread, or not at all when a host already holds the answer.

`fold` receives every chapter of one book in order, each paired with the
projected offset it was mapped at, and returns one `Aggregate` in book
coordinates with every seam resolved. Seam state is the fold's own local
business; there is no `Carry` on the trait, so nothing can leak between books.

`judge` receives every book's aggregate at once, in `BookIndex` order, changed
or not, plus a `Config`. It is corpus-level because a convention is a corpus
fact: whether "comma attached to a letter" is a slip depends on what every
other book does. It calls `out.open_book(i)` before pushing book `i`'s rows.

Judging is also the only step a `Config` reaches, which is what lets a host
re-judge on a config change without remapping a chapter or refolding a book.

## Then `locate` places what `judge` decided

```text
judge([&MRK, &GEN], &config, out)   → the corpus's pattern table, no coordinates
locate(MRK, MRK's text, MRK's chapters, &MRK aggregate, out)  → MRK's rows
locate(GEN, GEN's text, GEN's chapters, &GEN aggregate, out)  → GEN's rows
finish()
```

`locate` is the one step besides `map` that reads text, and it reads it only to
PLACE what the counts already decided — never to decide anything. It defaults
to a no-op, so a pass with nothing to site says nothing; `Substrate`'s reads
`out.patterns()`, keeps the rows whose glyph this book's own counts hold, and
pushes one `Convention` per matching run. See [sites.md](sites.md).

`firing` is the same filter without the text: it names the table positions
`locate` would scan for, so a resident host can hash them and decide whether
to rescan a book at all. `galley::sous::Expediter` caches a book's rows under
`(RawChecksum, FiringHash)` and replays them when neither moved.

## Row order comes from `finish`

`Findings::finish` stable-sorts every row by `(book_idx, from, to)`, and a
host calls it once after the last judge — `analyze` and
`galley::sous::Expediter::publish` both do. Judges therefore push in whatever
order suits them; the tie-break is the pushing judge's turn, so within a book
A's row precedes B's on an identical span. No wire rule demands that order;
the CLI and the tests read it.

## A tuple is a pass

`(A, B)` implements `ChapterPass`, so two rules ride one set of chapter
inputs: `map` calls both and pairs the observations, `fold` splits the
borrowed pairs into two views and folds both, and `judge` splits the corpus
of pairs into two views and judges both. The composed `SCHEMA` is
`A::SCHEMA.then(B::SCHEMA)` — order-sensitive and never either half, so a host
cannot key a tuple's observations under one member's stamp. `Aggregate` and
`Config` are the pairs. `sous_core::Brigade` — `(HygieneBytes, Substrate)` —
is the product pass.

## Why fold and judge are provenance-blind

`fold` takes `&[ChapterObs<&Observation>]` and nothing else; `judge` takes
`&[&Aggregate]` and a config. There is no `Some(prior)` argument, no cache
handle, and no freshness flag, so neither can branch on where its input came
from. A host that retained half the rows under their chapter content keys and
mapped the other half this call hands `fold` a slice indistinguishable from a
cold one, and hands `judge` cached and fresh aggregates it cannot tell apart —
which is exactly the statement "an incremental analysis equals a cold
analysis", made structural instead of tested per rule.

The observations are BORROWED, so a cache stays their owner and a publication
copies none of them: `ChapterObs<O>` is generic, `analyze` builds its view over
the vector it just mapped, and `galley::sous::Expediter` builds one straight
over its resident map; the same holds one level up, where the corpus view is
built over the Expediter's resident aggregates. `Observation` therefore need
not be `Clone` —
`a_pass_whose_observation_is_not_clone_analyzes` is a pass whose observation is
not, compiling.

The obligation this puts on a pass author is that `Observation` must be
`Send + 'static` and chapter-relative. A borrow, an absolute offset, or a
neighbour's fact smuggled into an observation would silently break reuse; the
trait bound refuses the first two.

## Scope: what a pass does not see

Chapters partition only what the producer's chapter rows cover. Under the Onion
adapter chapter 0 is filtered, so front matter never reaches a pass, and a
whole-book scan can therefore report a finding that `analyze` does not. This is
the ruled chapter grain, not a gap to patch here — pinned by
`text_outside_every_chapter_is_not_mapped`.

The same grain makes a run abutting a masked `\c` two findings, one per
chapter, rather than one crossing the seam. A pass that wants the joined run
merges it in its own fold; hygiene carries no seam state and accepts the
split. See [../../rules/hygiene.md](../../rules/hygiene.md).

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

`Findings` carries the corpus's pattern table beside its rows:
`push_pattern` takes a `judge::Pattern` and returns its table position, and
`into_parts` hands both to the encoder. Patterns are corpus-level, so they are
pushed either side of any `open_book`.

`open_book` before each book's rows is what lets `push` take a book-relative
span and check it against the right length. A host driving the pass itself
(Galley's `Expediter`) makes the same judge-then-`finish` calls `analyze` does.

## Passes are not walks

One scalar walk per chapter; byte sweeps as many as are useful. A pass that
needs class bits consumes the substrate walk's observation rather than
decoding text again. Hygiene's byte-level sweep is the exception the rule
allows: it runs at memory speed over a chapter already in cache.
