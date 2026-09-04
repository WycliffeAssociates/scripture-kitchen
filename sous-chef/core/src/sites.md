# `sous_core::sites`

The one step that reads text to *place* a judgment. Judging is a corpus fact
with no coordinates ([`judge.md`](judge.md)); this module rescans each Target
book's current text for the patterns that fired *and* occur in it, and turns
each matching run into one `Convention` row.

It may only agree with the counts. Every occurrence the substrate walk counted
for a pattern's key is one this module finds, and
`tests/sites_agree_with_counts.rs` is that equality over a synthetic sweep and,
ignored, over every chapter of the 8-corpus tier.

## Two calls, and why they are separate

```text
firing(book counts, the corpus's pattern table, &mut set)
  → the table positions whose glyph THIS book holds
locate(text, chapters, set as (index, pattern) rows, &mut sites)
  → one Site per matching run, in chapter then offset order
```

`firing` is split out because it is the only part a host can act on without
text. `galley::sous::Expediter` hashes the firing set's *content* and caches a
book's rows under `(RawChecksum, FiringHash)`: unchanged text plus an unchanged
firing set is a replay, and a publication that renumbered the table because
some other book's counts moved rescans nothing. It is also the shape of "a book
without the glyph reads no text" — the counts answer, and the text is never
opened.

## The engine

**One `memmem::Finder` per distinct firing glyph, searched per chapter.**
`memchr` on a UTF-8 lead byte is rejected: every Devanagari scalar shares
`0xE0`, so "find the danda" verifies every character and runs 6× slower
(evidence.md, 2026-09-03). For a one-byte needle `memmem` *is* `memchr`, so
Latin loses nothing.

Aho-Corasick is not here. The row that rejected it said to revisit if distinct
needles per book are often ≥ 10; D2b measures **10–14 median, 10–19 p90** over
the tier, which is at that line — but the needles are the *common* glyphs
(comma, period, danda) where the same row measured AC losing by 2–6×, so the
decision stands and the number is recorded rather than acted on.

**The pooled digit key has no literal.** `ScalarKey::DIGITS` covers every
`Nd` scalar, so a pattern on it scans the chapter's scalars once with
`is_decimal_digit`. That is the only classifier pass here, and it runs only
when a digit pattern fired.

The search is per **chapter**, not per book, because that is also the clip a run
takes. Searching the whole book once per needle and looking a hit's chapter up
afterwards was measured and is *slower*: the lookup costs more than the
`find_iter` call it saves.

## The cursor is the pattern language

Three reads, all over the classifier and `OuterClass::of`:

| call | answer | `Edge` when |
| --- | --- | --- |
| `prev_outer(at)` | the outer class before the scalar at `at` | the book starts there |
| `next_outer(at)` | the outer class after it | the book ends there |
| `run_around(at)` | the maximal nonletter run holding it, clipped to its chapter | — empty when the scalar is not a run atom |

This is exactly `fold_book`'s seam behaviour, which is what makes the counts and
the rescan the same claim:

```text
chapter k  "… one,"       chapter k+1  "two …"
                    └── the pair reads ACROSS ──┘   prev/next skip the seam
     "a,,"   |   ",,b"                              a run does NOT: two runs
```

An **empty** chapter is not a neighbour, so the cursor walks past it exactly as
the fold's carry does. Glue answers `Letter` rather than its base's class, so
neither the walk nor the cursor ever steps back over a mark — a comma with an
acute accent after it reads `next = Letter`, and skipping the accent to find a
second comma would disagree with the `pairs` lane.

## One row per maximal run

A run is evaluated once, however many needles hit inside it. Every pattern of
every firing glyph the run contains is tested:

| channel | matches when | occurrences counted |
| --- | --- | --- |
| `Placement{side, class}` | an occurrence of `g` in the run sees `class` on `side` | those occurrences |
| `RunShape{pure, bucket}` | the run's own shape is that | one, the run |
| `ExactNeighbor(n)` | some occurrence of `g` is immediately followed by `n` | those positions |
| `Rarity` | `g` occurs in the run | those occurrences |

A glyph's neighbours *inside* a run are `Nonletter` by construction; only the
run's first and last members can see `Letter`, `Space`, `Digit`, or `Edge`.
And `Edge` never appears in a pattern at all — a book boundary is a fact about
the file, not a convention, so `judge::placement` skips it while still counting
it in the denominator.

If anything matched, the run is **one** `Site`: the span is the whole run
widened to atom edges inside its chapter, lane A is the *headline* pattern —
finest channel first (`ExactNeighbor` > `RunShape` > `Placement` > `Rarity`),
ties by lowest table position — and lane B is the union of every rung any
matched pattern belongs to, placement split before/after. So one span is drawn
once with every way it is anomalous, and the wire needs no `MULTI` flag.

A consequence worth knowing when reading `--findings` or `--report`: a site is
listed under its **headline** pattern only. A run that matched both a run shape
and a placement appears under the run shape, with `PlacementBefore` beside
`RunShape` in its reasons. The pattern table is still the evidence; the site
count under a row is not that row's numerator.

**Rarity for a letter** is the exception to "the site is a run": a letter — or a
rare whitespace scalar — is never a run atom, so its site is its own atom.
Placement, run shape, and exact neighbour never name one, because those lanes
count nonletters alone.

## Cost

`cargo bench -p sous-core -- sites_locate`, whole-corpus rescan over the tier:
**14.0 ms** en_ulb (66 books, 4.1 MB), 24.1 ms hin2017, 0.38 ms amh — about
210 µs per book against `substrate_map`'s 500 µs, which is the point: the
rescan sits well under the walk it is not allowed to repeat. Sites per book are
2/21/69 (median/p90/max) on en_ulb and 6/36/59 on nya, the widest corpus in the
tier. Full distributions: evidence.md, 2026-09-04.
