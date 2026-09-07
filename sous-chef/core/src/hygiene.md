# `sous_core::hygiene`

The Level 1a scan. What each check claims, when it stays silent, and which
test pins it: [`../../rules/hygiene.md`](../../rules/hygiene.md). This file is
the implementation.

## Scan shape

Three byte sweeps over one projected book, each chosen so that clean text
never leaves a fast path. Rows are merged and sorted by start offset at the
end.

| pass | filter | finds |
| --- | --- | --- |
| `scan_controls` | 64-byte blocks, an OR-reduction the compiler autovectorizes | C0 controls, DEL, stray CR |
| `scan_needles` | one `memchr3` over `\`, `C2`, `EF` | stranded backslash, C1 controls, U+FFFD |
| `scan_conflict_markers` | one `memchr3` over `<`, `=`, `>` | line-initial merge-conflict markers |

The four scalar classes — free marks, misplaced format characters, NBSP,
noncharacters — are not a sweep here at all. They ride the substrate walk:
`ScalarSites` below is the streaming machine `substrate/walk.rs` drives, and the
row's `hygiene` lane is where they come out. See
[`substrate.md`](substrate.md).

One detail carries the cost: **the range filter branches once per block, not
per byte.** `is_control` is folded with `|` rather than short-circuited, so the
block test vectorizes; only a block that reports a hit pays the per-byte walk.

Every scalar-level check lives above U+007F, which is what lets the substrate
walk's eight-byte ASCII lane reach `ScalarSites` through one branch — a
pending verdict — and never through a class test.

`ScalarSites` restates the retired `scan_scalars` for streaming. A verdict
that needs the *next* scalar (a format character, an NBSP) is pending until
the following `step`, or until `finish` resolves it against no next scalar at
all; run members are contiguous, so a gated scalar that does not abut the open
run closes it. A chapter end is an edge of text, exactly as it was, which is
what keeps the lane byte-for-byte equal to the old scan
(`tests/hygiene_scalar_reference.rs`).

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

`hygiene::HygieneBytes` implements `ChapterPass` with
`Observation = Vec<HygieneFinding>` in chapter-relative coordinates,
`Aggregate = Box<[HygieneFinding]>` in book coordinates, and `Config = ()`.
`map` is `scan` over the chapter slice, `fold` only rebases each row by its
chapter's projected start, and `judge` pushes each book's rows under its
index. The product pass is `sous_core::Brigade` —
`(HygieneBytes, Substrate, Words)` — which is what a host registers to get all
seven classes. The contract itself,
and why neither fold nor judge can tell a cached input from a fresh one:
[`pass.md`](pass.md).

A run abutting a masked `\c` marker is two findings by design, and front
matter is outside every chapter row, so it is outside the pass. Galley may
cache rows by chapter content.

No config changes observations; enablement only filters.

## Throughput

Measured against the roofline ceilings in
[`../../evidence.md`](../../evidence.md) — `cargo bench -p sous-core`. The
`hygiene` row is the three byte sweeps alone now that the classifier walk has
moved to `substrate_map`, which is the row that carries it. Nothing in the
bench asserts on a number.

## The lone-backslash caveat

Through the Onion producer, a lone backslash never reaches this scan. Onion
lexes it as a marker — well-formed or not — and masks it out, so only a `\\`
pair arrives as content. `StrandedBackslash` therefore fires on a doubled
backslash under Onion, and on any backslash under a vref producer, which keeps
them all as content. Marker validity itself is Onion's job.
