# The Expediter

The Sous coordinator: one `ChapterPass`, one `Pantry`, one complete corpus
publication per call.

```text
sous.update("books/mrk.usfm", Role::Target, &text)?
sous.publish()?          -> SOUS corpus buffer, raw-book UTF-16
sous.last_mapped()       -> 6      // six chapters mapped
sous.publish()?          -> the same bytes
sous.last_mapped()       -> 0      // nothing re-read, nothing re-mapped
```

This note is publication ORDER and the counters that report it. What every
cache is keyed by, and what invalidates it, is one table in
`galley/src/pantry.md` — the caching layer this sits on.
`galley/docs/analysis-host.md` states the lifecycle contract both implement.

## Mutations go through the Expediter

`Expediter::update`, `update_with` and `remove` forward to the Pantry, and
`pantry()` hands back `&Pantry` only, so no book can be registered behind these
caches' backs. `update` takes the role's own retention, so
`update(id, Role::Reference, text)` keeps no text; `update_with` is where a
`Target` asking for `Retain::ProductsOnly` is refused, and where a `Reference`
asks for `Retain::Text` and gets a searchable projection with it (`pantry.md`);
neither changes what `publish` walks, which is the Target set alone. Re-sending
a Target as a Reference is a withdrawal from the target set and is treated as
one: the ring and the hot slot go at once, the aggregate is untallied by the
publication that no longer lists it, and the sweep takes the rest.

The four free-text doors — `lint`, `parse`, `parsed`, `masked` — forward to the
Pantry too. They take loose text the host holds, register nothing, and are how
a wasm handle serves an editor's onion products off the same warm chunks the
analysis reads.

## The order of a publication

`Pantry::update` stays pure Onion: lex, CST, TOC, mask, UTF-16 table, and
nothing of Sous. A host that updates ten books and publishes once pays for one
pass over the ten, not ten passes, and the indexing waits until `publish`:

```text
1  index    per Target book, key its chapters and map the missing ones
              chapter table for this RawChecksum?
                hit  -> no text is touched, nothing is projected
                miss -> OnionBook::from_parts(text, mask, toc)
                        for_each_chapter -> ObservationKey per chapter
                           absent      -> pass.map        every member walks
                           held, shed  -> pass.remap      only the shed member
                           held, whole -> nothing
                        store the table, DROP the projected text
2  fold     per book with no aggregate: pass.fold, then release the rows it
              read — except the hot set's, which are the point of keeping one
3  tally    the books whose checksum moved: untally the old, tally the new,
              and name the word keys those two moves touch
4  judge    pass.judge_kept over EVERY Target aggregate, in BookIndex order,
              re-deciding only the named keys and keeping the rest
5  pair     every Target against the Reference of its BookKey, from lengths
              both sides already retain; judge_paired pushes codes 0, 3 and 4
6  locate   per book: replay the cached site rows, or rescan its text —
              chapter by chapter for a hot book, whole otherwise
7  sweep    drop every table outside a ring, and every observation no
              surviving table names
8  rebase   each row through its book's retained mask and UTF-16 table,
              then encode
```

Projected text is never retained. It exists for exactly as long as it takes to
key a book's chapters — or, at step 5 or 6, to read one book's text — and only
for a book whose cache missed. One projection per book per publication at most:
the pair step fills a slot on a miss and the locate step TAKES it. Reprojecting
costs about 38 µs a book, which is why there is no second copy on either path.

Judging is corpus-level and never per book: what one book's counts mean depends
on the others, so step 4 runs over every Target aggregate whether or not
anything moved. `Findings::finish` then puts the rows in `(book_idx, from, to)`
order.

## The counters

Every one of these is "what the LAST publication did", and every one is zero on
a republication with nothing updated.

| counter | counts | after one keystroke |
| --- | --- | --- |
| `last_mapped()` | chapters mapped, remaps included | 1 hot, 16 cold (MRK) |
| `last_remapped()` | of those, the ones that kept an observation and re-walked only what `release` shed | 0 hot, 15 cold |
| `last_folded()` | books folded; the rest judged a cached aggregate | 1 |
| `last_words_judged()` | corpus word-tally keys re-decided | 1,731 of 14,093 |
| `last_firing_walks()` | books whose firing set was walked | 1 |
| `last_located()` | books rescanned for sites | 1 |
| `last_sited_chapters()` | chapters of those books whose word rows were walked; zero for a book outside the hot set, which is sited whole or not at all | 1 |
| `last_paired()` | Target books re-paired against their source | 1 |
| `last_wordless_references()` | sources that would have been walked for copy runs and kept no word lane | 0 |
| `resident_observations()` / `resident_tables()` / `resident_aggregates()` | what the cache holds after the last sweep | — |
| `resident_bytes()` / `tally()` / `budget()` | `pantry.md` | — |

