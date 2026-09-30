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
denominator, and a glyph may fire on three of them at once; D2b unions the
patterns one run matches into a single finding. What the channels do not do
is count one occurrence twice: **an entitled finer judgment is not counted
again by a coarser row** ([Both sides](#both-sides)).

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
| `SentenceStart` | handoff glyph | lowercase letters this glyph handed off to bare | cased letters it handed off to bare |
| `BookRate` | G0, one book | occurrences of `g` in one book with this outer class, one side | that book's occurrences of `g` |

`Placement` is **per side, marginal**: the outer class before `g` and the
outer class after `g` are two distributions over
`{Letter, Space, Digit, Nonletter, Edge}`, each against the same denominator.
A side fires on a class whose share is under the band.

`Edge` is the one class that **counts and never fires**. A glyph at the start
or end of a book has a neighbour that is a fact about the file, not about the
language, so the edge side emits nothing and the glyph is judged by its other
side — while the occurrence stays in the denominator both sides share, because
it is still an occurrence. A line break is the same kind of fact: the
projection writes paragraph and poetry markup as `\n`, so a mark beside one
touches structure, not a space.

```text
JER   \q2 —this is Yahweh's declaration    '—' prev=Edge   counted, never fires
ZEC   of hosts— and I will return          '—' next=Space  judged
``` The rows stay per side; the collapse a reviewer
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
coarser comparison rather than disappearing. A glyph whose G3 *was* entitled
still gets G1 and G0 judged too, less the occurrences G3 already owns; the
ladder decides only that an abstention does not end the question.

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

## Both sides

A `Placement` row asks one side: of every `.`, how many follow a digit. A row
unusual from the glyph's side is then asked from the class it touches, and
fires only if it is unusual there too. `RunShape` gets the same treatment
below.

```text
en_ulb   ',' prev=Nonletter    103/54,723 = 18 bp, band 4's ceiling is 30
   the mark before: ')' 97, ''' 4, '"' 1, '?' 1, each an entitled leader
   103 - 103 judged by ExactNeighbor = 0                  -> SILENT
en_ulb   '.' prev=Digit        116/38,776 = 29 bp, under 30
   number ends: 116 of 566 digits followed by a non-digit = 2,049 bp
   band 2's ceiling is 300 bp; ordinary among numbers     -> SILENT
'(1'     digits prev=Nonletter
   a digit is not a run atom, so no ExactNeighbor saw it  -> stays with Placement
```

| class it touches | the other side |
| --- | --- |
| `Nonletter` | the in-run pair's leader: `g`'s neighbour for `prev`, `g` itself for `next` |
| `Digit` | `ScalarKey::DIGITS` occurrences whose `next` (for `prev`) or `prev` (for `next`) is not a digit |
| `Letter`, `Space` | none: the fleet shows the check never clears one |

- **`Nonletter` subtracts.** An in-run pair whose leader's ExactNeighbor is
  entitled (its in-run positions reach `support_floor`) leaves the numerator,
  whether or not that row fired: a fired row already reports the pair, and an
  unfired one found it ordinary. An unentitled leader leaves the pair with
  Placement, so abstention still falls back. With `channels.exact_neighbor`
  off nothing judges the pair and nothing is subtracted.
- **`Digit` silences.** The whole row is ordinary when its numerator over the
  number ends (or starts) is at or over that denominator's band ceiling, on an
  entitled denominator.
- **The denominator never moves.** An explained occurrence is still an
  occurrence of `g`.
- **The glyph's side decides first.** The other side only removes: a row
  ordinary from `g`'s side never fires because a subtraction made it small.
  A row whose numerator reaches 0 is silent; one that fires reports what is
  left, and `books` counts the books holding what is left.

`RunShape` is one-sided the same way: "`;` rarely sits in a cluster of two"
fires on `);`, which is ordinary. Its other side is whether the exact cluster
recurs.

```text
en_ulb   ';' mixed len 2       26/4,904
   );×8  ';×7  ";×6   each occurs >= support_floor 5: a convention
   ;'×3  ;"×2         left                                -> FIRES 5/4,904
en_ulb   '"' mixed len 4       31/12,015
   ."'"×27 recurs; ?"'"×2 recurs as T"'"×29               sentence ends read as one
   .'?"×1  "...×1 left                                    -> FIRES 2/12,015
```

- **Exact sequences, save one kind.** A run recurs when it, or it with every
  `Pool::Terminal` mark read as one placeholder, occurs `support_floor`
  times. The order stays: `.'?"` is `T'T"`, not `T"'"`. Pooling any other
  kind folds `;'` into `';` and clears the swap that is the real signal, so
  `;'` still fires.
- **`support_floor` is the recurrence count.** It already means "enough
  evidence to call it a habit". A correct but rare variant (`?"'"`×2) still
  fires, and a wrong cluster repeated five times passes.
- The denominator never moves, the glyph's side decides first, and a row left
  with no runs is silent, as for `Placement`.
- A firing row lists its clusters, recurring ones marked by either route, into
  `Findings::clusters` right after it is pushed: at most 8, novel first, with
  3 slots kept for the conventions (`judge::Cluster`, and the wire section in
  [`codec/README.md`](codec/README.md)).

The rescan must site exactly what is left, and a firing set cannot say which
occurrences that is, so `Substrate::judge` publishes [`Explained`] into the
sink beside the pattern table: the entitled leaders the firing `Nonletter`
rows skip and the recurring clusters the firing `RunShape` rows skip.
`sites::locate` reads it, and a host keys its site caches on it (`galley`'s
`EvidenceHash`).

## The terminal table

Before either word channel judges, `Substrate::judge` learns one thing from
its own `follows` lane and publishes it into the sink: **which handoff
contexts this corpus puts a capital after.**

```text
he said, Go          (',', bare)       the last glyph that is not a quote or bracket
he said, "Go         (',', quoted)     a quote stood between it and the letter
"Go," he said        (',', quoted)     position tells no opening quote from a closing one
one. (Two            ('.', bare)       an opening bracket rides and marks nothing
forever.) to him     ('.', bracketed)  a closing bracket stood between
said "Go             none              a word closes the chain
```

```text
WA-en-ulb (the vref tier), follows merged over the corpus
   ('.', bare)       upper 33,353 of 33,359 cased handoffs   9,998 bp → forces
   (',', quoted)     upper  6,748 of  7,156                  9,429 bp → forces
   ('.', bracketed)  upper     33 of     54                  6,111 bp → mixed
   (',', bare)       upper  4,841 of 47,299                  1,023 bp → free
terminal_upper_share_bp 8,000, terminal_lower_share_bp 2,000, support_floor 5
```

A context **forces** when `upper / (upper + lower)` of the letters it hands off
to reaches `terminal_upper_share_bp` on at least `support_floor` cased
handoffs, and is **free** at or under `terminal_lower_share_bp`. Between the
two it is **mixed**: the punctuation neither chose the capital nor left the
word to choose, so the word after it is no casing evidence either way.

```text
JOS 14:15   …greatest man among the Anakim.) Then the land had rest
            ('.', bracketed) is mixed               → `Then` is not judged
spaRV1909   dirán: Su mujer es                     (':', bare) 7,922 of 14,345
            5,522 bp is mixed                       → `Su` is not judged
```

A context under `support_floor` cased handoffs has no share to read and stays
free. The denominator is the cased handoffs, so a context followed only by
uncased letters decides nothing and an uncased corpus learns an empty table.

Those two numbers replace every hard-coded punctuation rule the casing
channel used to need. `he said, \u{201C}Name your wages` is the case it
settles: the comma alone capitalizes a tenth of the time, diluted by every
comma not in front of speech, but the comma through a quote is its own
context with its own counts. In en_ulb it forces, so `Name` there is the
punctuation's capital and abstains; in a corpus that opens speech in lowercase
`, "` stays at or under the lower share and the capital is judged. Which marks are quotes is
`unicode::Pool::Quote`, and which brackets close is `unicode::closes` (general
category `Pe`), never a list. The per-corpus tables the committed tier
learns are in [`words.md`](words.md).

`Doubled` reads only the forcing half: a separator that forces is a sentence
boundary, and a mixed one is not.

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
word's FREE positions is under the word band** — neither forced nor mixed. `David` \u{d7}40 against `david`
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

The claim is: **one case-folded word written twice in a row, guilty until
innocent.** Every doubling fires unless the word doubles habitually. Two keys,
never pooled — `bare` (whitespace only between) and `separated` (a nonletter
run between, `na, na`) — each with its own habit count and its own switch.

```text
surface  x47, one `surface surface`           → FIRES
the      x60,000, one `the the`               → FIRES
vous     x9,000, 148 `vous vous`              → SILENT: doubled >= support_floor (5)
na       x2,000, one `na, na`                 → FIRES, a SEPARATE key
na       4 `na na` and 5 `na, na`             → bare FIRES, separated SILENT
```

The habit is an absolute count, `JudgingConfig::support_floor`, per key: the
word ladder and `word_support_floor` do not apply here, because a word used
under a hundred times could never fire on one doubling under them, and that is
most of the vocabulary. The row still carries the doubled count over every
occurrence the corpus counted of that word, forced or free, cased or not — the
two lanes of `words.md` partition it, so **an uncased script is judged here**
although it pays nothing for `Casing`. The row's band is the word ladder's rung
for that denominator: context for a reader, not a gate.

```text
JOB 20:7   …'Where is he?'
JOB 20:8   He will fly away…              → not a doubling: two verses
PSA        na\nna, one verse, two lines   → a doubling, bare
go. Go     `.` forces a capital here      → not a doubling: two sentences
```

A pair never spans a verse boundary: its two words carry the same `VerseKey`
or it is not counted, so two segments of one verse still pair. A line break is
whitespace. A separator whose handoff context the corpus's `TerminalTable` forces is
a sentence boundary and folds out of the separated numerator.

```text
JudgingConfig::doubled_bare = false        → no bare row
JudgingConfig::doubled_separated = false   → no separated row (the default)
DoublesPolicy::Never or channels.doubled = false → no row
```

**The recusal is corpus-level.** `WordTotals::doubling_share_bp` is the share
of the corpus's distinct words that appear doubled twice or more, in basis
points; above `JudgingConfig::doubles_productive_bp` (300 = 3%) the channel
abstains for the whole corpus, because doubling is productive in that language.
`JudgingConfig::doubles` (`DoublesPolicy::{Auto, Always, Never}`) is the host's
override, the same shape `LetterRoster` has. A share and never a count: Jonah
and a whole Bible must answer the same way. The recusal reads both keys
whatever the two switches say.

Both numbers are the fleet's (evidence.md, W2 and D1). Volume at the shipped
defaults (separated off) is p50 14 / p90 80 / p95 112 / max 391 rows per
corpus over 1,504 corpora, and p50 55 / p90 142 with the separated key on,
which is mostly vocatives and genealogy chains. The share distribution has **no knee**: p50 14 /
p90 68 / p95 99 bp with a maximum of 477, so 300 bp recuses the 4 most
reduplicating corpora (0.3%). Bantu reduplication (`bwino bwino`, one to four
times a word) stays under both the habit count and the recusal; a project it
floods turns the channel off.

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

## `SentenceStart` is the terminal table read the other way

The same `follows` lane the table above learns from, asked the opposite
question. The table asks *did the punctuation choose this capital*, so a word
after a forcing glyph is no evidence about the word. This channel asks *did the
punctuation not get the capital it almost always gets*, so a lowercase letter
after a near-certain glyph is one row for review.

```text
WA-en-ulb, follows merged, BARE handoffs only
   ('.', bare)    upper 33,353 of 33,359   9,998 bp >= 9,800  -> FIRES 6/33,359
   (',', bare)    upper  4,841 of 47,299   1,023 bp           -> silent
   ('!', bare)    upper  1,213 of  1,222   9,926 bp           -> FIRES 9/1,222
   ('?', quoted)  upper  1,030 of  1,069   not judged: `?" he said` is no exception
```

**The row judges a glyph's bare handoffs and nothing else.** Its pattern names
a glyph, not a context, and the quoted context mixes openings with closings:
folded in, `?" he said` would make `?` fire 42 times in en_ulb for ordinary
English. The bracketed context is the same kind of claim: in en_ulb PSA 136
every verse ends `(His covenant faithfulness endures forever.)` and the next
verse runs on in lowercase, `to him who by wisdom made the heavens`, and the
period there closed the parenthetical, not the sentence. `Cursor::handoff`
reports whether it rode a quote or a closing bracket, and the site rule drops
those handoffs exactly as the counts do.

Per glyph with at least `support_floor` cased handoffs: `upper / (upper +
lower)` decides whether the glyph speaks, and the row then reports `lower /
cased` — the exception's own fraction, which is what a reviewer reads. No row
when `lower` is zero: a glyph that never slipped has nothing to show. The band
is the glyph staircase over the cased handoffs and is **cosmetic**; the
threshold is the whole firing rule, which is why a band of 4 sits beside a
share far under its rung.

**Two knobs, and they are not the same number.** `terminal_upper_share_bp`
(8,000) decides forced for a WORD: above it the punctuation chose the
capital, so the word's own habit is unobservable there, and a mixed context
under it is no evidence either. `sentence_start_upper_bp`
(9,800) decides whether every exception is worth a look. A glyph can force at
80% and say nothing here, and the reverse cannot happen, because 98% is inside
80%. Sharing one number would mean either flagging every lowercase word after a
glyph that capitalizes four times in five — thousands of rows — or refusing to
hold a 98% glyph to its own habit.

The denominator is the **cased** handoffs, the same one the table uses, so an
uncased script is judged here exactly as it is there: it decides nothing and
abstains. A glyph followed only by uncased letters has a denominator of zero.

Its sites are the exception's own word, not the glyph's run — the one other
channel whose span is not what matched it is `Doubled`, and for the same
reason. Which atom the lane credits, and why the site rule must match it
exactly, is [`sites.md`](sites.md).

## What is usual instead

A row says what is rare; `Pattern::usual` says what the corpus does instead,
read from the same counts the row was judged on.

```text
en_ulb   ',' prev=Space            1/54,723    usual=Letter 54,427
en_ulb   ''' then '.'              3/496       usual='"' 416, reversed 974  (`.'`, Closing)
en_ulb   '"' mixed len 4           4/12,014    usual=pure len 1 6,157
en_ulb   '’' rarity                2/4,112,852 usual='"' 12,046  (the Quote pool)
```

| channel | usual |
| --- | --- |
| `Placement` | the class most common on that side, `Edge` excluded |
| `ExactNeighbor` | the most common in-run follower, and the row's pair reversed, facing the same way |
| `RunShape` | the glyph's most common shape |
| `Rarity` | the most common other mark in the glyph's `Pool`; none for a letter, for `Pool::Other` (letters, spaces, unlisted marks), or an empty pool |
| `Casing` | the word's most common form in the same free positions the row counts |
| the rest | nothing: the row already says it |

Ties go to the smallest value. A word row's usual form is judged per key like
the row itself, so a kept verdict keeps it too. The wire lanes:
[`codec/README.md`](codec/README.md).

### Facing: a straight quote's direction, from where it sits

`"` and `'` (general category `Po` in `Pool::Quote`) do not say whether they
open or close, so a reversed count across one mixes openings with closings.
The run holding one records which way it faces (substrate.md), and the
reversal compares like with like:

```text
en_ulb GEN 48:20   Manasseh'."⏎    Letter before, Edge after     Closing
   ''' then '.'    3/496, all Closing                 facing Closing
   '.' then '''    974 Closing                        reversed 974
en_ulb NUM 21:14   \q "... Zahab   Edge before, Space after      Unknown
en_ulb JER 3:19    "my Father".'⏎  Letter before, Edge after     Closing
   '"' then '.'    2/129: 1 Unknown, 1 Closing        facing Unknown (a tie)
   '.' then '"'    4,036 Closing, 1 Unknown           reversed 1
```

| before the run | after the run | facing |
| --- | --- | --- |
| letter or digit | space or edge | `Closing` |
| space or edge | letter or digit | `Opening` |
| letter | letter | `Inside` |
| anything else | | `Unknown` |

- **The row's facing is its pair's majority**, counted over the runs holding
  the row's numerator. A tie is `Unknown`: split evidence names no direction.
- **`reversed` counts only runs facing that way.** A pair with no
  directionless quote has no facing and counts every reversal, as before.
- **A listed cluster carries its own majority facing**, `None` when it holds
  no directionless quote. RunShape recurrence still reads the atoms alone.
- **The numerator does not move.** Facing says which reversal is comparable;
  it is not a second key, so no row fires or goes silent because of it.

## `BookRate`: one book breaks from the rest

Placement pools every book, so a habit concentrated in one book hides under the
rest. The same keys, judged per book:

```text
nya   ',' prev=Space   pooled 1,417/46,382 = 3.1%            -> no Placement row
      1SA 1,183/1,526 = 77.5%    the other 61 books' median 0.31%   -> FIRES
nya   ',' next=Letter  PHP 115/124 = 92.7%                         -> FIRES
en    '—' prev=Space   JER 86/371 = 23.2%    under 40%: poetry     -> silent
en    '"' prev=Letter  2CO 6/46 = 13.0%      46 uses, under 100    -> silent
```

For every glyph, side and outer class (`Edge` excluded):

- **Judged books** hold the glyph at least `support_floor` times. The channel
  needs at least four (`BOOK_RATE_MIN_BOOKS`), else it says nothing. Every
  judged book sits in the others' median.
- A book's **rate** is its count of that class on that side over its own
  occurrences of the glyph.
- The **baseline** is the median of the OTHER judged books' rates, the mean of
  the two middle ones for an even count. Leaving the tested book out keeps a
  dominant book from pulling the baseline toward itself.
- A book **fires** when it holds the glyph at least `book_rate_min_uses`
  (100) times, its count reaches `support_floor`, its rate reaches
  `book_rate_min_bp` (4,000 = 40%), and its rate is at least
  `book_rate_ratio` (10) times `max(baseline, 1 bp)`.

A genre moves a book's rate by a few tens of percent — Jeremiah's poetry
dashes, Ezra's census numbers, Malachi's `," says` — and a habit moves most of
it. Over the committed tier the defaults fire nya 8 (1SA's space before `,`
`;` `?`, PHP's `,` straight into a letter), hin2017 1, and every other corpus
none (evidence.md).

The row is the book's own fraction: `numerator` its count, `denominator` its
occurrences, `books` 1, no band. `Usual::BookRate` carries the baseline and the
number of other judged books, and the key names the book, so two books breaking
the same way are two rows. The counts are raw: no leader subtraction, because
the other books are the comparison. Its sites are that book's occurrences, the
Placement rescan kept to the book the row names (`sites::firing` takes the
book's index). `channels.book_rate` turns it off.

## Dispersion

`Pattern::books` is how many Target books hold part of that row's numerator,
saturating at 255. Books-possible is the publication's own `book_count`;
nothing is stored for it.

On a word-pass channel it is recomputed from the aggregates at judge time
rather than carried through the tally, because which stored `Before`s count
toward a numerator is a config-dependent judging decision and a tally that
pre-summed them could not answer a re-judge.

On every channel but one it is **information, not a judgement**. Genre
clusters punctuation legitimately and a project's book set is not the engine's
business, so no threshold, flag, or squiggle reads it — a front end can say
"818 letter-attached commas, in 3 of 40 books" and leave the ruling to a
person. `BookRate` is the one channel that gates on dispersion: it compares
books against each other, so its rows always name one book.

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
`SentenceStart` is channel 9 and `BookRate` channel 10, the last of a glyph's
own rows, which is where appending them put them: neither is on the grain
ladder, so no order claim was disturbed. `BookRate` rows sort by side, class,
then book.
The word pass's channels are last, in channel order — `Casing`, `WordLength`,
`Doubled`, then `LetterRun` — each hash-ascending within itself, and
`LetterRun` letter-ascending. Their rows are a separate pass's, so they never
compete for a headline with a glyph's; `LetterRun` names a real scalar and is
still emitted there rather than beside that glyph's substrate rows, because
what produced it is the word walk. `ScalarKey` orders by code point with the pooled digit
lane last.

## One key at a time

A word channel's verdict reads one tally key and nothing else about the tally:
casing reads that word's rows plus the terminal table and the config, `Doubled`
reads its doubles row plus the table, the config and the corpus-wide recusal,
`LetterRun` reads its own row plus the config. So `judge_words_for` takes a
list of keys and emits exactly the rows a whole judge would emit for them — the
pure piece a resident host merges into the verdicts it kept from last time
(`galley/src/sous/expediter.md`). `WordLength` is the exception and is judged
whole: its ceiling is the corpus's own length distribution, so one word moving
moves every verdict.

## Where the row goes

`Findings::push_pattern` takes it; patterns are corpus-level, so they are
pushed either side of any `open_book`. The 36-byte wire row, the header's
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
