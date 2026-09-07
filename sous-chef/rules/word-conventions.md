# Level 2 — word conventions

Word work starts only after Level 1 observations and judgment have proven the
common substrate. It uses words in free positions; forced positions are
classified from the learned terminal table rather than from a hard-coded
punctuation list. Convention-learned lane.

Status: **casing landed and calibrated (W1, W3); doubled words landed and
calibrated (W2).**
Stage 4 in [../roadmap.md](../roadmap.md); the walk, the row, and what stands
before a word are [../core/src/words.md](../core/src/words.md), the channels
and the terminal table are [../core/src/judge.md](../core/src/judge.md).

## Approved directions

- **Casing convention:** compare a case-folded word's forms in free positions.
  Bivariant words abstain. Uncased scripts pay almost nothing and stay silent.
  **Landed as `Channel::Casing`, on by default.** "Almost nothing" is measured:
  an uncased chapter's word row is 48 B against English's 5.3 KB, and its walk
  is faster than the substrate's (evidence.md, 2026-09-04).
- **Forced positions are learned, not listed.** Which punctuation a corpus puts
  a capital after is a fact about that corpus, so the row stores the glyph that
  stood before each word and the judge asks a `TerminalTable` built from the
  substrate's own follow lane. A glyph forces at
  `terminal_upper_share_bp` (8,000 = 80%) of its cased handoffs. Quotes and
  brackets are transparent, so `he said, "Stop` records the comma and the
  comma's own share decides — 1,022 bp in en_ulb, so `Stop` stays reviewable
  there, and a corpus that reports speech after a comma everywhere abstains
  instead (evidence.md, W3).
- **Doubled words:** keep adjacent and punctuation-separated counters distinct.
  A recurring French `vous vous` convention can excuse itself; a lone residue
  remains reviewable. State crosses verse seams within a book.
  **Landed as `Channel::Doubled`, on by default.** Two keys, never pooled, each
  judged against the word's own occurrences: `vous vous` x300 against `vous`
  x9,000 is 3.3% against band 3's 0.1% and stays silent, while one `the the`
  against 60,000 `the`s fires. A word doubled every time it appears owns its
  whole denominator and never fires, so there is no allow-list. State crosses a
  verse seam and stops at a chapter one, which is the same rule the fold
  already runs for a word.
- **Doubling has nothing to do with case, so uncased scripts are judged too.**
  That is the one place they stop paying nothing: the walk now hashes every
  word and an uncased chapter keeps one doubles row per distinct word. Measured
  (evidence.md, W2): amh 1.09 MB of chapter rows and hin2017 4.96 MB against ~0
  before, while a cased Bible's doubles lane is 1-17 KB against megabytes of
  casing rows. hin2017 fires 14 rows for it; amh fires none.
- **Productive reduplication recuses the corpus, not the word.** If more than
  `doubles_productive_bp` of the vocabulary appears doubled twice or more,
  doubling means something in this language and the channel abstains for the
  whole corpus. A share and never a count, so Jonah and a whole Bible answer the
  same way. `doubles: Auto | Always | Never` is the override.

## Calibrated on the fleet

Word casing fires at **twenty times** glyph-rule volume under shared bands —
p50 201 rows per corpus against the glyph channels' p50 10, over all 1,504
fleet corpora (`core/examples/word_volume.rs`, evidence.md W3). The glyph
staircase at **a tenth of every share** puts it at p50 11 / p90 31 / p95 42,
which is the glyph channels' own volume, so:

- `JudgingConfig::word_bands` = `Staircase::WORD_STEPS`, 250 / 100 / 30 / 10 /
  3 basis points;
- `JudgingConfig::word_support_floor` = 20 — the sweep shows 5, 10, and 20
  identical at that ladder, because its two lowest rungs cannot fire at all;
- `channels.casing` = **true**, since that volume holds. That was the
  condition, and it is met.

Doubled words are calibrated on the same fleet
(`core/examples/word_volume.rs`, evidence.md W2):

- volume at the shipped word ladder is **p50 10 / p90 29 / p95 35 / max 81**
  rows per corpus, which is the glyph channels' own volume, so
  `channels.doubled` = **true**;
- the vocabulary share that doubles has **no knee**: p50 20 / p90 91 / p95 132
  basis points over 1,504 corpora, maximum 511. `doubles_productive_bp` = **300**
  recuses the 8 most reduplicating corpora (0.5% of the fleet — kms, djkNT,
  kmh-m, urim, nii, urbNT, urt, yss-yawu, all Papuan or creole); 500 would have
  recused exactly one, and 100 would have recused 8.5%, well inside the ordinary
  body of the distribution. The recusal is a correctness guard and not a volume
  control: even those corpora fire only 13-51 rows, because the band already
  excuses a word that doubles often.

The word rule itself is measured: over the 1,504-corpus fleet it agrees with
UAX #29 on 98%+ of words in every spaced script, and on almost none in Thai,
Lao, Khmer, Burmese, Han, Kana, and Tibetan — where neither rule works without
a dictionary and the word lanes abstain. Hebrew's 8.7% is the maqaf, which
UAX 29 splits and this rule joins. Per-script rates: evidence.md, 2026-09-04.

## Idea shelf, not approved rules

- character n-gram surprisal;
- hapax-rate context;
- compound/split comparison against the corpus vocabulary.

These remain probes until each can state a narrow claim, fair comparison
population, counterexamples, and actionable result. "Not in the vocabulary" is
never sufficient; names, loanwords, and productive morphology are standing
counterexamples.

**Word length left the shelf as an off-by-default channel.**
`Channel::WordLength` names a word standing `word_length_sigma` (4) whole
standard deviations above the corpus's own occurrence-weighted mean word
length. It is a deviation from the rule above rather than an exception to it:
the standing counterexamples are exactly why `channels.word_length` ships
**false**. It exists because the length is already in the row, and because a
corpus whose typography has run words together has no other lane that sees it.
It abstains in uncased scripts, where a word is in no row at all.
