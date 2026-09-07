# Level 2 — word conventions

Word work starts only after Level 1 observations and judgment have proven the
common substrate. It uses words in free positions; forced positions are
classified from the learned terminal table rather than from a hard-coded
punctuation list. Convention-learned lane.

Status: **casing landed (W1); doubled words are W2.** Stage 4 in
[../roadmap.md](../roadmap.md); the walk, the row, and the forced rule are
[../core/src/words.md](../core/src/words.md), the channel is
[../core/src/judge.md](../core/src/judge.md).

## Approved directions

- **Casing convention:** compare a case-folded word's forms in free positions.
  Bivariant words abstain. Uncased scripts pay almost nothing and stay silent.
  **Landed as `Channel::Casing`.** "Almost nothing" is now measured: an uncased
  chapter's word row is 48 B against English's 5.3 KB, and its walk is faster
  than the substrate's (evidence.md, 2026-09-04).
- **Doubled words:** keep adjacent and punctuation-separated counters distinct.
  A recurring French `vous vous` convention can excuse itself; a lone residue
  remains reviewable. State crosses verse seams within a book.

## Blocked on calibration

Word-casing produced roughly eight times glyph-rule volume under shared bands
in the probe artifact. It needs an independently calibrated support floor or
band column before shipment; copying glyph defaults is blocked.

`JudgingConfig::word_support_floor` (5) and `word_bands` (the glyph staircase)
exist so the fleet run has something to sweep. They are **placeholders**, and
the doc line on `word_bands` says so. Nothing here is a shipped default until
that sweep runs.

The word rule itself is measured: over the 1,504-corpus fleet it agrees with
UAX #29 on 98%+ of words in every spaced script, and on almost none in Thai,
Lao, Khmer, Burmese, Han, Kana, and Tibetan — where neither rule works without
a dictionary and the word lanes abstain. Hebrew's 8.7% is the maqaf, which
UAX 29 splits and this rule joins. Per-script rates: evidence.md, 2026-09-04.

## Idea shelf, not approved rules

- character n-gram surprisal;
- hapax-rate context;
- word length against the corpus distribution;
- compound/split comparison against the corpus vocabulary.

These remain probes until each can state a narrow claim, fair comparison
population, counterexamples, and actionable result. "Not in the vocabulary" is
never sufficient; names, loanwords, and productive morphology are standing
counterexamples.
