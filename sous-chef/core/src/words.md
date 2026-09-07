# `sous_core::words`

The Level 2 walk. What the counts are *for* is
[`../../rules/word-conventions.md`](../../rules/word-conventions.md); this file
is the word rule, the forced rule, the row's shape, and the two arguments
behind them.

## Layout

`mod.rs` holds `Form`, the row, the aggregate, and the `Words` pass. `walk.rs`
is the per-chapter scan; `fold.rs` merges chapter rows into a book;
`totals.rs` merges books into the corpus tally judging reads, and keeps it
current one book at a time; `tests.rs` covers all four through the public
surface. The casing channel itself lives beside the
other judges in [`judge.md`](judge.md).

## What a word is

```text
"He said. \u{201C}Go,\u{201D} said don't-3rd \u{5d0}\u{5d1}\u{5d2} a--b 12,345"
   He        Title    forced   the chapter's first word
   said      Lower    free
   Go        Title    forced   an opening quote, and a terminal before it
   said      Lower    free     the comma broke the chain
   don't-3rd Lower    free     two joiners, and a digit riding a letter
   \u{5d0}\u{5d1}\u{5d2}       Uncased  \u{2014}      dropped: no cased letter, no convention
   a, b      Lower    free     `--` is two atoms, so it joins nothing
   (nothing)                   a digit run holding no letter is not a word
```

A word is a **maximal run of letters and glue, extended through ONE nonletter
that has a letter immediately on both sides**. `ng'ombe`, `don't`, and
`mother-in-law` are one word each; `a--b` is two, because the second atom
confirms nothing. **A digit beside a letter joins the word** — `3rd`, `1Ki` —
since splitting there would invent a word that is in no text; **a run of digits
with no letter is not a word at all**, so `12,345` contributes nothing.

There is no dictionary and no UAX #29 at runtime. Eight corpora cannot justify
that, so `examples/word_breaks.rs` compares this rule against
`unicode_segmentation`'s word breaks over the 1,504-corpus fleet and records
the disagreement per script in [`../../evidence.md`](../../evidence.md). Scripts
written without spaces — Thai, Lao, Khmer, Burmese, Han, Kana, Tibetan — defeat
both rules without a dictionary, and the word lanes abstain there rather than
pretend.

## The case fold, and what it costs

The hash is xxh3-64 over the word's scalars, each replaced by the first scalar
of `char::to_lowercase`. That is a **simple fold**, so `\u{df}` stays itself
and `\u{130}` loses its dot: `STRASSE` and `Stra\u{df}e` are two words here.
For a convention check that is the right trade — a full fold is a collator's
job, and neither case ever decides a finding on its own.

Words fit a `u64` only 12-23% of the time outside Latin (evidence.md,
2026-09-02), so the key is the hash and not the bytes. A hash cannot give a
length back, so the row carries the scalar count in one byte beside it: mean
and standard deviation over a corpus come from that alone, and hapax rate is a
fold over the same rows. Neither judges anything yet.

## Forced, and free

A casing claim may only use positions where the *word* chose the capital. An
occurrence is **forced** when:

- it is the first word of the chapter, or of a verse (`ChapterInput::verses`);
- the nearest non-space text before it ends in a run whose LAST atom is
  `Pool::Terminal` (`pool_of`, the same pinned UCD properties the G2 pools use,
  so Ethiopic `\u{1362}` and the danda `\u{964}` force a capital without an ASCII
  allow-list);
- it follows an opening quote or bracket that itself follows one of those.

Everything else is **free**. The walk carries that as a two-state chain: a
terminal opens it, a quote or bracket rides through it, any other atom closes
it, and a word reads it once, when it begins.

The verse clause is an **abstention, not a discourse claim**. Charter invariant
1 says a verse start is an address rather than a sentence boundary and that
word and adjacency state *may* cross a verse seam — and here it does: the scan
keeps its chain, its open word, and its joiner across the seam. All the clause
does is drop evidence at a position where nearly every translation capitalizes
regardless, which removes a numerator rather than adding a claim.

## The row

