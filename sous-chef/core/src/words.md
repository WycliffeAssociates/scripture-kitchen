# `sous_core::words`

The Level 2 walk. What the counts are *for* is
[`../../rules/word-conventions.md`](../../rules/word-conventions.md); this file
is the word rule, what the row records instead of a verdict, the row's shape,
and the two arguments behind them.

## Layout

`mod.rs` holds `Form`, the row, the aggregate, and the `Words` pass. `walk.rs`
is the per-chapter scan; `fold.rs` merges chapter rows into a book;
`totals.rs` merges books into the corpus tally judging reads, and keeps it
current one book at a time; `tests.rs` covers all four through the public
surface. The two word channels themselves live beside the other judges in
[`judge.md`](judge.md), and so does the terminal table they read.

## What a word is

```text
"He said. \u{201C}Go,\u{201D} said don't-3rd \u{5d0}\u{5d1}\u{5d2} a--b 12,345"
   He        Title    Start        the chapter's first word
   said      Lower    None
   Go        Title    Glyph('.')   the quote is transparent; the stop is not
   said      Lower    Glyph(',')
   don't-3rd Lower    None         two joiners, and a digit riding a letter
   \u{5d0}\u{5d1}\u{5d2}       Uncased  \u{2014}            dropped: no cased letter, no convention
   a, b      Lower    None, Glyph(',')   `--` is two atoms, so it joins nothing
   (nothing)                      a digit run holding no letter is not a word
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

## What stood before, and who decides

A casing claim may only use positions where the *word* chose the capital. The
walk does not decide that. It records what stood in front of each occurrence —

```text
Before::None        a word, or space and then a word
Before::Start       the first word of the chapter, or of a verse
Before::Glyph(g)    the last atom of the nonletter run in front of it
```

— and the judge asks the corpus's own **terminal table** what each glyph does.
Quotes and brackets are transparent in the chain, so `He said. \u{201C}Go` records
`Glyph('.')` and `he said, \u{201C}Stop` records `Glyph(',')`: what the capital
answers to is the mark behind the quote. Whitespace is transparent too, and a
word closes the chain behind it.

The table is `TerminalTable`, learned in [`judge.md`](judge.md) from the
substrate's `follows` lane: for each glyph, `upper / (upper + lower)` of the
letters it hands off to across the corpus. A glyph **forces** when that share
reaches `JudgingConfig::terminal_upper_share_bp` (8,000 = 80%) on at least
`support_floor` cased handoffs. `Start` always forces; `None` never does;
everything else is the corpus's answer.

This is why there is no punctuation allow-list and no rule per script. Over
the committed tier the tables learned are:

```text
WA-en-ulb   '!' '"' '.' ':' '?'
francl      '!' '*' '.' '?' '«' '»' '“' '”'
grcsr       '.' ';'                      // the Greek question mark
spaRV1909   '.' '?' '¡'                  // the inverted opener, not '¿'
swhulb      '!' '.' '?' '‘' '“' '”'
amh, hin2017  (nothing forces — the script is uncased)
```

and the `he said, \u{201C}Stop` case answers itself: **en_ulb's comma is not in
that table** — it hands off a capital 4,836 times in 47,291, which is 1,022 bp
against an 8,000 bp bar. So `Stop` is a free position there and stays reviewable,
and a corpus that does report speech after a comma 80% of the time gets the
abstention instead. The v1 comma dial and the "an opening quote opens the
chain" proposal are both replaced by that one measurement.

The table's own key is the run's LAST atom, so a quote can appear in it (`"`
in en_ulb, after `."`). No `Before` ever names one, because the chain rides
through quotes; those rows are learned and never read.

The verse clause is an **abstention, not a discourse claim**. Charter invariant
1 says a verse start is an address rather than a sentence boundary and that
word and adjacency state *may* cross a verse seam — and here it does: the scan
keeps its chain, its open word, and its joiner across the seam. All the clause
does is drop evidence at a position where nearly every translation capitalizes
regardless, which removes a numerator rather than adding a claim.

## The row

One `WordRow` per chapter, one `WordCount` per distinct
`(case-folded word, Before)`, sorted by that pair. `size_of::<WordCount>()` is
still **24 B**:

| field | bytes | answers |
| --- | --- | --- |
| `hash: u64` | 8 | the key; there is no packed alternative outside Latin |
| `counts: [u16; 4]` | 8 | Lower, Title, Upper, Mixed under this `Before` |
| `before: u32` | 4 | the packed `Before` |
| `len: u8` | 1 | scalar count, for the length channel and a hapax fold |

`Before` packs into four bytes because a `ScalarKey` is a code point and two
sentinels one past the last one spell `None` and `Start`. That is what keeps
the row at 24 B and makes `(hash, before.raw())` the sort key.

