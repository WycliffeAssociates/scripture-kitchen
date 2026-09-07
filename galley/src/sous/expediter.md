# The Expediter

The Sous coordinator: one `ChapterPass`, one `Pantry`, one complete corpus
publication per call. `galley/docs/analysis-host.md` states the lifecycle
contract it implements; `galley/src/pantry.md` states the registry's half.

This is its own note rather than a section of `pantry.md` because the two
answer different questions. The Pantry's question is ownership — who holds the
text, what survives it, what an id means. The Expediter's is reuse — what makes
skipping a `map` safe, and why a cold analysis and an incremental one publish
the same bytes.

## Mutations go through the Expediter

`Expediter::update`, `update_with` and `remove` forward to the Pantry, and
`pantry()` hands back `&Pantry` only, so no book can be registered behind these
caches' backs. `update_with` is where a `Target` asking for
`Retain::ProductsOnly` is refused — a target's findings are placed by rescanning
its own text, so a target that kept none could be judged and never sited
(`pantry.md`).

## Lazy: nothing is projected until a publication needs it

`Pantry::update` stays pure Onion: lex, CST, TOC, mask, UTF-16 table, and
nothing of Sous. A host that updates ten books and publishes once pays for one
pass over the ten, not ten passes, and the indexing waits:

```text
chapter table for this RawChecksum?
  hit   -> no text is touched, nothing is projected
  miss  -> OnionBook::from_parts(text, mask.clone(), toc.clone())
           for_each_chapter -> ObservationKey per chapter
              absent -> pass.map, mapped += 1
           store the table, DROP the projected text
```

Projected text is never retained. It exists for exactly as long as it takes to
key a book's chapters — or, at publication, to locate one book's sites — and
only for a book whose cache is missing. Reprojecting costs about 38 µs a book,
which is why there is no second copy of the text on either path.

## The two hashes, and the third

| key | over | moves when |
| --- | --- | --- |
| `RawChecksum` | the book's raw bytes | any edit at all, markup included |
| `ObservationKey` | one chapter's projected text + its rebased verse rows + `P::SCHEMA` | the analysis input changes |
| `SnapshotId` | the canonical (`BookKey`, id, `RawChecksum`) table + `P::SCHEMA` | the corpus is a different corpus |

That split is the whole point. Insert a footnote whose content the verse-text
mask removes and the raw checksum moves, so the projection and the UTF-16 table
are rebuilt and every published offset after the insertion shifts — while every
`ObservationKey` stands still, so not one chapter is mapped again. Retype a
verse marker without touching a content byte and the reverse happens: the
projected text is identical but the verse rows are rekeyed, so exactly that
chapter re-maps.

`ObservationKey` deliberately excludes the chapter's own address, which is what
lets one observation serve identical chapters in two books while the fold
still counts both positions. A pass whose `map` reads `ChapterInput::key` would break
that and does not belong behind this cache.

## The aggregate cache

What is cached is the fold product: one `P::Aggregate` per book, in projected
book coordinates and carrying no book index, keyed by the book's
`RawChecksum`. A book whose raw text has not changed folds to the same
aggregate, so `publish` folds only the books whose checksum has no entry.
`last_folded()` counts those, and a republication with nothing updated reports
zero. No book index inside, because the index is whatever *this* publication
assigns; a book that moved from 3 to 2 judges from the same aggregate.

Judging is not cached and is not per book. Every publication calls
`pass.judge_resident` once over every Target book's aggregate in `BookIndex`
order, because a convention is a corpus fact — what one book's counts mean
depends on the others. `Findings::finish` then puts the rows in
`(book_idx, from, to)` order and the rebase runs as before.

That is also why the config lives here rather than in the key:
`set_config` is a re-judge and never a re-fold or a re-map, since neither
`map` nor `fold` is handed the config at all. It may be a re-*locate*, because
moving a band can change which patterns fire — measured at 3.2 ms for the
66-book corpus, against 206 µs for a republication that changed nothing
(evidence.md).

## Retention grain: every chapter, or one book's aggregate

