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
`pantry()` hands back `&Pantry` only. The registry is not sealed off for
tidiness: the Expediter has to see every mutation, because for one retention
mode the update is the last moment the text exists.

## Eager for a text the Pantry will not keep, lazy for one it will

`Pantry::update` stays pure Onion: lex, CST, TOC, mask, UTF-16 table, and
nothing of Sous. A host that updates ten `Retain::Text` books and publishes
once pays for one pass over the ten, not ten passes, and the indexing waits:

```text
chapter table for this RawChecksum?
  hit   -> no text is touched, nothing is projected
  miss  -> OnionBook::from_parts(text, mask.clone(), toc.clone())
           for_each_chapter -> ObservationKey per chapter
              absent -> pass.map, mapped += 1
           store the table, DROP the projected text
```

A `Retain::ProductsOnly` book cannot wait. Its text is gone the moment
`update_with` returns, so its table is built there, from the caller's own
`&str`, before the Pantry drops it. That is the whole reason mutation is the
Expediter's method: the earlier arrangement — mutate the Pantry directly, index
at publish — could only answer `Err(NoText)` for the second publication of a
book the host holds the text of.

Projected text is never retained either way. It exists for exactly as long as
it takes to key a book's chapters, and only for a book whose table is missing.
`last_mapped` counts both kinds of map, so an eager one is still visible in the
publication it served.

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
lets one observation serve identical chapters in two books while reduce still
counts both positions. A pass whose `map` reads `ChapterInput::key` would break
that and does not belong behind this cache.

## Why the buffers are equal

`sous_core::for_each_chapter` is the only place a `ChapterInput` is assembled,
and both `analyze` and the Expediter call it, so neither can build an input the
other would not. Reduce is provenance-blind — it cannot tell a cached
observation from a fresh one — and both publishers rebase through the same
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
`n` edits is a table hit and maps nothing.

`resident_bytes` reports what the sweep bounds: the Pantry's own products plus
one entry per resident observation, chapter row, and ring slot. Shallow in one
place — a pass's heap inside an observation is not counted, because
`ChapterPass` states no size.

The sweep is skipped outright when no table was added and no ring aged since
the last one — a republication of an untouched corpus has nothing to free, and
pays nothing to learn it (`publish_unchanged` is unchanged at 14 µs;
evidence.md).

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

It is off by default because it is measured, not assumed: for `Hygiene` the map
is about half a millisecond of a 5.6 ms cold whole-Bible publication and
allocates one `Vec` per chapter, so the parallel path runs 1.7× SLOWER —
work-stealing and allocator contention cost more than the map saves
(evidence.md). The feature is here so a costlier pass can switch it on against
a gate that already holds; the first thing to change then is one `par_iter`
over the whole corpus rather than one per book.

## What is not here yet

A config stamp joins `SnapshotId` when judging config exists.
