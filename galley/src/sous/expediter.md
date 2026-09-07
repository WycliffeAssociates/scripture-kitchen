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
caches' backs. `update` takes the role's own retention, so
`update(id, Role::Reference, text)` keeps no text; `update_with` is where a
`Target` asking for `Retain::ProductsOnly` is refused — a target's findings are placed by rescanning
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

That cost is paid in whole books, but not in whole observations: a chapter
whose `ObservationKey` still stands keeps the members `release` never emptied,
and `ChapterPass::remap` walks only the shed ones.

```text
this book's checksum has an aggregate?
  yes -> nothing mapped, nothing folded, no text read
  no  -> for EVERY chapter of the book:
           key absent      -> pass.map, every member walks
           key held, shed  -> pass.remap in place, only the shed member walks
           key held, whole -> nothing
         fold once, keep the aggregate, then pass.release each observation
         — an empty WordRow flagged released, 24 B, the same as an
         uncased chapter's row
```

`last_mapped` counts a remap as a map of that chapter, because a chapter was
read and walked; `last_remapped` is how many of those kept an observation.
`a_keystroke_in_one_chapter_rewalks_words_for_the_book_but_glyphs_only_for_the_chapter`
pins the split through a counting wrapper around one tuple member: the words
walk the edited book whole, the glyph walk runs for the edited chapter alone.
The flag is why the check is honest — `release` sets it, so an uncased
chapter's genuinely empty row is never taken for a shed one.

## The hot set: two books do not shed at all

Shedding is per book, so the last step of a publication asks which books are
worth exempting. The answer is the ones a keystroke will land in again: an
editor types into one book for minutes at a time, and the second keystroke
there should not pay for the first one's decision.

```text
hot = the books whose text moved most recently, newest first, at most N (2)

fold done -> for every book folded this publication:
               in hot -> keep its rows whole
               else    -> pass.release each observation
             for every book that just fell out of hot:
               pass.release each observation of every generation it holds
```

The set is ordered by *edits*, not by publications: `index_book` moves a book
to the front exactly when its ring takes a new checksum, so republishing an
untouched corpus reorders nothing. `with_hot_books(0)` is the behaviour before
there was a set, and every test that pins the shed-grain law asks for it.

What this buys is the difference between a cold book's keystroke and a hot
one's. Cold, the aggregate is gone and every chapter's rows are shed, so the
whole book walks again — `last_mapped` 16, `last_remapped` 15 for MRK. Hot,
the rows are still whole, so only the chapter whose `ObservationKey` moved is
absent: `last_mapped` 1, `last_remapped` 0. That is the entire word rewalk of
an edited book, gone from every keystroke after the first
(evidence.md, W1e step 2).

What it costs is N books' word rows, which `resident_bytes` counts through
`ChapterPass::observation_bytes`: 281 KB for two books of `en_ulb`, ~137 KB
each, against the 24 B a shed row leaves. `remove` takes a book out of the set
without owing anything, because its rows go with its ring at the next sweep.
Releasing is never a correctness question — a released row is walked again on
demand — so a row two books share may be shed for the cold one and simply
re-walked for the hot one.

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

## The declared source

A `Role::Reference` book is not a target and never becomes one: it is mapped by
nobody, folded by nobody, holds no chapter table and no aggregate, and
publishes no section of its own. What it holds is one grapheme count per verse,
and that verse's word hashes only when the config in force at `update` would
judge with them (`pantry.md`).

`publish` runs the source comparison immediately after `judge_resident` and
before `locate`, from the lengths both sides already retain:

```text
pass.length_config(config)         -> the knobs, from the member that owns the lane
pass.verse_lengths(aggregate)      -> the target lane, per Target book
pantry.reference_lengths(id)       -> the source rows, per Reference book
pantry.reference_words(id)         -> its word sets, if it was asked to keep any
PairedBook::pair_with(target, ..)  -> one book's ratios, its presence rows AND
                                      its source-copy runs; cached together,
                                      facts dropped
judge_paired(books, project, ..)   -> codes 0, 3 and 4 over target spans
```

The source-copy lane is the one part of this step that reads text: on a pair
MISS with the lane on, the target book's projection is rebuilt from the
products it already retains and its words are walked. That projection is
hoisted above both text-reading steps — the site rescan below TAKES it rather
than building a second — so a book is projected at most once per publication
and is released as soon as it is located. A pair hit reads nothing, so an
unchanged republication with the lane on still walks no text.

