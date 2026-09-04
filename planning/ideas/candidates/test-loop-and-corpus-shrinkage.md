# Test loop: corpus shrinkage, and what the build cache already fixed

**State:** SUPERSEDED 2026-09-04 by `test-loop-shrinkage-plan.md`, which was
executed. Its measurement was `--release --include-ignored`; the loop an agent
actually pays is DEBUG, and the picture there was different by an order of
magnitude. Kept for the three instruments and the silent-skip finding, both of
which the plan carried forward. The numbers below are history.

The complaint is the inner loop with several agents on one laptop. Two
different costs were conflated in it — COMPILE time and TEST time. mbx
(a Cargo build cache, installed 2026-09-03) removed most of the first
and left the second untouched, so what remains is a test-data question,
not a tooling one.

## What the numbers say

`cargo test --release -- --include-ignored`, laptop is an M1 Max (10
cores, 32 GiB), worker is a shared 24-core/125 GiB Linux box:

| state | laptop | worker |
|---|---|---|
| warm no-op | 31.0 s | 45.9 s |
| empty target, warm mbx store | 45.5 s | 50.0 s |
| cold, no store | — | 1 m 42.6 s |

Two readings matter. First, **a fresh worktree now costs 45 s, not
minutes** — 149 cache hits, 0 misses, 0 B stored; mbx reports 12 m 35 s
of rustc intercepted. Cold builds, the thing remote compilation would
have fixed, mostly stopped happening. Second, **the laptop beats a
24-core box in every comparable state**, because once compiles are
served the suite is test EXECUTION, where M1 Max single-core wins and
where the worker's ext4 cannot reflink cache restores.

So the suite is ~46 s of which almost none is compiling, and it is
nearly SEQUENTIAL: the per-binary `finished in` times sum to ~45 s
against a 46 s wall. Cargo runs test binaries one at a time.

    lint_corpus              20.98 s   46%   ← one test inside it
    warmer                    4.39 s   10%
    equivalence               3.56 s    8%
    diff_corpus               3.41 s    7%
    substrate_reference       2.00 s    4%
    34 other binaries        ~11 s     25%

`every_corpus_fix_passes_the_oracle` alone is the 21 s: 226 books ×
(lex + build + lint + fix + re-lint). Everything else in the suite is
already cheap, INCLUDING the sweeps whose reputation is expensive —
`format_corpus` does 113 MB of edits in 1.02 s, `fast_path_identity`
runs in 0.08 s.

## The corpus, and what is actually in it

`testData/exampleCorpora` is 110 MB / 226 books, and en_ult is 99 MB of
it (67 books). Its size is word alignment: GEN alone carries 78,770 `\w`
and 23,020 `\zaln-s`/`\zaln-e` pairs. That is not redundant with
`testData/usfmtc` (28 MB of minimal and bizarre shapes) — it is the only
place with attribute-bearing `\w` nested in milestone pairs AT DEPTH.

Two things it is NOT costing us, contrary to the intuition that 99 MB of
one translation is waste:

- **Repo weight.** `.git` is 46 MB TOTAL — all history of everything —
  against a 139 MB `testData/`. Alignment markup compresses to nearly
  nothing. Clones are not paying for this.
- **Suite time**, except in the one test above.

What it IS costing is comprehensibility: one corpus is doing three jobs
(shapes, byte volume, real-world distribution) and no test says which
job it is asking for.

## The shrinkage idea (Will, 2026-09-03)

Anything that sweeps all four corpora — bdf_reg, en_ulb, en_ult,
examples.bsb — should instead read ONE representative corpus, shrunk by
running the current tests and keeping an exemplar of every case that
fires. Wild-shape coverage moves to fuzzing. Then the `#[ignore]` /
`--include-ignored` tier can go away entirely, and with a ~5 s suite,
neither nextest nor remote compilation has anything left to buy.

The reason this is defensible, and the fact that decides it: **the
wild-coverage job is already assigned elsewhere.** `corpora/calibration-corpora/`
is ~1500 bibles, swept deliberately as a bin/example with findings
recorded in the ledger. The committed tier never needed to be the
does-this-hold-everywhere instrument. Three instruments, three jobs:

    minimized corpus  →  shapes (fast, every run)
    fuzz targets      →  robustness: panics, roundtrip fixpoints
    calibration sweep →  real-world distribution (deliberate, recorded)

Cleaner than today, where one 110 MB corpus does all three and the tests
do not distinguish.

### Three corrections to the sketch

**Minimize on structural classes, not on what fires today.** "Keep what
current lint/format/diff/cst cases fire on" makes the corpus a function
of the present rule set: add a lint rule next month and there is no data
for it, and the suite goes green anyway. That is the silent-pass failure
mode below, promoted to a fixture-design principle. Minimize on INPUT
structure instead — unique (marker, nesting context, attribute shape)
tuples, plus boundary conditions: chapter/verse edges, milestone spans
crossing a chapter break, `\zaln`+`\w` nesting depths, non-BMP scalars
for the UTF-16 offsets. Those survive rule changes.

