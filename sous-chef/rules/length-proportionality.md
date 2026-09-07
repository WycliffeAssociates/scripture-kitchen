# Level 3 — length proportionality

Source-compared lane. Consumes the aligned-unit contract in
[../charter.md](../charter.md). Silent, or a typed refusal, when a trustworthy
pairing is not available; it never pairs by incidental array position.

Status: landed (S1). `sous_core::proportionality` is the implementation and
[../core/src/proportionality.md](../core/src/proportionality.md) its shape;
`Pantry::Role::Reference` is how a host declares the source. Wire code `0`, two
signed Q8.8 lanes — see
[../core/src/codec/README.md](../core/src/codec/README.md).

## Observation

For every nonempty paired verse unit, retain the target/source grapheme-length
ratio and its projected target span. Judge the ratio against two distributions:

- ratios in the same book; and
- ratios in the whole paired project, so a short book can still be judged.

## Pairing

Absent or empty counterparts produce no ratio. Exact duplicate keys pair by
occurrence ordinal only when unambiguous. A bridge pairs directly with the same
bridge, or with the exact contiguous set of constituent verses on the other
side after those constituent texts are coalesced. The bridge contributes one
ratio over the two range totals; it is not divided into guessed per-verse
lengths or repeated in the distribution. Any partial overlap abstains.

## Judgment

The v1 donor uses a median with separate above/below median absolute
deviations, falling back to pooled MAD when one side has fewer than three
strict deviations. This remains the algorithm because the short side is
bounded by zero while the long side is open-ended.

V2 retains the calibrated defaults — `z_long = 3.5`, `z_short = 3.5`,
`min_verses = 50` — and reproduces the paired survey, pairing semantics, and
seeded-fault behavior as regression evidence rather than reopening the defaults
without contrary evidence. The paired-survey row is in
[../evidence.md](../evidence.md).

The port keeps v1's two constants with it: the MAD-to-sigma scale `0.6745`, so
the knobs read in familiar z units, and the per-side data floor of three strict
deviations below which a side borrows the pooled symmetric MAD. Both are
`sous_core::proportionality`'s own; neither is configurable, because neither is
a sensitivity control.

## Claim

Only: "this verse's length is unusual relative to this declared source and the
surrounding paired verses." It cannot establish an omission, mistranslation, or
wrong language.

## Known limits, part of the rule

- empty target/source units have no ratio and need a separate presence check.
  A verse with no content is rarer than it looks: an Onion projection keeps the
  newline the mask retained, so "empty" means the projected span is empty, not
  that the verse reads as blank;
- 10-20% truncations were essentially undetectable in v1's seeded survey, and
  are in v2's: 2.6% of seeded 10% and 20% chops fire, against a 2.4% background
  rate — i.e. nothing. A 50% chop fires 49% of the time
  ([../evidence.md](../evidence.md), 2026-09-07);
- source-language paste can have an ordinary length;
- results legitimately change with the chosen source: the same target against
  two tier sources shares 36% of its rows;
- adjacent opposite extreme ratios may indicate versification shear, which is
  [a separate observation](presence-shear.md), not a length finding.