Widening the key costs rows, not bytes per row, and only at book grain: over
the committed tier a cased Bible's merged aggregate goes from 2.70-4.47 MB
keyed by hash alone to 3.35-5.25 MB keyed by the pair, **+17% to +24%**
(evidence.md, W3). Chapter grain barely moves — a chapter's vocabulary mostly
appears under one `Before` anyway.

Two shapes are deliberate:

- **A word with no cased letter is skipped entirely.** It cannot hold a casing
  convention, so `Form::Uncased` is counted nowhere and refused on the wire.
  The consequence is that an uncased chapter's row is empty and the scan hashes
  nothing at all: measured, Amharic pays 48 B a chapter against 5.3 KB for
  English, and its walk is *faster* than the substrate's (evidence.md,
  2026-09-04). "Uncased scripts pay nothing" is a measurement, not a slogan.
  The price is that `Channel::WordLength` abstains there too: a long uncased
  word is in no row to judge.
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
`(hash, before)` into one row per pair, carrying the four form lanes and how
many books hold the pair at all. `WordTotals::by_word` hands the judge one
word's rows at a time, and the judge sums whichever `Before`s the table left
free.

```text
merge([GEN, MRK])   hash(david) None [2, 40, 0, 0]  holders 2
remove([old MRK])   hash(david) None [2, 38, 0, 0]  holders 1
add([new MRK])      hash(david) None [2, 39, 0, 0]  holders 2
```

The before dimension has to survive into the tally: which positions are free
is a *judging* decision, and a host must be able to move
`terminal_upper_share_bp` and re-judge without re-walking a chapter. For the
same reason dispersion is no longer carried here — a row's `books` is counted
at judge time from the aggregates, because it counts books holding part of a
numerator whose shape the config decides.

The point of the two updates is that a resident host does not merge 66 books
again on every publication — 4.7 ms of a 4.9 ms warm republication before this
existed (evidence.md, W1). `add` and `remove` are one tandem walk over two
hash-sorted sequences, and the result is exactly `merge` over the books left:
`holders` is why, since a row whose lanes the table later reads as forced is
still a row a fresh merge holds, and only its last book leaving takes it away.

`Words` is therefore retained at BOOK grain — `RETAIN_CHAPTERS = false`, and
`release` puts the chapter's row back to its 24-byte default, flagged
`released`, once the fold has read it. The flag is what `is_released` reads:
an uncased chapter's row is empty too, and it is whole. A host re-walks a whole
edited book instead of one chapter and keeps 3.4-5.3 MB per Bible instead of
7.5-9.6 (evidence.md, W1 grain and W3); only this member re-walks, because the
tuple's `remap` leaves its neighbours' retained rows alone. A host is free to
exempt a few books it expects the next keystroke in — `galley::sous::Expediter`
exempts two, and pays `observation_bytes` for them — which buys back the
one-chapter walk for the book being typed in without buying the whole Bible.
The seam is `galley/src/sous/expediter.md`.

## Judging, and placing

`Words::judge` merges the tally and calls the word channels
([`judge.md`](judge.md)); `Words::judge_resident` calls the same channels over
a tally a host already holds, which is the only difference between the two. A
corpus whose aggregates are all `cased == false` emits nothing and hashes
nothing.

Both read the terminal table out of the sink, where `Substrate::judge` put it.
That is a real order: `Brigade` judges the substrate first, and **`Words`
alone abstains** rather than invent a forced rule of its own — pinned by
`words_alone_abstain_because_nothing_published_a_terminal_table`. The table is
corpus evidence the substrate already holds, and merging its follow lane a
second time inside the word pass would be the same numbers computed twice.

`Words::firing` names the table positions whose word this book's own counts
hold, so a book without the word reads no text. It is deliberately
**position-blind**: it asks whether the book holds the `(hash, form)` at all,
not whether it holds it free. A superset costs a rescan that finds nothing;
reading the terminal table there would put a judging decision inside a site
cache key. `Words::locate` rewalks the book with the same scan, reads the same
table, and sites what the counts named — a casing row's free occurrences, a
length row's every occurrence — one `Convention` row per word span, carrying
`Reasons::CASING`, `Reasons::WORD_LENGTH`, or both when one word fires both.

The rescan is the same walk over the same table, so it must agree with the
counts exactly: `tests/casing_agree_with_counts.rs` is that equality over a
synthetic sweep and, ignored, over every chapter of the 8-corpus tier, one
channel at a time. It is why `locate` takes the book's verse rows — the map
read them to record `Before::Start`, and a rescan that could not read them
would place occurrences it never counted.
