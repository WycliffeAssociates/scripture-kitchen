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
| `Casing` | word | free-position occurrences of one case-folded word in one case form | that word's free-position occurrences, all forms |
| `WordLength` | word | corpus occurrences of one long case-folded word | every word occurrence the corpus counted |
| `Doubled` | word | one case-folded word immediately repeated, adjacent OR separated | that word's occurrences, cased and uncased |
| `LetterRun` | word walk | runs of one letter of exactly this length | runs of that letter of ANY length ≥ 2 |

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

## The terminal table

Before either word channel judges, `Substrate::judge` learns one thing from
its own `follows` lane and publishes it into the sink: **which glyphs this
corpus puts a capital after.**

```text
en_ulb, follows merged over the corpus
   '.'  upper 33,332 of 33,338 cased handoffs
   ','  upper  4,836 of 47,291
terminal_upper_share_bp 8,000, support_floor 5
   '.' 9,998 bp → forces        ',' 1,022 bp → does not
```

A glyph forces when `upper / (upper + lower)` of the letters it hands off to
reaches `terminal_upper_share_bp` on at least `support_floor` cased handoffs.
The denominator is the cased handoffs, so a glyph followed only by uncased
letters decides nothing and an uncased corpus learns an empty table.

That single number replaces every hard-coded punctuation rule the casing
channel used to need. `he said, \u{201C}Stop\u{201D}` is the case it settles: the quote
is transparent, so `Stop` records the comma, and the comma's own share decides.
In en_ulb it is nowhere near 80%, so `Stop` is free evidence; in a corpus that
reports speech after a comma everywhere it forces, and `Stop` abstains. The
per-corpus tables the committed tier learns are in
[`words.md`](words.md).

`Findings` carries the table beside the pattern table, because three readers
need the same one: the casing judge, `Words::locate`'s rescan, and any host
re-judging from a resident tally. `Words` judged with no substrate beside it
finds none and abstains — an abstention, never a guess.

## `Casing` is a word channel

`Casing` judges [`words.md`](words.md)'s counts rather than the substrate's,
and it is one of three channels whose key is not a scalar. Its `glyph` field is
`ScalarKey::NONE` and the wire carries the u64 word hash in its place
([`codec/README.md`](codec/README.md)); `Pattern::word_hash` reads it back.

The claim is: **for one case-folded word, a case form whose share of that
word's FREE positions is under the word band.** `David` \u{d7}40 against `david`
\u{d7}2 flags the two; a word common in both forms fires nothing, so bivariance
needs no rule of its own. Forced positions are out of both numerator and
denominator, because there the punctuation chose the capital and not the word;
the row itself stores the glyph, and the table above says which ones those are.

Two knobs, and the reason they are separate from the glyph pair:
`word_support_floor` (20) and `word_bands` (`Staircase::WORD_STEPS`, the glyph
staircase at a tenth of its shares). The fleet sweep is why: at shared bands
word casing fires p50 201 rows per corpus against the glyph channels' p50 10,
and a tenth of the shares brings it to p50 11 / p90 31 / p95 42
(`examples/word_volume.rs`, evidence.md W3). `channels.casing` turns the whole
lane off; it ships **on**, because that volume holds.

A corpus whose word aggregates are all `cased == false` emits nothing and
hashes nothing: an uncased script pays for this channel exactly zero.

## `WordLength` is the other one, and it ships off

The claim is: **one case-folded word whose scalar length stands
`word_length_sigma` whole standard deviations or more above the corpus's own
mean word length.** The mean and the deviation are occurrence-weighted over
every `len` byte the aggregates carry, so they are the corpus's own scale and
not a constant. The key is that sigma, saturating in a `u8`; the numerator is
the word's corpus count and the denominator every word occurrence, so the row
reads as "this word, this long, this often". Long end only.

It is `channels.word_length = false` by default, and the reason is a
counterexample, not a volume: names, loanwords, and productive compounds fill
this tail, and none of them is a slip. It exists because the length is already
in the row, and because a corpus whose typography really has run words
together has no other lane that sees it. It abstains in uncased scripts, since
an uncased word is in no row at all.

Its sites are the word's spans, every occurrence and not only the free ones —
length is a property of the word, not of a position. A word both channels name
is **one** site row carrying `CASING | WORD_LENGTH`.

## `Doubled` is the third, and it ships on

The claim is: **one case-folded word written twice in a row, judged against
that word's own count.** Two keys, never pooled — `bare` (whitespace only
between) and `separated` (a nonletter run between, `na, na`) — because they
have different denominators and different reasons to be a slip.

```text
vous  x9,000 in the corpus, 300 of them `vous vous`
   300/9,000 = 3,333 bp, band 3's ceiling is 10 bp   → SILENT, it is a
                                                       construction
the   x60,000, one `the the`
   1/60,000 = 0 bp                                   → FIRES
na    x2,000, one `na, na`                            → a SEPARATE key
```

