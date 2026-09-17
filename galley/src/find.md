# Find

Literal search over a book's projection, with every hit placed back in the
source. Galley's, not the engine's: Onion already contributes everything an
engine can — the mask and its two-way offset map — so Find is `memmem` over
`mask.text()`, `to_source` per hit, and replacement as ordinary edits. Ruled
2026-08-24, `planning/ideas/candidates/find-and-overlay.md`.

```text
source   \v 1 Jesus wept.\f + \ft why\f* Then…
mask     ·····Jesus wept.················ Then…      · = dropped

Find::literal("wept. Then")   → Hit { projected: 6..16,
                                      source: Split([11..16, 31..36]) }
Find::literal("why")          → nothing
```

## The two coordinate spaces, and why `Split` exists

`Hit.projected` is in the mask's byte space — what a second search, a
highlighter, or any projected-coordinate consumer wants. `Hit.source` is in
the raw document's, which is where an edit has to land.

They are not the same interval, and the difference is not a rounding error. A
hit that crosses a masked gap covers dropped bytes in source space that it
does not cover in projected space, so `SourceSpan` has two shapes:

- `Contiguous(a..b)` — the hit never left one kept range.
- `Split([a..b, c..d, …])` — one range per contiguous source piece, in order.

The offset map knows the markup is there, so nothing here hands back a single
`a..d` that would swallow a footnote. `source.start()` and `source.end()`
bound the hit, and the bytes between two pieces are exactly the markup the
projection dropped. `SourceSpan::pieces()` walks them; a caller that concatenates
them gets the needle back, which is what the tier test asserts.

Mask ranges are MAXIMAL — adjacent survivors are one range — so a seam between
two pieces is always a real gap and never an artifact of how the mask was
built.

## What the search does

Leftmost-first and non-overlapping: an accepted hit advances the cursor past
itself. A hit REJECTED by `whole_word` advances one byte instead, so a legal
hit can never be hidden by an overlapping illegal one. An empty needle matches
nothing rather than matching everywhere.

`in_projection(&mask, source)` is the primitive; `in_book(&entry)` is that over
a Pantry book's retained mask and text, and `in_pantry(&mut pantry, role)` runs
it over every registered book in canonical order, leaving out books with no
hit. The projection is materialized per search and dropped with the iterator —
the Pantry retains the mask, not the projected text.

**Either role is searchable, on one condition: it kept its text.** A target
always does. A reference does only under `Retain::Text` (`keepText` at the wasm
door), which buys it the same mask and UTF-16 table a target gets — text and
projection travel together, so a book has both halves or neither
(`pantry.md`). A reference registered lengths-only has nothing to search and
is left out rather than reported clean.

Literal only. The `regex` crate becomes a galley dependency the day a consumer
asks for one, and not before; `aho-corasick` waits for a genuine many-patterns
consumer (termlist highlighting).

## Whole word: a restatement, meant to agree

`whole_word(true)` uses the same rule as `sous_core::words` — **a maximal run
of letters and glue, extended through ONE nonletter that has a letter
immediately on both sides**. `mother-in-law` and `don't` are one word each;
`a--b` is two. A digit beside a letter joins the word (`3rd`), and a run of
digits holding no letter is no word at all.

The rule is RESTATED here, not called: Find is an Onion-side galley feature and
must not become a Sous one. Only the boundary question is asked, so the
restatement is a dozen lines — for each edge, whether the scalar outside it
continues the run, and whether an outside nonletter is a confirmed joiner
rather than a break.

The two are **meant to agree**, and `galley/tests/find.rs` asserts they do:
over the whole test tier it compares every whole-word, case-insensitive hit of
`the` against every `sous_core::words::for_each_word` occurrence that folds to
`the`, book by book, and requires the same spans in the same order. Galley is
the one crate where both rules are in scope, so it is the one place that
agreement can be asserted rather than asserted about.