`ChapterPass::RETAIN_CHAPTERS` is each rule's answer to "may a host keep my
per-chapter observation?". `Words` says no: its rows are about 5 KB a chapter
of cased Latin against the substrate's 0.3, which is 4-5 MB more per Bible to
save the ~200 µs it takes to rewalk one edited book (evidence.md, "W1 grain").
A phone kills a tab for memory and never notices 200 µs. A tuple retains
chapters only if every member does, because one observation carries them all.

That also means the cost is paid in whole books — the tuple's `map` runs every
member at once, so there is no remapping one member's slot:

```text
this book's checksum has an aggregate?
  yes -> nothing mapped, nothing folded, no text read
  no  -> map EVERY chapter of the book, fold once, keep the aggregate,
         then pass.release each observation — WordRow::default(), 24 B,
         the same as an uncased chapter's row
```

So a markup-only edit, which moves the `RawChecksum` and not one
`ObservationKey`, now re-maps the book it touched: the aggregate is keyed by
the raw checksum, and the rows that could have folded it again are gone.
`a_markup_only_edit_maps_nothing_and_shifts_the_published_offsets` pins the
chapter-grain claim through `HygieneBytes`, and
`a_book_grain_pass_remaps_the_edited_book_and_nothing_else` pins this one.

Within one publication a chapter mapped for one book is still a hit for the
next, because nothing is shed until every fold that publication needed has
run. That is what keeps two identical books one map, and `last_mapped` honest.

A book-grain pass's aggregate also does not ride the ring: the sweep keeps it
only for a book's CURRENT checksum, dropping every older generation even
while the ring and the chapter tables behind it survive for `with_generations`
more edits. A `WordAggregate` holding thousands of rows costs the same to keep
five generations deep as it does one, which is exactly the heap
`aggregate_bytes` below made visible (evidence.md, 2026-09-04 "the ~58 MiB").
An undo inside the ring therefore still re-maps and re-folds a book-grain
pass's book — the regrain check in `index_book` already forces that whenever
the aggregate is missing — while a chapter-grain pass's undo stays free.
`a_book_grain_pass_keeps_one_aggregate_per_book_through_edits_and_an_undo`
pins it.

## The resident corpus totals

Judging words used to re-merge every book's rows into corpus totals on every
publication: 4.7 ms of a 4.9 ms warm republication (evidence.md, W1). The
Expediter keeps the totals instead, in one `CorpusTotals`, and moves a book at
a time.

```text
tallied[BookId] = the RawChecksum this book contributes to the totals now

checksum unchanged -> nothing at all
checksum moved     -> pass.untally the old aggregate, pass.tally the new
book gone          -> pass.untally, and the id leaves the table
```

The old aggregate is still resident when it is subtracted: a checksum leaves
the ring in `index_book` and leaves `aggregates` in the sweep at the END of the
same publication, and the totals are moved between the two.

What makes it safe is that the tally is exactly a merge. `WordTotals` after any
sequence of adds and removes equals `WordTotals::merge` over the books resident
then — counts, dispersion, and which rows exist at all, so a word held only in
forced positions keeps its all-zero row exactly as a fresh merge does. Cold
`analyze` builds no tally; it calls `judge`, which merges its own. The two
paths' bytes are therefore the same claim `galley/tests/equivalence.rs` already
makes, plus the two words cases it gained: a chapter recased, and the casing
channel flipped off.

## The site cache

A judged pattern has no coordinates; `pass.locate` gives it some by rescanning
one book's current text (`sous-chef/core/src/sites.md`). That is text reading,
so it is cached:

```text
sites[RawChecksum] = (FiringHash, [SiteRow])
```

`FiringHash` is xxh3-128 over the book's firing patterns' **content** — glyph,
channel, key — in table order, and never over their indices. The distinction is
the whole point: a publication renumbers the pattern table whenever any other
book's counts move a denominator, while what THIS book can be sited for is
unchanged, so an index-keyed cache would miss on every keystroke anywhere in
the corpus. For the same reason a cached row names its pattern by content
(`PatternRef`) and resolves to this publication's `PatternIndex` at replay.

A book whose checksum and firing hash both stand replays its rows and reads no
text. `last_located()` counts the books that did not — one after a keystroke,
all of them on a cold open, none on a warm republication. The sweep retains a
site entry exactly as it retains a chapter table, and `resident_bytes` counts
it: the inline row plus its boxed slice.