`last_mapped` counts a remap as a map of that chapter, because a chapter was
read and walked. A cold open answers "all of them" to every row above; a knob
moved through `set_config` answers zero to `last_mapped` and `last_folded` and
"all of them" to `last_located`, because moving a band can change which patterns
fire — measured at 3.2 ms for the 66-book corpus, against 206 µs for a
republication that changed nothing (evidence.md). `last_firing_walks` answers
"all of them" only when the knob moved the table's CONTENT: the firing cache is
keyed by `TableHash`, so a knob that renumbers the table or moves nothing in it
answers zero.

## Retention grain: every chapter, or one book's aggregate

`ChapterPass::RETAIN_CHAPTERS` is each rule's answer to "may a host keep my
per-chapter observation?". `Words` says no: its rows are about 5 KB a chapter
of cased Latin against the substrate's 0.3, which is 4-5 MB more per Bible to
save the ~200 µs it takes to rewalk one edited book (evidence.md, "W1 grain").
A phone kills a tab for memory and never notices 200 µs. A tuple retains
chapters only if every member does, because one observation carries them all.

That cost is paid in whole books, but not in whole observations: a chapter
whose `ObservationKey` still stands keeps the members `release` never emptied,
and `ChapterPass::remap` walks only the shed ones. `release` sets a flag, which
is what keeps the check honest — an uncased chapter's genuinely empty row is
never taken for a shed one.

Within one publication a chapter mapped for one book is still a hit for the
next, because nothing is shed until every fold that publication needed has run.
That is what keeps two identical books one map, and `last_mapped` honest.

## The hot set: two books do not shed at all

Shedding is per book, so step 2 asks which books are worth exempting: the ones
a keystroke will land in again. An editor types into one book for minutes at a
time, and the second keystroke there should not pay for the first one's
decision.

```text
hot = the books whose text moved most recently, newest first, at most N (2)

fold done -> book in hot -> keep its rows whole
             else        -> pass.release each observation
             book that just fell out of hot ->
                            release every generation's rows it still holds
```

The set is ordered by *edits*, not by publications: `index_book` moves a book to
the front exactly when its ring takes a new checksum, so republishing an
untouched corpus reorders nothing. `with_hot_books(0)` is the behaviour before
there was a set, and every test that pins the shed-grain law asks for it.

Cold, an edited book's aggregate is gone and every chapter's rows are shed, so
the whole book walks again: `last_mapped` 16, `last_remapped` 15 for MRK. Hot,
the rows are whole, so only the chapter whose `ObservationKey` moved is absent:
`last_mapped` 1, `last_remapped` 0. That is the entire word rewalk of an edited
book, gone from every keystroke after the first (evidence.md, W1e step 2). It
costs N books' word rows — 281 KB for two books of `en_ulb` against the 24 B a
shed row leaves — counted in the hot tier (`pantry.md`).

`remove` takes a book out of the set without owing anything, because its rows go
with its ring at the next sweep. Releasing is never a correctness question — a
released row is walked again on demand — so a row two books share may be shed
for the cold one and re-walked for the hot one.

A book-grain pass's aggregate does not ride the ring: the sweep keeps it for a
book's CURRENT checksum only, dropping every older generation even while the
ring and the chapter tables behind it survive for `with_generations` more edits.
A `WordAggregate` holding thousands of rows costs the same to keep five
generations deep as one (evidence.md, 2026-09-04 "the ~58 MiB"). An undo inside
the ring therefore still re-maps and re-folds a book-grain pass's book; a
chapter-grain pass's undo stays free.

## The resident corpus totals and the kept verdicts

Judging words used to re-merge every book's rows into corpus totals on every
publication (4.7 ms of a 4.9 ms warm republication, evidence.md W1) and
re-decide every word in the corpus (about a third of a warm republication).
Steps 3 and 4 move a book at a time instead.

```text
tallied[BookId] = the RawChecksum this book contributes to the totals now

checksum unchanged -> nothing at all
checksum moved     -> pass.untally the old aggregate, pass.tally the new
book gone          -> pass.untally, and the id leaves the table
```

