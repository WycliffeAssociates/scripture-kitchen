# Level 1b — character inventory

One shared observation substrate counts every scalar a chapter holds. The
census covers letters too, so a stray letter in a small alphabet reaches the
rarity roster; placement, directed neighbors, run shape, and dispersion are
recorded for punctuation, symbols, digits, and nonletter grapheme atoms only,
and never as occurrence lists. These are views over one model, not twenty
independent rule implementations. Convention-learned lane.

Status: substrate walk landed (`sous_core::substrate`); judging landed
(`sous_core::judge`), so a project's patterns are on the wire; sites landed
(`sous_core::sites`, D2b), so each pattern's matching runs are on the wire
beside it as `Convention` rows. Book scope and dispersion are D3. Stage 3 in
[../roadmap.md](../roadmap.md).

## Evidence questions

### Placement and neighbor ladder

Ask where a glyph or run sits, from coarse to fine:

| grain | comparison | example for comma |
| --- | --- | --- |
| G0 | outer class: letter, space, digit, nonletter | attached versus spaced |
| G1 | outer class conditioned by run composition | attached inside a digit-bearing run |
| G2 | pooled neighbor category | followed by a quote or digit — **D3**: it needs a pooling table the classifier does not carry, so the shipped ladder is G3 → G1 → G0 |
| G3 | exact nonletter neighbor | followed specifically by `.` |

Letters are never individuated as neighbors. Fine grain is used only when its
opportunity count clears the evidence floor; otherwise judgment falls back to
the coarser pool rather than becoming silent.

Directed pairs belong to their first member and are described as logical
start/end relationships, never visual left/right.

A **book edge** is the fifth outer class the substrate records and the one G0
never convicts on: the neighbour a glyph has at the start or end of a file is a
fact about the file, not about the language. The edge occurrence stays in the
denominator, and the glyph is judged by its other side.

### Run length and composition

Run length is judged against that glyph's own run history and is conditioned
by composition so `12,345,678` does not teach punctuation pile-up. Same-glyph
continuations and mixed runs remain distinguishable in retained observations.

### Absolute rarity

A very low corpus count produces a roster for review. Rarity is a list, not a
claim that a glyph is wrong. Accepted glyphs use suppression, not a threshold
distortion.

**Letters are in the roster.** A Hawaiian translation uses thirteen letters;
a stray `z` is exactly the kind of slip a rarity roster should surface, and
the `scalars` lane counts every scalar. The neighbor ladder still never
individuates letters; this is the census only.

Abstention for large letter inventories is **ruled**: `letter_roster_bound`
defaults to 500 distinct letters, above which letters leave the roster and the
report says so. The fleet census found every alphabet under 300 distinct
letters and every logographic corpus above 3,000, with an empty band between
(see the inventory census row in [../evidence.md](../evidence.md)). A second
floor, `letter_roster_min_letters` (default 5,000, about two chapters), keeps a
ten-verse draft from rostering `q`, `x`, `z` by sample size; nonletters have no
such floor. `LetterRoster::Auto | Always | Never` overrides both.

### Dispersion

Books-touched versus books-possible is annotation and ranking only. Genre can
legitimately cluster punctuation, so dispersion does not convict. A small
"forgiven but clustered" review group may surface above-band patterns without
turning dispersion into a gate.

## Observation

`sous_core::substrate` is the one walk every question above reads from. One
`ChapterRow` per chapter, each lane a sorted vector, seams resolved in the fold
(shape and argument: [`../core/src/substrate.md`](../core/src/substrate.md)).
What the counts are judged into — the entitlement rule, the ladder, and the
emission order — is [`../core/src/judge.md`](../core/src/judge.md).

| lane | answers |
| --- | --- |
| `scalars` | absolute rarity, the dense census letters included, and every denominator |
| `pairs` | G0 placement: the outer class either side of a nonletter |
| `runs` | run composition — the exact scalar sequence, so G1 conditioning and the G2/G3 in-run neighbours are reads, not a second walk |
| `run_lengths` | run length against that glyph's own history, derived from `runs` rather than stored twice |
| `follows` | the casing a run terminal hands off to |
| `lead`, `trail` | the two open edges a masked `\c` would otherwise swallow |

Dispersion and per-book occurrence are fold products over these, not stored
rows. Sites are absent by construction: nothing here records a position.

## Fraction bands

The initial readable staircase is a shipping candidate, not an eternal
constant:

| opportunity count | minority share eligible for review |
| --- | --- |
| under 5 | abstain; rarity owns the roster |
| up to 10 | 25% |
| up to 100 | 10% |
| up to 1,000 | 3% |
| up to 10,000 | 1% |
| above 10,000 | 0.3% |

Judging expresses these in basis points, so 0.3% is exact and every step is a
config field (`sous_core::judge::Staircase`). A key fires when its share is
strictly under its rung.

Fleet evidence found this approximates `0.8 / sqrt(n)` and sits in a broad
valley between slip-shaped minorities and competing conventions (see the band
sweep row in [../evidence.md](../evidence.md)). Keep the five explicit bands
for speakability. A master sensitivity control, if later needed, moves a
calibrated global mapping; it does not fit itself to the open project.

Each evidence channel judges independently. A site fires when any entitled
channel passes. The UI may headline the strongest reason, but the finding
retains every independently sufficient reason and one run still yields one
finding.

## Required examples

- `ng'` used 851 times across books is learned as convention and stays silent.
- 818 letter-attached commas in only three books become one pattern row in the
  "forgiven but clustered" group, not 818 findings and not silence.
- `word?.` can fire on the exact pair when its specific opportunity set is
  strong even though the individual glyphs are common.
- `,..,` can fire on run length when its outer placement is ordinary.
- `...` in an ellipsis-writing corpus is excused by its entitled run history.
- A strange comma/backtick pair fires through the rare member when the common
  comma has no entitled exact-pair evidence.
- French guillemet spacing, Amharic `::`, uncased Amharic, and a glottal-stop
  apostrophe convention self-abstain without allow-lists.
- In a ten-verse draft, a 1-of-6 comma pattern is a cheap review rather than a
  small-sample silence.
- Low Line used 28 times across six books reads "rare but dispersed"; `}` used
  once or twice appears in the rarity roster.
- ZWJ/ZWNJ in Indic text never enters the inventory (charter invariant 8).

## PO checklist absorbed by this lane

The substrate covers spacing around punctuation, free-floating spacing-clone
marks, orphaned punctuation, repeated punctuation, punctuation/quote order,
phrase-ending marks at logical seams, word-medial punctuation, unexpected
glyphs, lowercase after a learned terminal, and digit-lane census shapes.
These are catalog views and reasons, not separate hot-loop rules.
