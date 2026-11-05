# `sous_core::presence`

The other half of the Level 3 source-compared lane. What the rule *claims* and
what it refuses to claim is
[`../../rules/presence-shear.md`](../../rules/presence-shear.md); this file is
the shape of the computation. It reads no text and adds no walk: the facts are
already in the pairing [`proportionality.md`](proportionality.md) runs.

## Where it runs

Beside `judge_lengths`, from the same `pair_keys` call:

```rust
PairedBook::pair(book, target, source, &mut facts)   // ratios AND presence rows
judge_paired(&books, &project, &config, &mut findings)
```

`PairedBook` caches the rows next to its ratios, keyed by both checksums, so a
resident host that hits the pair cache pays nothing for presence at all. The
rows are a pure function of the two key lists, exactly as the ratios are.

## From facts to rows

`pair_keys` reports what it could not pair. Three of the four shapes matter:

| the pairing says | the row |
| --- | --- |
| `SourceOnly { key }` | `Missing` |
| `TargetOnly { key }` | `Extra` |
| a unit whose target counts 0 graphemes and whose source counts more | `Empty` |
| `AmbiguousDuplicate`, `PartialOverlap` | nothing — the fact stands alone |

A target book with no source book of its key is skipped whole, which is the
contract and not an error, and a source book with no target book is never
visited: presence is per PAIRED book. That is what keeps a 27-book New
Testament judged against a whole Bible at eighteen rows rather than eighteen
thousand.

## Across two numberings

When the target and its source follow different versification schemes (a
`ulb` translation of an `org`-numbered Hebrew text, say), the host passes a
`Crosswalk` (`PairedBook::pair_across`) and the pairing is
`pair_keys_across` instead of `pair_keys`:

| the two sides | the unit |
| --- | --- |
| target PSA 3:1 (eng), source PSA 3:2 (org) | one unit: renumbered, paired |
| target PSA 13:5 and 13:6, source PSA 13:6 | one unit, two target ranges (a merge) |
| target PSA 89:6 (rsc), source PSA 90:5 and 90:6 | one unit, two source ranges (a split) |
| source PSA 3:1 (org's title verse), target a `\d` title | no unit and no fact: a title is not a verse row |

Units are the connected groups of rows across the crosswalk, which is the
same unit contract bridges already use, so lengths, presence and source copy
read them unchanged. Two differences from `pair_keys`, both deliberate:

- A `SourceOnly` key is in the **target's** numbering: presence places
  "missing" among the target's own rows, and it is what the translator reads.
  A source verse whose counterpart is a title, or nothing (the target's
  scheme has no such verse), is no missing verse and is dropped.
- A key repeated on one side can't be placed across numberings, so every copy
  is blocked as `AmbiguousDuplicate` rather than zipped in order.

With no crosswalk (no declaration, or both sides on one scheme) it is
`pair_keys`, unchanged. sous-core knows no scheme: galley builds the crosswalk
from canon and keys its pairing cache on it.

## Coalescing

Keys of one kind, in one chapter, whose numbers run consecutively become ONE
row, and lane B is how many. A bridge counts as one key, so the count is
"keys covered", not "verses". `3`, `4`, `5` is one row of three; `3`, `5` is
two rows of one, because a paired key sits between them and the run is broken.

The revert, if a consumer would rather have one row per key: drop the
`coalesce` call and push each keyed entry straight into `out`. Lane B then
reads 1 on every row and nothing else changes.

## Spans

A row's span is a navigation coordinate, never a claim about the bytes inside
it.

- **Extra** and **Empty** bound the target verses they cover, first byte
  through last — the same bounding rule a bridge's length row uses.
- **Missing** is ZERO-LENGTH at the insertion point, because the target holds
  no bytes for a key it does not have: the end of the last target verse of that
  chapter preceding the absent key, or the start of the chapter's first verse
  when nothing precedes it, or the end of the target book when the whole
  chapter is absent.

## The row

`RuleCode::Presence`, wire code 4, lane A the `PresenceKind` discriminant and
lane B the key count, `1..=i16::MAX` with `SATURATED` above it — the hygiene
lane's shape, for the same reason. A zero count is unconstructible on the way
in and refused on the way out.

## Configuration

`LengthConfig::presence`, default true, beside `enabled`. The three lanes are
independent: presence with the ratio lane off is a legal configuration and the
`Expediter` still pairs for it. All three off is the only state that skips
pairing entirely ([`source_copy.md`](source_copy.md) is the third).