A declared source registered while `source_copy` was OFF kept no word lane
(`pantry.md`), so it is skipped and counted: `last_wordless_references()` is
how many target books paired against such a source. Nonzero means "re-send
those references", never "no run was found".

Every Target pairs with the Reference of the same `BookKey` — the first, if a
caller registers two files under one key, since `books(Role::Reference)` is
canonically ordered and the choice has to be an order rather than a hash. A
Target with no Reference gets no ratios and no rows; that is the contract, not
an error, so a host may declare a source for part of a corpus.

Three properties follow, and each is a test:

- **a source change touches no target observation.** Registering, replacing, or
  removing a reference moves no target checksum, so nothing is re-mapped,
  re-folded, or re-located, and every target-only row is byte-identical either
  side of the swap. The length rows are the only thing that moves. This is the
  charter's "source choice legitimately changes results without invalidating
  target-only observations", pinned in `equivalence.rs` and in the Expediter's
  own tests;
- **the length knobs are the same judging config as every other knob.** They
  ride `JudgingConfig::lengths` and reach the step through
  `ChapterPass::length_config`, so `set_config` moves them and still maps
  nothing and folds nothing;
- **references ride `SnapshotId`.** Swapping the declared source changes what
  the publication is OF, not only what it says, so the reference table is
  hashed beside the target one under its own role byte.

The facts pairing returns — target-only and source-only keys, ambiguous
duplicates, partial overlaps — are dropped here. They are alignment structure,
and the two the presence rule reads it has already turned into rows before this
point (`rules/presence-shear.md`); a host that wants the facts themselves runs
the cold `analyze_paired`, which returns them, and `sous-cli` prints them as
per-book counts. `LengthConfig::enabled`, `presence` and `source_copy` are
independent, and pairing runs while ANY of them is on.

## The paired cache

Pairing is a pure function of the two books' rows, so it is cached like every
other product here — keyed by BOTH sides, because either moving is a different
sample:

```text
paired[(target RawChecksum, source RawChecksum, words walked)]
  = PairedBook { ratios, target spans, coalesced presence rows,
                 maximal source-copy runs of >= 2 words,
                 the book's knob-free Spread }
```

`last_paired()` counts the books that missed: all of them on a cold open, one
after a keystroke, none on a warm republication or a `set_config`. The third
key member is not a knob but a property of the ENTRY — a pairing made without
the word walk holds no runs and cannot answer for one that wants them — so
turning `source_copy` on misses and re-pairs, and turning it off misses back
onto the entry it already had. `source_copy_min_run` is nowhere in here: the
runs are cached from a floor of two and the knob filters them at judging time.
Nothing else is either, because neither the pairing nor a book's order
statistics read a knob — `Spread` is knob-free and `LengthConfig::min_verses` gates it at
judging time, which is what makes a length knob a re-judge and never a re-pair.

The project scope is the pooled sample over every paired book, and it too is
kept: the same key sequence is the same multiset of ratios, whatever order the
books contribute them in, so a publication that re-paired nothing reuses the
pooled `Spread` verbatim. A keystroke recomputes it, which is the honest floor
— one book moving moves the pool.

The sweep is by live keys: an entry no target named has no book on either side
any more. Removing the last reference, or switching the lane off, drops the
cache whole, because nothing is left to key it by.

What this buys is measured: a warm 66-book republication with the corpus
declared as its own source goes from 4.9 ms to 0.79 ms and a keystroke from
6.6 ms to 2.95 ms, so a declared source now costs 53 µs warm and 382 µs on a
keystroke (evidence.md, 2026-09-07). What it costs is 494 KB for a whole Bible
— 31k units at 8 B of ratio and 8 B of span — which `resident_bytes` counts.

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
generation of it. Both sides are real heap and not inline size:
`ChapterPass::aggregate_bytes` for an aggregate and
`ChapterPass::observation_bytes` for a chapter row, which is what makes a hot
book's unshed rows visible where they are held rather than free by omission.

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

## The handle over it

`galley::wasm::Galley` IS an `Expediter<Brigade>` and nothing else, so the
publication a JS host reads is this one. `galley/src/wasm.md` states the
equivalence and its three tests.

## What is not here yet

A config stamp joins `SnapshotId` when the judging config carries bands: today
`P::Config` is `()` for every shipped pass, so two publications under different
configs cannot exist.