The denominator is every occurrence the corpus counted of that word, forced or
free, cased or not — the two lanes of `words.md` partition it, so the sum needs
no special case per script and **an uncased script is judged here** although it
pays nothing for `Casing`. A word doubled every time it appears owns its whole
denominator and never fires, which is why the band alone excuses `vous vous`
without an allow-list.

**The recusal is corpus-level, not a band.** `WordTotals::doubling_share_bp` is
the share of the corpus's distinct words that appear doubled twice or more, in
basis points; above `JudgingConfig::doubles_productive_bp` (300 = 3%) the
channel abstains for the whole corpus, because doubling is productive in that
language and no per-word fraction can say so. `JudgingConfig::doubles`
(`DoublesPolicy::{Auto, Always, Never}`) is the host's override, the same shape
`LetterRoster` has. A share and never a count: Jonah and a whole Bible must
answer the same way.

Both numbers are the fleet's (evidence.md, W2). Volume at the shipped ladder is
p50 10 / p90 29 / p95 35 / max 81 rows per corpus over 1,504 corpora — the
glyph channels' own volume — so `channels.doubled` ships **true**. The share
distribution has **no knee**: p50 20 / p90 91 / p95 132 bp with a maximum of
511, so 300 bp recuses the 8 most reduplicating corpora (0.5%) and 500 would
have recused one.

A doubled site's span covers **both words and the separator**, so it is not the
word's span and never merges with a casing or length row; it carries
`Reasons::DOUBLED_BARE` or `Reasons::DOUBLED_SEPARATED`.

## `LetterRun` is the fourth, and it ships on

The claim is: **one letter held down longer than this corpus ever holds it** —
`theee` against thousands of `ee`. It comes out of the word walk like the three
above, but its key is not a word: the row's glyph is the folded letter and the
key byte is the run length, so it is the one channel a hash-keyed decoder must
not treat as a word row.

```text
en_ulb   'l'  24,317 runs of two, one of three
   1/24,318 = 0 bp, band 4's ceiling is 3 bp    → FIRES: `joyfullly`, PSA 81:1
Finnish-like   'aa' x2,000, one `aaa`
   1/2,001 = 4 bp, band 3's ceiling is 10 bp    → FIRES, and `aa` never does
a corpus with no `ee` and one `eee`
   the support gate                             → SILENT
```

The denominator is **that letter's own repeat history**: every run of it of two
or more. So a language that doubles its vowels everywhere is judged on its
triples, a script that never repeats a letter is judged on nothing, and there
is no rule per script.

Two guards, and they are different claims. The first is the ordinary word
staircase over that denominator. The second is a **support gate**: every
shorter length ≥ 2 must itself stand on at least `word_support_floor` runs, so
`eee` speaks only where `ee` is established, and `xxxx` says nothing in a
corpus that never wrote `xxx`. **Length 2 never fires** — it is most of the
denominator, and a letter doubled at all is evidence of nothing.

`channels.letter_runs` ships **true**. Over the committed tier the shipped
defaults fire 9 rows across 8 corpora — mean 1.1, max 4, and three corpora fire
none (evidence.md, W4) — so no fleet sweep was needed to set it. Recusal: none.
The denominator already IS the language's own habit, which is what a recusal
statistic would have had to measure.

Its sites are the word each run sits inside, one row per run; a word this
channel and the casing channel both name is one row carrying both bits.

## Dispersion

`Pattern::books` is how many Target books hold part of that row's numerator,
saturating at 255. Books-possible is the publication's own `book_count`;
nothing is stored for it.

On a word-pass channel it is recomputed from the aggregates at judge time
rather than carried through the tally, because which stored `Before`s count
toward a numerator is a config-dependent judging decision and a tally that
pre-summed them could not answer a re-judge.

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

`Staircase::WORD_STEPS` is the same table at a tenth — 250, 100, 30, 10, 3 bp
— and `word_bands` defaults to it.

A key fires when its share is **strictly under** the rung's share, so a key
that owns every occurrence never fires. Every rung is a config field, and
`Staircase::new` refuses bounds that do not ascend or a last bound that is not
`u32::MAX`.

## Emission order

Deterministic, because the wire pins it:

1. every `Rarity` row, by glyph ascending;
2. then, per glyph ascending, that glyph's rows sorted by `(channel, key)`.

`Channel`'s discriminants run finest grain first, so sorting by channel *is*
sorting finest first, and G2 comes out between G3 and G1 without a sort.
The word pass's channels are last, in channel order — `Casing`, `WordLength`,
`Doubled`, then `LetterRun` — each hash-ascending within itself, and
`LetterRun` letter-ascending. Their rows are a separate pass's, so they never
compete for a headline with a glyph's; `LetterRun` names a real scalar and is
still emitted there rather than beside that glyph's substrate rows, because
what produced it is the word walk. `ScalarKey` orders by code point with the pooled digit
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