**Where the classifier comes from.** The rule needs Unicode bits — Alphabetic,
the Mark/Extender pair the charter calls *glue*, and Nd. Onion has no
classifier of any kind, so the bits come from `mise::unicode::class_of` (pinned
UCD 17.0.0), the workspace's only one, in the leaf both engines share. Find
reaches for no Sous type at all: sous-core keeps the pools and the atom rule
that only judging means, and the bits sit below both. `char::is_alphabetic`
alone would not do:
it says nothing about combining marks, so a doubly-pointed Hebrew word or a
multi-virama Devanagari one would break where Sous joins, and the tier test is
what would catch it.

## Case-insensitive: the fold, and the measurement that chose it

`case_insensitive(true)` folds both sides with the **simple** lowercase fold —
the first scalar of `char::to_lowercase`, one scalar per scalar — which is
exactly how `sous_core::words` folds. `ß` stays itself and `İ` loses its dot.
A convention check, not a collator.

The fold changes byte offsets (`İ` is two bytes and folds to one), so the
folded copy carries an offset map back to the projection. It is sparse: one
`(folded, projected)` mark only where the running difference MOVES, plus a
terminal mark when the text ends on a difference no scalar announced. ASCII
text records nothing and the lookup is a length check.

The two candidate designs were measured once, over en_ulb's 4.1 MB verse-text
projection, on this machine (2026-09-06):

| design | `melchizedek` | `the` |
| --- | --- | --- |
| case-sensitive baseline (no fold) | 0.35 ms | 2.58 ms |
| folded copy + offset map (shipped) | 5.81 ms | 8.26 ms |
| fold per scalar, no copy (lower bound) | 5.55 ms | 7.82 ms |

Fold-per-scalar is 5% ahead on the rare needle and 5% on the common one — and
that row is a LOWER BOUND: it counts naive matches and builds no hits, does no
whole-word test, and places no spans, so the shipped version of it would be
slower still. Inside noise of each other, the folded copy wins on everything
else: it keeps `memmem`, so a long needle stays sublinear and no needle can go
quadratic, and it is a linear pass a future SIMD fold can subtract from. Both
designs use an ASCII fast arm (`A..Z` lower to `a..z` and nothing else moves),
which is worth ~20% of the fold on Latin scripture and nothing on Devanagari.

The remaining gap to the baseline is the fold pass itself, ~750 MB/s of scalar
loop. A byte-level fold that decodes only non-ASCII leads is the next available
subtraction; it is recorded here rather than built, because no consumer has
asked for case-insensitive whole-Bible search yet.

## Measured

`galley/benches/find.rs`, `cargo bench -p usfm_galley --bench find`, 2026-09-07,
M-series laptop, release + thin LTO:

```text
find               fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ en_ulb                        │               │               │               │         │
│  ├─ Melchizedek  2.185 ms      │ 2.538 ms      │ 2.252 ms      │ 2.275 ms      │ 100     │ 100
│  │               2.062 GB/s    │ 1.775 GB/s    │ 2 GB/s        │ 1.98 GB/s     │         │
│  ╰─ the          4.968 ms      │ 5.613 ms      │ 5.138 ms      │ 5.168 ms      │ 100     │ 100
│                  906.9 MB/s    │ 802.7 MB/s    │ 876.8 MB/s    │ 871.8 MB/s    │         │
╰─ hin2017         13.06 ms      │ 20.15 ms      │ 13.46 ms      │ 13.67 ms      │ 100     │ 100
                   751.4 MB/s    │ 487 MB/s      │ 729.1 MB/s    │ 717.8 MB/s    │         │
```

Whole-Bible, case-sensitive, median per row: `Melchizedek` **2.25 ms** and
`the` **5.14 ms** over en_ulb's 66 books (4.51 MB of USFM, 11 and 76,324 hits);
`और` **13.46 ms** over hin2017 (9.82 MB, 35,816 hits).

Three things the rows say:

- **The projection copy is the floor.** `Melchizedek` matches 11 times in a
  whole Bible, so almost all of its 2.25 ms is `mask.text()` per book — about
  4 MB of copying. A retained or borrowed-per-range projection is where that
  goes if a consumer needs it; the Pantry retains the mask, not the text of the
  view.