What this cache does NOT key on is the numerator and denominator of a firing
pattern. Those move constantly and change nothing about where the pattern
occurs; the published row carries only an index into the table, and the table
is re-encoded every publication anyway.

The cache is worth having because `Brigade`'s fold is not free the way
`HygieneBytes`' was: `fold_book` merges every lane of all 1,189 chapters, which
measured 697 µs of a warm whole-Bible republication with nothing changed
(evidence.md). `resident_bytes` counts an aggregate's real heap via
`ChapterPass::aggregate_bytes`, not its inline size; `Brigade` is book-grain
overall (`Words` alone is), so the sweep keeps its aggregate for a book's
current checksum only, not the whole ring — the previous section.

## Why the buffers are equal

`sous_core::for_each_chapter` is the only place a `ChapterInput` is assembled,
and both `analyze` and the Expediter call it, so neither can build an input the
other would not. Fold and judge are provenance-blind — neither can tell a
cached observation or aggregate from a fresh one — `locate` is handed the same
text and chapter rows either way, and both publishers rebase through the same
`rebase_span`, so the two paths differ in what work they skip and in nothing
else. `galley/tests/equivalence.rs` pins that as bytes, not as a claim: a
seeded edit churn republishes after every step and compares against a cold
`analyze` of the same texts, over a synthetic corpus and over a whole Bible.

## Mark and sweep, N generations deep

Every publication ends by sweeping: a chapter table survives only if some
book's ring names its checksum, and an observation only if some surviving table
names its key. A book's ring is its current `RawChecksum` plus the last
`with_generations(n)` before it, four by default; `remove` drops the ring, so
the book's tables go at the next publication.

Why keep any previous generation at all: an undo restores byte-identical
chapter text and therefore the identical `ObservationKey`, so an undo within
`n` edits is a table hit and maps nothing — unless the pass is book-grain, in
which case its aggregate does not ride the ring (the previous section) and the
undo re-maps and re-folds regardless.

`resident_bytes` reports what the sweep bounds: the Pantry's own products plus
one entry per resident observation, chapter row, cached aggregate, and ring
slot, plus the corpus tally's own rows — which the sweep does not bound,
because the tally holds one row per word the current corpus has and no
generation of it. Shallow in one place: the heap a pass hangs off an
observation is not counted, because `ChapterPass` states no size for one. An
aggregate's real heap IS counted, via `ChapterPass::aggregate_bytes`.

The sweep is skipped outright when no table was added and no ring aged since
the last one — a republication of an untouched corpus has nothing to free, and
pays nothing to learn it. With the aggregate cache in front of it, that whole
republication is 6.9 µs for a 66-book Bible (evidence.md).

## Parallel map

`--features parallel` maps a book's missing chapters on rayon's global pool,
and changes nothing a publication says. The missing chapters are queued in
chapter order, `par_iter` keeps that order, and the observations are inserted
from it afterwards, so the chapter table and the buffer are the serial ones
byte for byte. Both settings drop a repeated chapter from the queue the same
way, so `last_mapped` is the same number too.

The map is the only parallel part, and only over the chapters a publication
actually misses. Galley adds no pool of its own, no scheduler, and no
background work: `publish` still returns when the last chapter is mapped. The
queue owns a copy of each missing chapter's projected text and verse rows,
because `for_each_chapter` lends its `ChapterInput` for the callback only; the
serial path stays inside that callback and copies nothing, so it pays for none
of this.

`the_parallel_map_publishes_the_serial_bytes` compares the two publications
inside one binary, and the equivalence gate runs under both settings.

It is off by default because it is measured, not assumed: for `HygieneBytes`
the map is about half a millisecond of a 5.6 ms cold whole-Bible publication and
allocates one `Vec` per chapter, so the parallel path runs 1.7× SLOWER —
work-stealing and allocator contention cost more than the map saves
(evidence.md). `Brigade` is that costlier pass, and it does pay for a cold
open — 33 ms serial against 24 ms parallel — while staying a wash on a
keystroke, so the feature is still opt-in and a host asks for it. The first
thing to change then is one `par_iter` over the whole corpus rather than one
per book.

## What is not here yet

A config stamp joins `SnapshotId` when the judging config carries bands: today
`P::Config` is `()` for every shipped pass, so two publications under different
configs cannot exist.
