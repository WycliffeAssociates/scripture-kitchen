# Perf notes (rough)

2026-08-21, Apple M1 Max, 10 cores, 32GB. These are `playground` numbers —
max-of-4 runs, no criterion, no statistics beyond "ran it a few times, took
the best." Not a benchmark suite. Refresh by rerunning the exact command
printed with each row. Build once: `cargo build --release --bin playground`
(add `--features par` for the par rows).

## 1. The keystroke answer

Question: is it feasible to re-lex + re-CST + re-lint a whole document on
every keystroke? Budget: 16ms/frame for 60fps; people type at roughly
5-15 keys/sec (66-200ms between keystrokes).

Full pipeline = lex + cst::build + lint (mode `--lint`), single file, best of 4:

| Scope | Corpus | Time | vs 16ms frame | Command |
|---|---|---|---|---|
| Median book (Zechariah, 35KB, 14 ch) | en_ulb (unaligned) | 0.044ms | ~363x headroom | `playground --lint --iters 200 example-corpora/en_ulb/38-ZEC.usfm` |
| Largest book (Psalms, 273KB, 150 ch) | en_ulb (unaligned) | 0.747ms | ~21x headroom | `playground --lint --iters 200 example-corpora/en_ulb/19-PSA.usfm` |
| Median book (Ecclesiastes, 747KB, 12 ch) | en_ult (word-aligned) | 1.81ms | ~8.8x headroom | `playground --lint --iters 30 example-corpora/en_ult/21-ECC.usfm` |
| **Largest book (Genesis, 5.15MB, 50 ch)** | **en_ult (word-aligned)** | **12.18ms** | **~1.3x headroom** | `playground --lint --iters 30 example-corpora/en_ult/01-GEN.usfm` |
| Per-chapter (derived: Psalms/150) | en_ulb | 0.005ms | ~3200x | book time ÷ chapter count |
| Per-chapter (derived: Genesis/50) | en_ult | 0.244ms | ~66x | book time ÷ chapter count |

**Interpretation:** whole-book-per-keystroke is comfortably feasible for
anything shaped like the unaligned corpus (en_ulb) — even the largest book,
Psalms, eats only ~5% of a frame. On the word-aligned stress corpus
(en_ult), the median book is still fine (~8.8x headroom) but the largest
book, Genesis with its alignment markup, spends ~12ms of a 16ms frame —
that's inside budget but leaves little room for anything else the UI does
that frame (layout, paint, other JS), so whole-book-per-keystroke on the
largest en_ult book is *not* comfortably feasible, just barely possible.
Chapter-scoped re-processing (the TOC/chunk direction) sidesteps this
entirely: ~66x cheaper than the whole book on Genesis, ~3200x on Psalms —
chapter-scoped re-lint is not a close call at any corpus size measured here.

## 2. Per-stage cost (ns/token, MiB/s), whole corpus, best of 4

| Stage | en_ulb (4.5MB, 255K tok) | en_ult (103MB, 6.57M tok) | Command (en_ulb shown; swap path for en_ult) |
|---|---|---|---|
| lex only (serial) | 1616 MiB/s | 985 MiB/s | `playground --iters 20 example-corpora/en_ulb` |
| parse-header (lex+index) | 1446 MiB/s | 926 MiB/s | `playground --parse-header --iters 20 example-corpora/en_ulb` |
| cst (lex+build, full) | 1061 MiB/s | 695 MiB/s | `playground --cst --iters 20 example-corpora/en_ulb` |
| cst-only (build alone) | 3166 MiB/s | 2288 MiB/s | `playground --cst-only --iters 20 example-corpora/en_ulb` |
| lint (full: lex+cst+lint) | 24.8 ns/tok, 679 MiB/s | 37.2 ns/tok, 402 MiB/s | `playground --lint --iters 20 example-corpora/en_ulb` |
| lint-only (lint alone) | 9.5 ns/tok, 1774 MiB/s | 16.2 ns/tok, 925 MiB/s | `playground --lint-only --iters 20 example-corpora/en_ulb` |
| usj-only (serialize) | 134 ns/tok, 125 MiB/s | 125 ns/tok, 120 MiB/s | `playground --usj-only --iters 20 example-corpora/en_ulb` |
| usx-only (serialize) | 163 ns/tok, 103 MiB/s | 124 ns/tok, 121 MiB/s | `playground --usx-only --iters 20 example-corpora/en_ulb` |

`--html-only` didn't exist on this build (concurrent HTML-export work in
flight) — skipped, note for a rerun once it lands.

USJ/USX serialization is notably heavier per token (~5x lint-only) — worth
remembering if a caller wants live USX/USJ preview, not just lint, on every
keystroke.

## 3. Scale: whole-corpus wall time, serial vs `--par`

| Corpus | Serial (lex only) | `--par` (lex only, rayon over books) | Command |
|---|---|---|---|
| en_ulb (4.5MB, 66 books) | 2.66ms | 0.66-0.68ms (~4x, core-bound) | `playground --par --iters 20 example-corpora/en_ulb` (needs `--features par`) |
| en_ult (103MB, 67 books) | 100ms | 15.1-15.8ms (~6.5x) | `playground --par --iters 4 example-corpora/en_ult` |

`--par` here only parallelizes lex, not the full pipeline — treat as "what
does core count buy you," not a full-pipeline number.

No Hindi/non-ASCII-density corpus present under `example-corpora/` at time
of writing (only `bdf_reg`, `en_ulb`, `en_ult`, `examples.bsb`; `bdf_reg` is
plain ASCII) — skipped the non-ASCII density case, flag if one gets added.

## 4. Shapes to remember

- en_ulb: ~17.7 bytes/token, ~58 tokens/KB of USFM source.
- en_ult (word-aligned): ~15.7 bytes/token, ~65 tokens/KB — denser in tokens
  per byte because of the alignment attribute markup, not because the prose
  changed.
- Lex-only prose throughput: **~1.6 GiB/s on en_ulb** (established fact was
  "~1.7 GiB/s" — close, within run-to-run noise, not a real drift).
  en_ult lex-only came in at ~985 MiB/s (word-aligned markup is heavier per
  byte, as expected).
- Full staged pipeline (lex+cst+lint): **~24.8 ns/token on en_ulb, ~37.2
  ns/token on en_ult** — matches the established "~25 ns/token en_ulb /
  ~38 ns/token en_ult" figures, no drift.