One `WordRow` per chapter, one `WordCount` per distinct case-folded word,
sorted by hash. `size_of::<WordCount>()` is **24 B**:

| field | bytes | answers |
| --- | --- | --- |
| `hash: u64` | 8 | the key; there is no packed alternative outside Latin |
| `free: [u16; 4]` | 8 | Lower, Title, Upper, Mixed in free positions |
| `forced: u16` | 2 | positions the punctuation decided, kept out of every claim |
| `len: u8` | 1 | scalar count, for a later length or hapax fold |

Two shapes are deliberate:

- **A word with no cased letter is skipped entirely.** It cannot hold a casing
  convention, so `Form::Uncased` is counted nowhere and refused on the wire.
  The consequence is that an uncased chapter's row is empty and the scan hashes
  nothing at all: measured, Amharic pays 48 B a chapter against 5.3 KB for
  English, and its walk is *faster* than the substrate's (evidence.md,
  2026-09-04). "Uncased scripts pay nothing" is a measurement, not a slogan.
- **Counts saturate at `u16`.** A chapter is not where a word reaches 65,535
  occurrences, and the aggregate widens to `u32` immediately.

## The fold

`WordAggregate` is the book's rows merged by hash with `u32` counts, plus
`cased: bool` — whether the book holds a cased letter at all. **A word never
crosses a masked `\c`**: the chapter seam is an edge of text for a word exactly
as it is for a nonletter run, which is the ruling `substrate.md` already makes.
So there is no seam state here, no carry, and the fold is a plain merge whose
result cannot depend on the order rows arrive in — which is what makes a
cached row and a fresh one indistinguishable.

## The corpus tally

`WordTotals` is the fold one level up: every book's `WordAggregate` merged by
hash into one row per case-folded word, carrying the four free lanes, how many
books hold each lane, and how many hold the word at all. It is what
`Channel::Casing` actually reads.

```text
merge([GEN, MRK])   hash(david) free [2, 40, 0, 0]  books [1, 2, 0, 0]  holders 2
remove([old MRK])   hash(david) free [2, 38, 0, 0]  books [1, 1, 0, 0]  holders 1
add([new MRK])      hash(david) free [2, 39, 0, 0]  books [1, 2, 0, 0]  holders 2
```

The point of the two updates is that a resident host does not merge 66 books
again on every publication — 4.7 ms of a 4.9 ms warm republication before this
existed (evidence.md, W1). `add` and `remove` are one tandem walk over two
hash-sorted sequences, and the result is exactly `merge` over the books left:
`holders` is why, since a word held only in forced positions has an all-zero
row that a fresh merge holds too, and only its last book leaving takes it away.

`Words` is therefore retained at BOOK grain — `RETAIN_CHAPTERS = false`, and
`release` puts the chapter's row back to its 24-byte default, flagged
`released`, once the fold has read it. The flag is what `is_released` reads:
an uncased chapter's row is empty too, and it is whole. A host re-walks a whole
edited book instead of one chapter and keeps 2-3.4 MB per Bible instead of
6.4-8.2 (evidence.md, "W1 grain"); only this member re-walks, because the
tuple's `remap` leaves its neighbours' retained rows alone. The seam is
`galley/src/sous/expediter.md`.

## Judging, and placing

`Words::judge` merges the tally and calls the casing channel
([`judge.md`](judge.md)); `Words::judge_resident` calls the same channel over a
tally a host already holds, which is the only difference between the two. A
corpus whose aggregates are all `cased == false` emits nothing and hashes
nothing.
`Words::firing` names the table positions whose `(hash, form)` this book's own
counts hold, so a book without the word reads no text. `Words::locate` rewalks
the book with the same scan and sites every free occurrence whose hash and form
a firing row named, one `Convention` row per word span with
`Reasons::CASING`.

The rescan is the same walk, so it must agree with the counts exactly:
`tests/casing_agree_with_counts.rs` is that equality over a synthetic sweep
and, ignored, over every chapter of the 8-corpus tier. It is why `locate`
takes the book's verse rows — the map read them to decide which positions were
free, and a rescan that could not read them would place occurrences it never
counted.
