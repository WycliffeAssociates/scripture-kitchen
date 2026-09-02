# `sous_core::hygiene`

The Level 1a scan. What each check claims, when it stays silent, and which
test pins it: [`../../rules/hygiene.md`](../../rules/hygiene.md). This file is
the implementation.

## Scan shape

Four passes over one projected book, each chosen so that clean text never
leaves a fast path. Rows are merged and sorted by start offset at the end.

| pass | filter | finds |
| --- | --- | --- |
| `scan_controls` | 64-byte blocks, an OR-reduction the compiler autovectorizes | C0 controls, DEL, stray CR |
| `scan_needles` | one `memchr3` over `\`, `C2`, `EF` | stranded backslash, C1 controls, U+FFFD |
| `scan_conflict_markers` | one `memchr3` over `<`, `=`, `>` | line-initial merge-conflict markers |
| `scan_scalars` | eight-byte SWAR ASCII skip ahead of `unicode::lookup::trie_at` | free marks, misplaced format characters, NBSP, noncharacters |

Two details carry the cost:

- **The range filter branches once per block, not per byte.** `is_control` is
  folded with `|` rather than short-circuited, so the block test vectorizes;
  only a block that reports a hit pays the per-byte walk.
- **The SWAR lane has hysteresis.** It re-arms only after 32 consecutive ASCII
  scalars. Without that, non-Latin text pays the eight-byte test on every
  chunk and loses more than the chunk saves — the failure mode measured in
  [`../../evidence.md`](../../evidence.md) and kept in
  [`../../experiments/`](../../experiments/).

Every scalar-level check lives above U+007F, which is exactly what lets the
ASCII lane skip whole words without looking at a class.

## Runs and spans

A finding is one **maximal same-class run**, so 223 NUL bytes are one row, not
223. `run()` counts offending code points; a marker line counts as one.

Every span then passes through `unicode::atoms::widen_to_atoms`, so a finding
never splits a rendered grapheme (charter invariant 6). The span may therefore
be one atom wider than `run()` suggests.

Five classes provably cannot move under widening — `C0Control`, `Delete`,
`C1Control`, `StrayCarriageReturn`, `ConflictMarker` — because their scalars
are Grapheme_Cluster_Break Control, CR, or LF, and UAX #29 breaks on both
edges. A `debug_assert` pins that. U+FFFD and a stranded backslash are
ordinary bases, so a combining mark behind one legitimately joins it; that is
invariant 6 working, not drift.

## Pass contract

`hygiene::Hygiene` implements `ChapterPass` with
`Observation = Vec<HygieneFinding>` in chapter-relative coordinates and
`Carry = ()`; `map` is `scan` over the chapter slice and `reduce` only rebases
each row by its chapter's projected start. The contract itself, and why reduce
cannot tell a cached observation from a fresh one:
[`pass.md`](pass.md).

A run abutting a masked `\c` marker is two findings by design, and front
matter is outside every chapter row, so it is outside the pass. Galley may
cache rows by chapter content.

No config changes observations; enablement only filters.

## Throughput

Measured against the roofline ceilings in
[`../../evidence.md`](../../evidence.md) — `cargo bench -p sous-core`. Carrying
the classifier walk costs real throughput, and that is the pass every Level 1b
observation will ride. Nothing in the bench asserts on a number.

## The lone-backslash caveat

Through the Onion producer, a lone backslash never reaches this scan. Onion
lexes it as a marker — well-formed or not — and masks it out, so only a `\\`
pair arrives as content. `StrandedBackslash` therefore fires on a doubled
backslash under Onion, and on any backslash under a vref producer, which keeps
them all as content. Marker validity itself is Onion's job.