- **The common needle costs its hits, not its bytes.** `the` adds 2.9 ms for
  76,324 hits — ~38 ns each, which is the `partition_point` into the mask plus
  the `Hit`. Building a `Vec` per hit instead of returning `Contiguous`
  directly cost 1.7 ms of that before it was removed.
- **A Devanagari needle has no rare byte to prefilter on.** `और` is
  `E0 A4 94 E0 A4 B0`, and `E0 A4` leads nearly every scalar in the corpus, so
  memmem's prefilter fires constantly and the scan runs at ~730 MB/s against
  the ~2 GB/s a rare Latin needle sees. This is the same fact
  `sous-chef/core/src/sites.md` records about memmem versus memchr on a UTF-8 lead
  byte; it is a property of the script, not of this code.

`hin2017` is a vref-style plain text corpus (`GEN 1:1\ttext`) — there is no
Hindi USFM in the repo — so its row searches the file under an IDENTITY mask,
one range over the whole text. It measures scan and placement over a non-Latin
script; it does not measure masking.

## Crossing to a host

`find::wire::encode` is the same hits as one little-endian `u32` buffer, in
UTF-16, with the ids and a projected preview per hit — the shape a host outside
Rust reads. It lives beside Find rather than in `wasm.rs` because BOTH of
Sefer's doors encode it: the browser through `Galley::find`/`findAll`, the
desktop through `Expediter::find` linked natively. One encoder, so the two doors
cannot drift. `wasm.md` states the layout for the reader on the other side.

The buffer leads with two words, as the onion and sous buffers do:

```text
u32   magic     0x444E4946 — "FIND", the four bytes in order
u32   version   1
u32   hitCount
u32   bookCount
…
```

`find::wire::MAGIC` and `VERSION` are the constants. A reader a version behind
fails on the header rather than on a field it misread, which is the whole
reason the two words cost eight bytes per answer.

Which books are searched is the CALLER's list: `Expediter::find` takes ids, and
the buffer's id table names exactly those, in that order. The wasm door turns a
scope string into that list (`wasm.md`); nothing inside Find knows what a scope
is.

The preview is in the buffer because the projection is materialized per search
and dropped with the iterator; `Hits::projection()` is the borrow that lets the
encoder read the offsets and the snippet off the copy the search already made.

## Replacement is the caller's

Not in this slice, and it does not need to be: a hit is already source ranges,
and source ranges are what `onion::Edit` takes.

```rust
// A Contiguous hit, replaced, then applied with onion::edit::apply.
let SourceSpan::Contiguous(at) = &hit.source else { .. };
edits.push(Edit { from: at.start, to: at.end, insert: FixStr::new(b"Melchizedek") });
```

`FixStr` holds 15 ASCII bytes; longer or non-ASCII replacement text is several
`Edit`s sharing one `from`, which `onion::edit::apply` concatenates in list
order. Sort by `from` and keep them non-overlapping — `check_edits` re-checks
both.

A `Split` hit is a decision the caller has to make and Find must not make for
it: the markup between the pieces either survives (delete each piece, insert
beside one of them) or does not (one edit over `start()..end()`). Find's job is
to have told the truth about where the gap was.

## The wire is declared, not typed

The buffer `find` and `findAll` answer with is declared once, in
`galley/src/find/wire/schema.rs`: its Rust writer (`find/wire/generated.rs`)
and its TypeScript reader (`galley/find-reader.ts`) are both generated from
that file, and `galley/tests/codegen_output_matches_input.rs` fails the build
if either is stale. The rows come from `ticket`, the vocabulary and emitters
every declaration in the workspace shares; the envelope — four header words,
two length arrays and two blobs — stays hand-written in `find/wire/mod.rs`,
because that framing is what this format IS.

A hit is the one record in the workspace with a TAIL: its source pieces are a
run whose length the row itself carries (`pieceCount`), which is the shape a
hit crossing a masked gap needs. The migration onto the generated writer moved
no byte — `VERSION` is still 1, and `the_generated_writer_reproduces_v1_byte_for_byte`
is what says so, over a split hit, a limited two-book search, an unregistered
id beside a book with no hits, and no books at all.
