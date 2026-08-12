# Measured marker frequencies (2026-08-10)

Source for the `priority` column / `common_marker_checks` membership.
Method: `grep -oE '\\[+]?[a-z][a-z0-9]*(-[se])?\*?'` over each corpus,
canonicalized (leading `\`/`+` stripped, trailing `*` stripped, `-s`/`-e`
stripped, trailing digits stripped), openers+closers pooled. Prose corpora
only per ruling — aligned (en_ult) deliberately excluded; we don't work
aligned data right now.

| rank | marker | en_ulb | examples.bsb | combined | note |
|---|---|---|---|---|---|
| 1 | v | 31,102 | 31,084 | 62,186 | |
| 2 | q | 23,080 | 24,104 | 47,184 | near-v-level: poetry is huge |
| 3 | p | 6,204 | 12,251 | 18,455 | |
| 4 | s | 13,636 | 3,095 | 16,731 | ulb-heavy |
| 5 | f | 780 | 9,704 | 10,484 | footnote family dominates bsb |
| 6 | b | 1,961 | 3,953 | 5,914 | |
| 7 | ft | 395 | 5,022 | 5,417 | |
| 8 | fr | — | 4,852 | 4,852 | |
| 9 | xt | — | 3,154 | 3,154 | bsb only |
| — | c | 1,189 | 1,189 | 2,378 | ~4% of v — does NOT pay for an arm |
| — | fqa | 1,074 | 246 | 1,320 | |
| — | li | — | 1,537 | 1,537 | borderline |
| — | r | — | 1,322 | 1,322 | |
| — | m | 436 | ~400 | ~850 | |

Cut for `common_marker_checks`: top 9 (v q p s f b ft fr xt), ~85%
cumulative coverage. `c` explicitly excluded (frequency, not shape — it
shares `\v`'s digit shape but occurs once per chapter). Re-measure when a
corpus that represents real editing load exists; arms are added one at a
time, measured, per NEXT-STEPS step 4.
