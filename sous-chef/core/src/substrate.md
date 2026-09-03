# `sous_core::substrate`

The Level 1b walk. What the counts are *for* — the G0–G3 ladder, run history,
rarity — is [`../../rules/character-inventory.md`](../../rules/character-inventory.md);
this file is the observation's shape and the two arguments behind it. It
implements charter invariants 2–4, 7, and 8, and the roadmap's Stage 3 rule of
thumb: **one scalar walk per chapter**.

## The row

One `ChapterRow` per chapter, every lane a sorted vector of plain scalars.
Nothing borrows, nothing hashes, nothing carries a coordinate.

| lane | key | value | bytes/entry | answers |
| --- | --- | --- | --- | --- |
| `scalars` | `ScalarKey` (a scalar, or the pooled `DIGITS`) | count | 8 | absolute rarity, the dense census, every denominator |
| `pairs` | `(ScalarKey, prev outer, next outer)` | count | 12 | G0 placement, and G1 once conditioned by `runs` |
| `runs` | the run's scalar sequence | count | 12 + 4/atom | run composition, G2/G3 neighbours inside a run |
| `follows` | `ScalarKey` of a run terminal | upper/lower/uncased | 16 | lowercase after a learned terminal |
| `lead`, `trail` | — | one open edge each | 20 each | the seam (below) |
| `scalar_count`, `word_count` | — | — | 8 | denominators and the Stage 4 word seam |

`size_of::<ChapterRow>()` is 128 B; the rest is what the five lanes own.
Measured over the committed tier (the ignored oracle in
`tests/substrate_reference.rs` prints it): median **964 B**, p90 **1,324 B**,
per-corpus medians 828 B (Spanish) to 1,752 B (Greek). The budget is 1.5 KB
median, 2 KB p90.

Three shapes are deliberate:

- **Letters are counted.** A Hawaiian `z` has to be able to reach the rarity
  roster, so the census is dense. It is also the single biggest lane —
  Greek's 107 distinct scalars per chapter are 856 of its 1,752 bytes.
- **`run_lengths` is derived, not stored.** The rule wants each glyph's own
  run history; the run *sequences* already carry it exactly, so
  `ChapterRow::run_lengths()` decomposes them on read rather than the row
  paying ~260 B a chapter to say the same thing twice.
- **Digits pool everywhere a scalar is a key** — inventory, pair, run atom,
  follow. Charter invariant 7 is one judging lane, so `12,345` and `१२,३४५`
  are the same run shape and the same pair triple.

Glue is the mirror rule: a Mark or extender is never a scalar, a pair member,
or a run atom (charter invariant 8), but it still *reads* as `Letter` when a
neighbour asks, because it rides its base.

## Why intern, and why the table is per call

The walk counts pairs, runs, and follows into arrays indexed by a dense
chapter-local id, then resolves ids back to scalars once, at the end. The
alternative is hashing a `(scalar, prev, next)` triple per nonletter, which is
a hash and a probe on the hot path instead of `slots[id].pairs[prev * 5 +
next] += 1`.

Interning is cheap because it is only over *nonletters*: 5–13 distinct per
chapter across the tier. ASCII nonletters take a `[u32; 128]` direct table,
the pooled digit lane a single slot, and only non-ASCII punctuation reaches an
`FxHashMap`.

The dense *inventory* — every scalar, letters included — is a different
problem: 48 distinct per English chapter but 132 for Amharic and 107 for
Greek, and every scalar touches it. It is a `[u32; 128]` ASCII array plus a
512-slot open-addressed table that never fills past half, so a probe lands
first try and an empty slot always ends the chain. What the table declines
once it is half full falls back to an `FxHashMap`. Measured: that table is
worth ~14% on Amharic and nothing on Latin.

All of it is allocated per call and dropped at the end of `map`. `map` takes
`&self` and the pass must be `Sync`, so a thread-local or a `RefCell<Scratch>`
would either be a lie about sharing or a lock on the parallel path. The cost
of allocating per call is measured, not assumed: **~11 allocations and ~16
allocator operations per chapter**, most of them the five output lanes. If
that ever shows, the smallest fix is a `map_with(&mut Scratch)` on a separate
trait method that a serial host may call and the parallel path ignores — not
hidden shared state.

The hot per-scalar state is a separate 24-byte `Hot` local rather than fields
on the counter struct. Every counter write goes through a heap pointer, and a
compiler that finds the walk state behind the same `&mut` reloads all eleven
fields after each one. Hoisting it out was worth 20–27% on every corpus.

## The seam, and the two open edges

A chapter map may not read its neighbours (charter invariant 4), but three
facts genuinely straddle a `\c`:

```text
chapter k  "… good."          chapter k+1  "Then he …"
                    └── trail ──┴── lead ──┘
  pair    ('.', Letter, Edge)      →  ('.', Letter, Letter)
  follow  '.' awaiting a letter    →  '.' → upper 1
  word    "good" ended             →  no join; two words
```

So the row records two open edges and reduce resolves them:

- `outer` — the edge scalar's own class, which is all the neighbour needs.
- `open_pair` — the edge scalar when it is a nonletter, with the one neighbour
  class it already knows. Reduce decrements the triple that names `Edge` and
  increments the resolved one.
- `open_follow` (trailing) — a run terminal with only whitespace between it
  and the chapter end.
- `edge_case` (leading) — the casing of the first non-whitespace scalar, when
  it is a letter. Paired with the previous chapter's `open_follow`, that is
  the follow the seam swallowed.
- `blank` (trailing) — the chapter held whitespace and nothing else, so the
  previous chapter's `open_follow` survives it.

`Carry` is that trailing edge, `Default` at every book. Two cases need saying:
an **empty** chapter is not a neighbour at all, so the carry passes through it
untouched; and a **one-scalar** chapter is its own lead and trail, so the
trailing edge inherits the `prev` its leading fix just resolved.

One thing is deliberately *not* stitched: a nonletter run that straddles the
seam stays two runs. That is the same ruling hygiene makes for a control run
abutting a masked `\c`, and it is what `a_run_straddling_a_seam_stays_two_runs`
pins.

`fold_book` merges every lane by sorted merge — one pass, no hashing, and the
same result whether a row came from a cache or from this call, which is what
makes an incremental analysis equal a cold one. In D1a it returns a
`BookAggregate` and emits nothing; D2's rule reducers judge that aggregate.