The old aggregate is still resident when it is subtracted: a checksum leaves the
ring in step 1 and leaves `aggregates` in step 7 of the same publication, and
the totals move between the two.

What makes it safe is that the tally is exactly a merge. `WordTotals` after any
sequence of adds and removes equals `WordTotals::merge` over the books resident
then — counts, dispersion, and which rows exist at all, so a word held only in
forced positions keeps its all-zero row exactly as a fresh merge does. Cold
`analyze` builds no tally; it calls `judge`, which merges its own.

The delta the judge re-decides is named, not guessed: `ChapterPass::moved_keys`
reports the tally keys of exactly the aggregates `untally` and `tally` move, so
a key whose counts did not change cannot be in it. The merge is
`debug_assert_eq!`ed against a whole `judge_words` on every publication a debug
build makes, which is the whole test suite and both churn oracles. The one
channel outside the scheme is `WordLength`, whose ceiling is the corpus's own
length distribution: it is off by default, and judged whole whenever it is on.

## The declared source

A `Role::Reference` book is not a target and never becomes one: it is mapped by
nobody, folded by nobody, holds no chapter table and no aggregate, and publishes
no section of its own. What it holds is one grapheme count per verse, and that
verse's word hashes only when the config in force at `update` would judge with
them (`pantry.md`).

Step 5 runs from the lengths both sides already retain:

```text
pass.length_config(config)         -> the settings, from the member that owns the lane
pass.verse_lengths(aggregate)      -> the target lane, per Target book
pantry.reference_lengths(id)       -> the source rows, per Reference book
pantry.reference_words(id)         -> its word sets, if it was asked to keep any
PairedBook::pair_with(target, ..)  -> one book's ratios, its presence rows AND
                                      its source-copy runs; cached together,
                                      facts dropped
judge_paired(books, project, ..)   -> codes 0, 3 and 4 over target spans
```

The source-copy lane is the one part of this step that reads text: on a pair
MISS with the lane on, the target's projection is rebuilt from the products it
already retains and its words are walked. A pair hit reads nothing, so an
unchanged republication with the lane on still walks no text.

A declared source registered while `source_copy` was OFF kept no word lane, so
it is skipped and counted: `last_wordless_references()` is how many such SOURCES
a publication met, each named once however many targets paired against it.
Nonzero means "re-send those references", never "no run was found".

Every Target pairs with the Reference of the same `BookKey` — the first, if a
caller registers two files under one key, since `books(Role::Reference)` is
canonically ordered and the choice has to be an order rather than a hash. A
Target with no Reference gets no ratios and no rows; that is the contract, not
an error, so a host may declare a source for part of a corpus.

Three properties follow, and each is a test:

- **a source change touches no target observation.** Registering, replacing, or
  removing a reference moves no target checksum, so nothing is re-mapped,
  re-folded, or re-located, and every target-only row is byte-identical either
  side of the swap. This is the charter's "source choice legitimately changes
  results without invalidating target-only observations", pinned in
  `equivalence.rs` and in the Expediter's own tests;
- **the length settings are the same judging config as every other knob.** They
  ride `JudgingConfig::lengths` and reach the step through
  `ChapterPass::length_config`, so `set_config` moves them and still maps
  nothing and folds nothing;
- **references ride `SnapshotId`.** Swapping the declared source changes what
  the publication is OF, not only what it says, so the reference table is hashed
  beside the target one under its own role byte. So does the judging config,
  through `ChapterPass::config_stamp`: a `FindingHandle` is a snapshot plus a
  row, and two publications under different settings name different rows.

The facts pairing returns — target-only and source-only keys, ambiguous
duplicates, partial overlaps — are dropped here. They are alignment structure,
and the two the presence rule reads it has already turned into rows
(`rules/presence-shear.md`); a host that wants the facts runs the cold
`analyze_paired`, and `sous-cli` prints them as per-book counts.
`LengthConfig::enabled`, `presence` and `source_copy` are independent, and
pairing runs while ANY of them is on.

What this buys is measured: a warm 66-book republication with the corpus
declared as its own source goes from 4.9 ms to 0.79 ms and a keystroke from
6.6 ms to 2.95 ms, so a declared source costs 53 µs warm and 382 µs on a
keystroke (evidence.md, 2026-09-07).

## Siting, whole book or chapter by chapter

