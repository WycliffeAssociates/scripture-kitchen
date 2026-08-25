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

## 6. Format (2026-08-24, same machine)

`format_edits` = lex + cst + lint (harvest) + the Form pass. Best of 20,
release, from the `--format-trace` headers (refresh: the exact commands in
debug/formatting/*.txt, or `playground --format-trace <file>`):

| Scope | Edits | Time | Command |
|---|---|---|---|
| Jonah (7.4KB), default | 71 | 45.5µs | `playground --format-trace example-corpora/en_ulb/32-JON.usfm` |
| Psalms (273KB), default | 3,971 | 1.5ms | `playground --format-trace example-corpora/en_ulb/19-PSA.usfm` |

Shape to remember: format ≈ 2x the `--lint` pipeline on the same book
(PSA 0.747ms lint vs 1.5ms format_edits) — the harvest IS a full lint run,
the Form pass and the claims resolve are the rest. Still ~10x under a frame
on the largest book.

## 7. Diff: onion's measured baseline (pre-port, from onion's own profiling)

No diff code here yet — these are the numbers the port must beat, recorded
from ../usfm_onion's profiling memory (diff-perf-sid-stringification,
2026-07-24) before porting:

- 2-way diff of a book: ~ms scale, and ~100% of it allocation/String cost —
  per-token `format!` sid strings, `to_vec` token clones, `text_full`
  concatenation. Myers itself: ~62 of ~40k samples ("free").
- Its rayon by-chapter gate sat at ≥20k combined tokens; the parallel
  correctness oracle was byte-equality with a serial reference over a
  263k-token book.
- The port's anchor cut (Toc verse anchors -> byte-range blocks, Copy Sid
  pairing keys) removes the entire measured cost class by construction: no
  String sids, no token clones, no owned text anywhere in the skeleton.
  Onion's ms-per-book is therefore the FLOOR, not the target; measure at
  port time per the go-slow law (no rayon — parallelism deferred to
  galley (the workflows crate)).

### Measured after the port (2026-08-24, same machine)

`diff` = lex + toc PER SIDE + blocks + Myers + coalescing + the classifiers.
`diff_with_text` adds a CST + a `Filter::reader_text` mask per side and the
UAX-29 word diff per changed unit. Best of 20, release, from the
`--diff-trace` headers (refresh: `playground --diff-trace <a> <b>`, dumps in
debug/diff/):

| Pair | Units | `diff` | `diff_with_text` |
|---|---|---|---|
| en_ulb MRK vs en_ult MRK (84KB vs 2.4MB, aligned) | 695 (679 modified) | **3.0ms** | 32.6ms |
| en_ulb PSA vs itself (273KB, the largest book) | 2,612 (all unchanged) | **2.2ms** | 3.0ms |
| en_ulb MRK vs itself (84KB) | 695 | 552µs | 581µs |
| en_ulb JON vs bdf_reg ROM (7KB vs 70KB, all-added) | 500 | 587µs | 696µs |

Shape to remember: lex + toc is HALF of it (PSA 0.505ms/side ⇒ ~1.0ms of the
2.2ms), and the rest is one `String` unit id per BLOCK — 2,612 of them for
PSA, where onion allocated one sid `String` per TOKEN (30,892). The
alignment itself is free: Myers over `Copy` addresses short-circuits on an
identical pair, and the byte-range units mean no text is ever copied.
Onion's ms-per-book floor is met on the largest book and beaten on a median
one; the text-diff layer is the expensive one (the CST + mask + a word diff
per modified unit — 32.6ms on a 2.4MB aligned side) and it is opt-in.

Replay is minimal, not per-unit: 679 modified units in the MRK pair emit 17
`SpliceEdit`s, because adjacent changed blocks coalesce into one splice and
an unchanged block costs none (a book against itself is ZERO edits).

## 8. analyze: the editor wire (2026-08-24, same machine)

`analyze(text, wants::ALL, None)` = lex + cst + lint + toc + the reader-text
mask + the emit of all seven reads + the UTF-16 conversion of every offset in
them. Best of 5 runs of 20 iterations, release, `playground --analyze` (per-read
rows: `--analyze-wants <mask>`).

| Book | `--serial` (lex) | `--lint` | `--analyze` (ALL) | analyze ÷ lint |
|---|---|---|---|---|
| en_ulb ZEC (35KB, median) | 0.020ms | 0.058ms | **0.201ms** | 3.5× |
| en_ulb PSA (273KB, largest) | 0.331ms | 0.993ms | **3.08ms** | 3.1× |

Per read, PSA, each bit ALONE (so each row includes the lex it cannot avoid,
0.33ms, and the artifact it needs):

| bit | read | PSA | what it pays for beyond lex |
|---|---|---|---|
| 1 | `chapters` | 0.51ms | toc |
| 2 | `blocks` | 0.94ms | cst + a node sweep + a sorted-permutation conversion |
| 4 | `note_extents` | 0.64ms | cst (few notes in Psalms) |
| 8 | `token_spans` | 1.10ms | 30,892 rows = 371KB of u32, and their conversion |
| 16 | `text_runs` | 1.16ms | cst + the reader-text mask |
| 32 | `verse_anchors` | 0.57ms | toc |
| 64 | `diagnostics` | 1.13ms | cst + lint |

Shape to remember: **analyze ≈ 3× the `--lint` pipeline**, and the extra 2× is
not the conversion — it is the two artifacts lint does not build (toc, mask)
plus `token_spans`, which is a third of a megabyte of u32 on the largest book.
The UTF-16 wall itself is the cheap part: `token_spans` alone is 1.10ms against
a 0.33ms lex, so ~0.77ms buys 92,676 offsets emitted AND converted — the
ascending fast path is one SWAR sweep of the document, no index, no allocation
beyond the read.

Against the sketch's envelope (engine ×2–3 under wasm, 150ms debounce): PSA at
3.08ms native is ~6–9ms in the browser with every read on, and the reads an
editor actually renders per commit (chapters + blocks + verse anchors +
diagnostics, no `token_spans`, viewport-clipped where it wants spans) are well
under half of it. `wants` is doing exactly the job it was designed for — the
1000× cost spread the sketch predicted shows up as a 2× spread here only
because Psalms is prose; on an aligned book (en_ult, 31MB of `\w` interiors)
`token_spans` is the read that would dwarf the rest.

en_ult PSA — the aligned stress case (5.1MB, 340,639 tokens; word-level
alignment inflates 273KB of scripture 19×). avg of 20, release:

| wants | reads | time |
|---|---|---|
| 1 | chapters only (lex+toc) | 6.6ms |
| 64 | diagnostics (lex+cst+lint) | 14.0ms |
| 99 | the editor per-commit set (chapters+blocks+verse_anchors+diagnostics) | 16.5ms |
| ALL | everything incl. token_spans + mask | 25.8ms |

(`--lint` alone on this file: 12.7ms — the pipeline, not the emit, is the
cost at this size.) ×1.4–1.9 measured wasm ⇒ per-commit set ≈ 23–31ms in
the browser on the single worst file in the corpus — inside the 150ms
debounce, at/over one frame (the vision's §9.7 names 20–30ms "potentially
acceptable"; the content-addressed rung is the ladder if real UI says no).
Note the editing target is a translator's DRAFT (en_ulb-class, 3ms);
aligned corpora are read-only references that don't re-analyze per
keystroke.

MEASURED wasm (2026-08-24, Node = V8, onion-wasm pkg-node build, no +simd128,
best of 20, each call includes the string encode IN and one read copy-out):

| Book | ALL | commit-set (wants=99) | vs native ALL |
|---|---|---|---|
| en_ulb ZEC | 0.23ms | 0.16ms | 1.14× |
| en_ulb PSA | 3.36ms | 2.14ms | 1.09× |
| en_ult PSA | 41.8ms | 30.2ms | 1.62× |

The §5 multiplier (1.4–1.9×) only bites where LEXING dominates (the 5MB
aligned file — memchr gets simd128, not NEON, and this build had simd128
OFF: ~8% free there). Prose books run near-native: the ×2–3 envelope was
pessimistic and the boundary costs are noise. The one number to watch:
30ms commit-set on the worst aligned file — the vision's "20–30ms,
observe real UI" band; mitigations ranked: aligned corpora are read-only
references (no per-keystroke re-analysis), +simd128, then the
content-addressed rung. Rebuild harness: `wasm-pack build --target
nodejs --release -d pkg-node` in onion-wasm/, then a small node script
timing analyze() per book.

### Pass 8: what the widened wire cost, and what it bought (2026-08-24)

`analyze` gained two reads (`lines` stride 4, `note_parts` stride 4), two
widened ones (`chapters` 5→7, `verse_anchors` 3→5) and one scalar
(`usfm_version`). Measured in the CM spike's browser probe on John (113KB,
`onion-2-spike/PERF.md`):

| | pass 7 | pass 8 |
|---|---|---|
| `analyze-commit` (whole book, per keystroke) | 0.74ms | 0.93ms |
| the editor's JS projection over it | 1.06ms | 0.81ms |
| …of which: rebuilding the line model | ~0.9ms | **~0.16ms** |
| …of which: a token pass for inline decorations | — | 0.70ms |

So the engine took ~0.19ms to delete ~0.74ms of JS. The keystroke median did
not move (7ms on John, both passes) — at these sizes the commit is dominated
by CodeMirror's own update and the decoration build, and BOTH sides of the
wall together are under a third of it. The line read's real payoff is that
whole-book `TOKEN_SPANS` is no longer a structural requirement, which makes
that read clippable for the first time.
