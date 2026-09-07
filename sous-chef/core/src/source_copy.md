# `sous_core::source_copy`

The other source-compared step, and the only one that reads text. What the rule
*claims* and refuses to claim is
[`../../rules/source-copy-residue.md`](../../rules/source-copy-residue.md);
this file is the shape of the computation.

## Where it runs

Inside the same `pair_keys` call the ratios and the presence rows come out of:

```rust
PairedBook::pair_with(book, target, source, Some((text, words)), &mut facts)
judge_paired(&books, &project, &config, &mut findings)
```

The target's projected text and the source's word sets are handed over
together or not at all. Handed over, the pairing walks the target unit's words;
not handed over, the lane is silent and no text is read.

## The two lanes a source retains

Per Reference verse, beside `SourceVerse { key, graphemes }`, the sorted
deduplicated 32-bit hashes of that verse's words:

```text
SourceWords { hashes: Box<[u32]>, spans: Box<[(u32, u32)]> }
   spans[i]  is verse i's window into hashes, index-aligned with SourceVerse
```

One flat lane per book, not one allocation per verse. The hash is the low 32
bits of xxh3-64 over the word's raw UTF-8 bytes — **not** case-folded, because
a paste preserves case and case-insensitive matching is a different claim. Word
boundaries are `words::walk`'s and nobody else's, so the two sides cannot
disagree about what a word is. Nothing else is retained: no positions, no
strings, no token tape.

A whole Bible costs 2.77 MB of it (evidence.md, U1 (a)), which is 61% of the
raw text it hashes and takes a declared source from 1.17 MB to 3.94 MB. It is
therefore OPT-IN: `galley`'s `SourceLanes` says whether a Reference derives it,
and a host judging without the lane keeps the 1.17 MB shape. A source that kept
no lane is skipped and counted, never silently judged as clean —
`Paired::wordless` names those target books (`galley/src/pantry.md`).

## The walk

Per paired unit, in target order:

1. slice the unit's target rows out of the projected text;
2. `for_each_word` over each slice, keeping run state ACROSS a bridge's
   constituents — a bridge is one unit;
3. hash each word the same way and binary-search the source unit's set (the
   union of its constituents' sets, for a bridge);
4. a hit extends the open run, a miss closes it.

A run of digits holding no letter is not a word to the walk at all, so a verse
number or a year neither lengthens a run nor breaks one, and it is not counted
as eligible either. It can still sit *inside* a row's span, because the span is
the bounding range from the run's first word to its last — a navigation
coordinate, exactly as a bridge's length row is, never a claim about every byte
between.

## What is cached and what is a knob

`PairedBook` caches every maximal run of at least `MIN_RUN` (2) words beside
the ratios, under the same (target checksum, source checksum) key.
`source_copy_min_run` is applied over those rows at judge time, so raising the
floor re-judges and does not re-pair.

The `source_copy` switch is the exception: it decides whether text is walked at
all, so the `Expediter` holds it beside the cache and clears the cache when it
flips. An unchanged republication with the lane on walks no text and replays
the cached runs (`an_unchanged_publish_walks_no_text_for_source_copy`).

## The row

`RuleCode::SourceCopy`, wire code 3, lane A the run in words and lane B the
unit's eligible target words, both `1..=i16::MAX` with `SATURATED` above.
Unlike the hygiene and presence lanes, `SATURATED` says *a* lane clamped and
not which, because two lanes share one bit — the proportionality row's rule.

## Configuration

`LengthConfig::source_copy`, **default false**, beside `presence`. The
measurement that set it: against a source in the same language family every
tier corpus fires about two rows per verse at floors of both 3 and 4 (71,081
rows over `examples.bsb`, 1,393 over one book of `en_ult-fixtures`), while a
genuinely unrelated pair fires none. The lane is a deliberate query, not a
default channel. The revert is one bool.