**Keep a volume anchor.** Some tests are not case-based: `format_corpus`
idempotence, `fast_path_identity`, `utf16_oracle`, the partition/fold
boundary oracles, galley's `warmer` and `equivalence`. Their claim is
"over this many real bytes the invariant holds," which a shape-minimized
corpus cannot express. Two or three full books of continuous real text
(MRK as the diff twin, PSA for poetry, one en_ult book for alignment
density) keeps it at ~6-8 MB instead of 110.

**The pinned numbers are evidence.** `lint_corpus`'s module doc is
explicit that every nonzero count is explained, not tolerated.
Regenerating against a new corpus is fine, but record the old numbers
and the corpus they came from in the evidence ledger first, or the
change destroys the ability to read a moving count as either data or
regression.

And scope the fuzzing honestly: `arbitrary` + `cargo-fuzz` over
lex→build→emit fixpoints and the USJ/USX round-trips finds panics and
roundtrip violations cheaply. It will NOT tell us a lint rule misfires
on real Hebrew poetry, because it does not generate scripture's
distribution unless we model it. Fuzzing replaces the wild-SHAPE claim,
not the real-text claim; calibration keeps the latter.

## Found while measuring: the oracle can pass while doing nothing

`onion/tests/lint_corpus.rs:352` (and `lint_corpus()` at :34) resolve the
corpus by relative path and return silently when it comes up empty:

    collect_usfm_paths(Path::new("../testData/exampleCorpora"), &mut paths);
    if paths.is_empty() {
        eprintln!("fix oracle SKIPPED: no *.usfm under testData/exampleCorpora/");
        return;
    }

Run the binary from anywhere but `onion/` and the largest single piece of
coverage in the suite passes in 0.01 s having tested nothing; cargo
swallows the `eprintln!` without `--nocapture`. Measured both ways: 21.14 s
from `onion/`, 0.01 s from the workspace root, both "ok. 1 passed".
`testData/` is committed, so there is nothing to be defensive about — this
is the trap CLAUDE.md names ("let it be absent and let it fail") sitting
inside the test that matters most. `assert!(!paths.is_empty(), …)`.

Independent of shrinkage, and worth fixing whether or not this idea ships.

## Options, ranked

1. **`#[ignore]` the fix oracle** (2 minutes, reversible). 46 s → ~25 s
   today; the gate still runs it under `--include-ignored`. A stopgap
   that costs nothing if shrinkage lands later, and the `assert!` fix
   rides along.
2. **Shrinkage** (a pass). 15 test files read `exampleCorpora`, several
   pin counts, and the minimizer itself has to be written — reduce while
   preserving structural coverage, which is delta-debugging with a
   coverage oracle, not a script. Ends with a ~5 s suite, no ignore
   tier, and fuzz targets for the shapes.
3. **cargo-nextest** (an afternoon). Runs tests across binaries in
   parallel processes; the ~11 s tail of 34 binaries collapses and the
   floor becomes the slowest single test. Predicted 46 s → ~23 s with
   zero code change. Caveat that does NOT apply here: nextest skips
   doctests, and this workspace has none — all 158 example fences are
   `text`-annotated, so nothing is lost. Worth having eventually for the
   many-small-binaries shape, but it is a parallel runner hiding a data
   problem if bought BEFORE shrinkage, and after shrinkage it may have
   nothing left to buy. Not now.
4. **Remote compilation** (deferred 2026-09-03). Reasoning kept in the
   table above: the offload target is slower than the laptop in every
   comparable state, mbx removed the cold builds that justified it, and
   the box is shared (9 users, a 66-day ClearML stack, load 2-3.4 before
   we touch it) so its speedscore is noisy CPU we do not control. Its
   remaining value is CPU RELIEF under agent concurrency, not latency —
   revisit only if several agents building at once is still the pain
   after the suite is seconds long.

## Open questions

- What is the coverage oracle for the minimizer? Marker×context tuples
  are enumerable from `LINT_ROWS` and the marker table; nesting depth and
  boundary classes are not, and picking them is the design work.
- Does the volume anchor belong in `exampleCorpora` or beside it, and do
  the sweep tests name it explicitly so the claim they make is readable?
- Do the shrunk fixtures get committed as USFM, or generated from a
  recipe? Generated keeps them honest against a corpus refresh; committed
  keeps the suite hermetic.
- `mise/` note: mbx's `.mbx.toml` now carries `incremental = false`, for
  reasons in `Cargo.toml` (CGU stability for benches, byte-compared wasm).
  Linker selection is machine config, NOT repo config — a committed
  `mold@…` selector broke the Linux worker outright (no `clang` to drive
  `-fuse-ld`) and would make CI fetch mold on every run.
