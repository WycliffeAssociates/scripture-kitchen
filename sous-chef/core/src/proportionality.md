# `sous_core::proportionality`

The Level 3 source-compared lane. What the rule *claims* and what it refuses to
claim is [`../../rules/length-proportionality.md`](../../rules/length-proportionality.md);
this file is the shape of the computation and the arguments behind it. It
consumes the charter's aligned-unit contract and nothing else.

## Not a `ChapterPass`

Every other rule here is one: a chapter maps to a detached observation, a book
folds, a corpus judges. This one cannot be, because a chapter observation is a
pure function of *that chapter's declared local inputs* (charter invariant 4)
and a ratio is a function of two corpora. So it is a corpus-level step a host
runs after the chapter passes, from lengths both sides already retain:

```rust
judge_lengths(&target, &source, &config, &mut findings) -> Paired
```

`analyze_paired` runs it for a cold caller, `Expediter::publish` for a resident
one, in the same place and the same order — after `judge`, before `locate` —
so the two publish identical bytes (`galley/tests/equivalence.rs`).

It reads NO text. Both sides are already counted:

| side | where the count comes from | retained by |
| --- | --- | --- |
| target | the substrate walk's `verses` lane, folded into book coordinates | the host's chapter/aggregate cache |
| source | `source_lengths(&book)` over any `ProjectedBook`, once per update | `Pantry` under `Role::Reference` |

The target lane carries a span because a target publishes coordinates; the
source rows carry a key and a `u32` and nothing else, because a reference never
publishes one. That asymmetry is the whole reason `Role::Reference` costs a
quarter of what a `Role::Target` does (`galley/src/pantry.md`).

## Pairing

One law, in `alignment.rs`, over `VerseKey` slices — `pair_keys`. The
text-carrying `align` and this length-carrying caller both call it, so they
cannot drift:

- exact key plus occurrence ordinal, when the two sides hold the same number of
  rows under that key; unequal counts abstain as `AmbiguousDuplicate` rather
  than letting a shorter prefix pair silently;
- a bridge pairs with the same bridge, or with the exact contiguous constituent
  run on the other side, **coalesced to one ratio over the two range totals** —
  never divided into a guessed per-verse split and never repeated in the
  distribution;
- any partial overlap abstains;
- a key absent on either side, and an empty unit on either side, produce no
  ratio at all.

A target book with no source book of its `BookKey` is skipped whole: no
ratios, no rows, and no facts either. That is the contract, not an error — the
source corpus is optional and whole.

`pair_keys` hands each pair to a callback as positions into the two slices
rather than as owned index lists. An ordinary verse pairs one row with one row
and a whole Bible is 31k of them; allocating two vectors per unit was 2 ms of
every publication (evidence.md, 2026-09-07).

## Judgment

v1's algorithm, ported whole, because the short side of a length ratio is
bounded by zero and the long side is open-ended:

```text
median   over the whole sample
MAD⁺     median of (x − median) over x > median
MAD⁻     median of (median − x) over x < median
MAD      median of |x − median| over every point         ← the pooled fallback
z        0.6745 · (ratio − median) / MAD(side the ratio fell on)
```

Two gates, in order:

1. **the whole unit** — a scope with fewer than `min_verses` ratios does not
   judge at all, and neither does an empty one whatever the floor;
2. **each side** — a side uses its own one-sided MAD only with at least three
   strict deviations AND a nonzero MAD; otherwise it falls back to the pooled
   symmetric MAD. Below three, a side's MAD is measured from the very points it
   would judge: at one deviation the "median" is that deviation, pinning its z
   at exactly 0.6745 however extreme the ratio. The pooled fallback is absent
   only when every ratio is identical, and then nothing should fire.

Two scopes judge every ratio — its own book, and every paired ratio in the
corpus — and a unit fires if EITHER calls it an outlier, `z_long` above the
median and `z_short` below it, each side held to its own knob. A book under
`min_verses` is therefore not silent: its book lane is unavailable and the
project lane judges it alone. That is the fallback the small-book case rests
on, and the tier says it is real — 11 of en_ulb's 66 books are under 50 paired
verses and the 7 rows they carry all have an unavailable book lane
(evidence.md, 2026-09-07).

## The row

`RuleCode::LengthProportionality`, wire code 0, over the target unit's
projected span. A bridge is ONE row over the **bounding** target range: its
constituents may be discontinuous, and `from`/`to` are navigation coordinates,
not a claim that the bytes between belong to the unit — the same rule the
envelope already states for a split mask (`codec/README.md`).

Both scopes ride every row, always both, as signed Q8.8. `i16::MIN` is the
"scope unavailable" sentinel and decodes as `None`; it is also, by
construction, the under-`min_verses` flag, so no flag bit says the same thing
twice. `SATURATED` means a lane clamped — the analysis value is larger than
±127.996.

Nothing is stored as a float. The ratio is an `f64` inside judging only, and
the wire carries the two deviations rather than the ratio, because a
deviation is what the claim is about. A host that wants the ratio has both
sides and can recompute it; `sous-cli` does exactly that for `--findings` and
for the report page.

## Configuration and recomputation

`LengthConfig { z_long: 3.5, z_short: 3.5, min_verses: 50, enabled: true,
presence: true, source_copy: false, source_copy_min_run: 3 }`, a field of
`JudgingConfig`. `presence` and the two `source_copy` fields belong to the
other two source-compared rules and are documented with them
([`presence.md`](presence.md), [`source_copy.md`](source_copy.md)); only
`source_copy` costs a text walk, which is why it alone is in the pair cache's
identity. The defaults are v1's calibrated ones and the
paired survey is their regression gate, not an invitation to retune them
(`rules/length-proportionality.md`).

A pass reaches them through `ChapterPass::length_config`, and its verse lane
through `ChapterPass::verse_lengths`; `Substrate` answers both, and a tuple
takes its first member that does. That is what lets a resident host move one
judging config rather than two, and keeps the `Expediter` generic over the
pass it drives.

Everything here is derived state a caller may recompute per publication from
the retained lengths, and `judge_lengths` does exactly that. It is also
separable, which is what a resident host needs: `PairedBook::pair` is a pure
function of the two books' rows — ratios, the target spans they name, and the
knob-free `Spread` over them — and `judge_paired` turns pairs a caller already
holds into rows under a config, both rules' rows at once. A `Spread` carries no knob, so `min_verses`
gates it at judging time and a knob never invalidates a pair.

The `Expediter` keys one `PairedBook` per (target checksum, source checksum)
and re-pairs only the books whose side moved, which took a warm paired
republication from 4.9 ms to 0.79 ms and a keystroke from 6.6 ms to 2.95 ms
(evidence.md, 2026-09-07). The key is the whole input, so a hit is the value a
recomputation would produce.

## What is NOT here

Presence and versification shear. An absent key and an empty unit produce no
ratio here; they are [`presence.md`](presence.md)'s wire code 4, judged from
the same `pair_keys` call and cached in the same `PairedBook`, and no
proportionality row ever fabricates a zero-length ratio to stand in for one.
Ambiguous duplicates and partial overlaps come back in `Paired::facts` as
structure a host may report and are rows in neither rule. Versification shear
stays parked and needs its own actionable claim and rule contract rather than
hiding inside this one
([`../../rules/presence-shear.md`](../../rules/presence-shear.md)).