A judged pattern has no coordinates; `pass.locate` gives it some by rescanning
one book's current text (`sous-chef/core/src/sites.md`). Step 6 replays cached
rows where it can, and for a hot book replays them per chapter: a keystroke
walks the chapter it landed in and rebases its neighbours.

Whether that split is real is the pass's own answer.
`ChapterPass::CHAPTER_SITES` is true only when `locate_book` plus
`locate_chapters` over every chapter equals `locate` row for row, and a tuple
says yes only when exactly one member does. In `Brigade` that member is `Words`,
whose walk restarts at every chapter; `Substrate` places its rows in
`locate_book` as before. So a keystroke runs the substrate's rescan whole and
the word walk for one chapter — `last_sited_chapters()` answers 1.

One walk per RUN of missing chapters, not per chapter: reading a firing set is
per book, and a keystroke leaves exactly one chapter missing anyway. Rows land
in chapter order, so the sequence is the one a whole-book `locate` would have
pushed.

## Mark and sweep, N generations deep

Every publication ends by sweeping: a chapter table survives only if some book's
ring names its checksum, and an observation only if some surviving table names
its key. A book's ring is its current `RawChecksum` plus the last
`with_generations(n)` before it, four by default; `remove` drops the ring, so
the book's tables go at the next publication.

Why keep any previous generation at all: an undo restores byte-identical chapter
text and therefore the identical `ObservationKey`, so an undo within `n` edits is
a table hit and maps nothing — unless the pass is book-grain, in which case its
aggregate does not ride the ring and the undo re-maps and re-folds regardless.

The sweep is skipped outright when no table was added and no ring aged since the
last one — a republication of an untouched corpus has nothing to free, and pays
nothing to learn it. With the aggregate cache in front of it, that whole
republication is 6.9 µs for a 66-book Bible (evidence.md).

## Why the buffers are equal

`sous_core::for_each_chapter` is the only place a `ChapterInput` is assembled,
and both `analyze` and the Expediter call it, so neither can build an input the
other would not. Fold and judge are provenance-blind — neither can tell a cached
observation or aggregate from a fresh one — `locate` is handed the same text and
chapter rows either way, and both publishers rebase through the same
`rebase_span`, so the two paths differ in what work they skip and in nothing
else. `galley/tests/equivalence.rs` pins that as bytes, not as a claim: a seeded
edit churn republishes after every step and compares against a cold `analyze` of
the same texts, over a synthetic corpus and over a whole Bible.

## Parallel map

`--features parallel` maps a book's missing chapters on rayon's global pool, and
changes nothing a publication says. The missing chapters are queued in chapter
order, `par_iter` keeps that order, and the observations are inserted from it
afterwards, so the chapter table and the buffer are the serial ones byte for
byte. Both settings drop a repeated chapter from the queue the same way, so
`last_mapped` is the same number too.

The map is the only parallel part, and only over the chapters a publication
actually misses. Galley adds no pool of its own, no scheduler, and no background
work: `publish` still returns when the last chapter is mapped. The queue owns a
copy of each missing chapter's projected text and verse rows, because
`for_each_chapter` lends its `ChapterInput` for the callback only; the serial
path stays inside that callback and copies nothing.

`the_parallel_map_publishes_the_serial_bytes` compares the two publications
inside one binary, and the equivalence gate runs under both settings.

It is off by default because it is measured, not assumed: for `HygieneBytes` the
map is about half a millisecond of a 5.6 ms cold whole-Bible publication and
allocates one `Vec` per chapter, so the parallel path runs 1.7× SLOWER —
work-stealing and allocator contention cost more than the map saves
(evidence.md). `Brigade` is that costlier pass, and it does pay for a cold open
— 33 ms serial against 24 ms parallel — while staying a wash on a keystroke, so
the feature is still opt-in and a host asks for it. The first thing to change
then is one `par_iter` over the whole corpus rather than one per book.

## The modules

```text
expediter.rs           the struct, the doors, index_book, publish
expediter/keys.rs      what makes two cached values interchangeable
expediter/siting.rs    a judged pattern gets coordinates exactly once
expediter/pairing.rs   a comparison is a pure function of both sides' rows
expediter/residency.rs nothing reachable that no live generation names
```

## The handle over it

`galley::wasm::Galley` IS an `Expediter<Brigade>` and nothing else, so the
publication a JS host reads is this one. `galley/src/wasm.md` states the
equivalence and its three tests.
