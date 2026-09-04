# `sous_core::judge`

What the substrate's counts are judged *into*: one pattern row per firing
claim, over the whole project. The rule this implements is
[`../../rules/character-inventory.md`](../../rules/character-inventory.md);
this file is the ladder, the entitlement rule, the bands, and the order the
rows come out in.

## Scope is the project

Every Target book's counts, summed. A convention is a corpus fact: whether
"comma attached to a letter" is a slip depends on what every other book does.
Judging reads the `BookAggregate`s and keeps nothing.

## The channels

A pattern is `(channel, glyph, key)`. Each channel judges on its own
denominator, and the channels are independent — a glyph may fire on three of
them at once, and D2b unions the patterns one run matches into a single
finding.

| channel | grain | numerator | denominator |
| --- | --- | --- | --- |
| `ExactNeighbor` | G3 | in-run positions where `g` is followed by `n` | in-run positions where `g` is followed by anything |
| `PooledNeighbor` | G2 | in-run positions where `g` is followed by an atom of that pool | the same positions G3 counts |
| `RunShape` | G1 | runs of `g` with this `(pure, length bucket)` | runs containing `g` |
| `Placement` | G0 | occurrences of `g` with this outer class, one side | every occurrence of `g` |
| `Rarity` | — | corpus count of the glyph | every scalar counted |

`Placement` is **per side, marginal**: the outer class before `g` and the
outer class after `g` are two distributions over
`{Letter, Space, Digit, Nonletter, Edge}`, each against the same denominator.
A side fires on a class whose share is under the band.

`Edge` is the one class that **counts and never fires**. A glyph at the start
or end of a book has a neighbour that is a fact about the file, not about the
language, so the edge side emits nothing and the glyph is judged by its other
side — while the occurrence stays in the denominator both sides share, because
it is still an occurrence. The rows stay per side; the collapse a reviewer
wants happens at the site, where `Reasons::PLACEMENT_BEFORE | PLACEMENT_AFTER`
ride one span.

`PooledNeighbor` names a **kind** of neighbour rather than a scalar, so a
convention that a script spells three ways — `”`, `"`, `’` — is one row
instead of three. The eight pools come from pinned UCD properties
(`unicode::Pool`), first match wins, so Ethiopic `።`, danda `।`, and `.` are
all `Terminal` without an ASCII allow-list. It shares G3's
denominator, so the two are entitled and abstain **together**: G2 is the
coarser statement over the same evidence, not a fallback for a G3 that went
silent — and because a pool's share is never under a member's, a G2 row never
fires alone. It is therefore **off by default** (`Channels::pooled_neighbor`);
the pool table stays for grouping and for later rules. `Pool::Digit` cannot occur — a digit is not a run atom — and exists so
`pool_of` is total.

`Rarity` is its own channel and its own kind of claim: a list for review, not
an assertion that a glyph is wrong. It carries no band. Digits are one pooled
key and never rare; glue never reaches the inventory at all (charter invariant
8), so it can never be rostered.

## Entitlement, and the ladder

A channel is **entitled** when its denominator is at least `support_floor`
(default 5). Below that it abstains: it emits nothing, and it makes no claim
that nothing is wrong.

The ladder is **finest-entitled-first**, G3 → G2 → G1 → G0, and what it rules is
that *abstention is not silence*. A glyph whose finest channel abstains is
still judged at the next coarser grain, so the corpus's answer falls back to a
coarser comparison rather than disappearing. Because the channels are
independent, a glyph whose G3 *was* entitled still gets G1 and G0 judged too;
the ladder decides only that an abstention does not end the question.

Worked example — a strange `` ,` `` pair against five thousand ordinary
commas:

```text
runs      [',']  ×5,000        [',', '`']  ×1

G3 for ','   positions where ',' is followed by an atom: 1
             1 < support_floor 5  → ABSTAIN, and the comma emits no
                                    ExactNeighbor row
G2 for ','   the same denominator                        → ABSTAIN too
G1 for ','   runs containing ',': 5,001, keys (pure,1)×5,000 and (mixed,2)×1
             band for 5,001 is 100 bp; 1/5,001 = 1 bp  → FIRES
G0 for ','   5,001 occurrences, two sides                → judged as usual
Rarity '`'   corpus count 1 < rarity_floor 5             → rostered
```

The pair is reviewable through the rare member and through the run shape. The
comma's own exact-pair evidence was never entitled, and that is a fact about
the opportunity set, not a verdict.

## Dispersion

`Pattern::books` is how many Target books hold part of that row's numerator,
saturating at 255. Books-possible is the publication's own `book_count`;
nothing is stored for it.

It is **information, not a judgement**. Genre clusters punctuation
legitimately and a project's book set is not the engine's business, so no
threshold, flag, or squiggle reads it — a front end can say "818 letter-attached
commas, in 3 of 40 books" and leave the ruling to a person.

The count is taken during the same merge that builds the numerator: books
arrive in `BookIndex` order, so a `Tally` needs only the last contributor to
count distinct ones, and nothing walks the aggregates twice.
`judge::books_touched(corpus, pattern)` recomputes it from the retained
aggregates for any pattern a host holds, and is the oracle the merge is tested
against.

## The bands

The staircase of `rules/character-inventory.md`, in basis points so 0.3% is
exact. A `band` on a pattern row is the index of the rung its denominator
fell in.

| band | denominator up to | minority share eligible for review |
| --- | --- | --- |
| — | under `support_floor` | abstain; rarity owns the roster |
| 0 | 10 | 2,500 bp |
| 1 | 100 | 1,000 bp |
| 2 | 1,000 | 300 bp |
| 3 | 10,000 | 100 bp |
| 4 | `u32::MAX` | 30 bp |

A key fires when its share is **strictly under** the rung's share, so a key
that owns every occurrence never fires. Every rung is a config field, and
`Staircase::new` refuses bounds that do not ascend or a last bound that is not
`u32::MAX`.

## Emission order

Deterministic, because the wire pins it:

1. every `Rarity` row, by glyph ascending;
2. then, per glyph ascending, that glyph's rows sorted by `(channel, key)`.

`Channel`'s discriminants run finest grain first, so sorting by channel *is*
sorting finest first, and G2 comes out between G3 and G1 without a sort. `ScalarKey` orders by code point with the pooled digit
lane last.

## Where the row goes

`Findings::push_pattern` takes it; patterns are corpus-level, so they are
pushed either side of any `open_book`. The 24-byte wire row, the header's
`pattern_count`/`pattern_offset`, and wire code 2 (`Convention`, which names a
pattern from a site) are
[`codec/README.md`](codec/README.md).

A pattern has no coordinates. `ChapterPass::locate` runs after judging and
gives it some: [`sites.md`](sites.md) rescans each book's current text for the
patterns its own counts hold, and each matching run becomes one `Convention`
row naming this table.

## Arithmetic

Denominators, numerators, and shares are computed in `u64` and saturate into
their wire widths, so a ten-million-count corpus cannot wrap on the way out —
pinned by `a_ten_million_count_corpus_does_not_wrap`.
