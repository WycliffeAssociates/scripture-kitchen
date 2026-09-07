# Level 2 — word conventions

Word work starts only after Level 1 observations and judgment have proven the
common substrate. It uses words in free positions; forced positions are
classified from the learned terminal table rather than from a hard-coded
punctuation list. Convention-learned lane.

Status: **casing landed and calibrated (W1, W3); doubled words are W2.**
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
