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

## 5. wasm boundary (Node, 2026-08-21)

Question: what does `analyze(source)` cost AT THE WASM BOUNDARY in Node —
the "UTF-8 bytes cross into wasm once" line. Same M1 Max, node v24.4.1.

Scaffolding lives outside the repo (a scratchpad `cdylib` with a plain
`extern "C"` ABI — no wasm-bindgen, no glue, so the numbers are the boundary
itself). `analyze` = `lex` + `cst::build` + `lint` + `toc`, returning a
checksum so nothing folds away; the wasm and native builds return identical
checksums. Both are `opt-level=3, lto=true, codegen-units=1`. `min / median`
of 200 iterations after 50 warmup.

| Stage (µs, min/median) | Median book (ECC, 34.6KB) | Largest (PSA, 273KB) |
|---|---|---|
| `TextEncoder.encode` (new array) | 71.7 / 81.6 | 531 / 563 |
| `encodeInto` straight into wasm memory | 37.8 / 39.0 | 296 / 296 |
| copy only (`.set` of pre-encoded bytes) | **0.5 / 0.6** | **4.4 / 4.5** |
| `utf8_check` in wasm (`from_utf8`) | 2.5 / 2.6 | 19.7 / 19.8 |
| `lex_only` wasm, no simd | 42.8 / 43.3 | 455 / 459 |
| `lex_only` wasm, `+simd128` | 39.4 / 39.8 | 416 / 427 |
| `lex_only` native | 21.3 / 23.2 | 251 / 256 |
| `analyze` wasm, no simd | 115.3 / 119.5 | 1220 / 1247 |
| `analyze` wasm, `+simd128` | 109.8 / 112.5 | 1175 / 1204 |
| `analyze` native | 79.1 / 87.3 | 778 / 798 |

Ratios (min, simd wasm ÷ native): **lex 1.85x / 1.66x, analyze 1.39x /
1.51x**. Whole corpus, en_ulb (66 books, 4.5MB): encode-all 9.11ms, wasm
`analyze` 10.3ms (simd) / 11.1ms (plain), wasm `lex` 4.3ms (simd) / 5.0ms;
native `--lint` 6.59ms, native lex 2.59ms.

Refresh: the probe crates are scratch, not repo files — rebuild with
`cargo build --release --target wasm32-unknown-unknown` and again under
`RUSTFLAGS="-C target-feature=+simd128"`, drive both with
`WebAssembly.instantiate(fs.readFileSync(...))` and `process.hrtime.bigint()`.
Native side: `playground --lint --iters 200 <file>` (that mode has no `toc`
pass, so the table's native column comes from a matching scratch binary
running the identical `analyze` body).

**Interpretation.** The copy is free and the encode is not. Moving already-
encoded bytes into wasm memory runs at tens of GB/s — 4.5µs for all of
Psalms, noise against a 1.2ms analyze. But JS holds a UTF-16 string, and
turning it into UTF-8 costs ~1 GB/s in V8: 296µs for Psalms, which is a
quarter of the analyze time, and 38µs for a median book, a third of it.
`encodeInto` (writing straight into wasm memory) is 1.8x cheaper than
`encode` because it skips the allocation, and it shows no penalty for
targeting wasm memory — so it is the call to use. Non-ASCII density does not
matter here (the one-byte vs two-byte V8 string representations timed within
noise); the encoder is simply not fast. The real escape is not encoding at
all on every keystroke: keep the editor's buffer as bytes, or feed `edit.rs`
incrementally. Even paying it in full, median-book `encodeInto` + `analyze`
is ~148µs — still ~100x under a frame.

wasm costs 1.4-1.9x native, worse on the lexer than on the whole pipeline —
which is the expected shape, since lex is the memchr-bound stage and native
gets NEON where wasm gets, at best, simd128. And `+simd128` is worth having
but is not the story: ~8% off lex, ~4-5% off `analyze`, consistently, at both
sizes. It buys nothing on `from_utf8` (core's validator has no wasm SIMD
path). Turn it on — one RUSTFLAG for a free 8% on the hot stage — but do not
expect it to close the gap to native.
