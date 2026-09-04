# Plan: the whole suite under 10 s

**State:** first draft, 2026-09-04. Supersedes the "options, ranked" and the
fuzzing scope in `test-loop-and-corpus-shrinkage.md`; keeps its three
instruments and its silent-skip finding.

## Correct the measurement first

The candidate measured `--release --include-ignored`. Agents run plain
`cargo test`, which is DEBUG, and the picture is different by an order of
magnitude (M1 Max, warm target, mbx serving compiles):

    warm `cargo test --no-run`        13 s     (compile is NOT the problem)
    `cargo test` default tier, wall  294 s

    lint_corpus         223.1 s   ← every_corpus_fix_passes_the_oracle, not ignored
    format_corpus        13.9 s
    mask_oracle          11.5 s
    atom_conformance      7.8 s   (sous-core; reads no corpus — profile it)
    html_corpus           5.6 s
    everything else      <2 s each, ~35 binaries

So the ignore tier is not what an agent waits on. It waits on four
binaries that sweep 110 MB unoptimized, and one of them is 75 % of the
wall. Every step below is a measured subtraction from 294 s.

## Target

`cargo test` (debug, default, warm) under 10 s wall for the whole
workspace. That is the LOCAL gate a builder pays per pass. A small,
justified `#[ignore]` tier survives for claims that are unique and
inherently corpus-scale; CI runs it per PR in release, as today, so no
coverage depends on a scheduled job. Plus `cargo clippy --all-targets`
clean.

Three instruments, unchanged from the candidate, but the first two are
what the suite runs and the third is never a `#[test]`:

    minimized corpus + usfmtc  →  shapes            (every run, < 10 s)
    volume anchor: examples.bsb →  byte-scale laws   (every run, part of the 10 s)
    calibration sweep           →  distribution      (bin/example, ledger-recorded)

Fuzz/proptest: not now. The library is still moving; add `cargo-fuzz`
targets over lex→build→emit and the USJ/USX round-trips once the wire and
CST settle. Record as a follow-up candidate, not a step here.

## Steps

**0. Freeze the evidence before touching bytes.** Run
`playground --lint-stats testData/exampleCorpora` and the other pinned
oracles once against the current 226 books; copy the counts and the
corpus manifest into `evidence.md` / `planning/choices.md`. After this
the old numbers are history, not a lost baseline.

**1. Two cheap levers, measured before the data work.**
- `[profile.test] opt-level = 1` (or `[profile.dev.package."*"]`).
  `debug_assert!` stays on; the sweeps get most of release's speed. If it
  buys 3× on the four fat binaries it changes what step 3 has to cut.
- Kill the silent skips: every `if paths.is_empty() { eprintln!; return }`
  becomes `assert!`, and paths resolve via `CARGO_MANIFEST_DIR`, not cwd.
  Rides along regardless.

**2. Move en_ult out of the test tier; keep it in the repo.** 99 MB of
110, and it stays: benches and stress runs want alignment density at real
volume. But no `#[test]` reads it. The boundary is a DIRECTORY, the same
pattern as `corpora/` vs `corpora/calibration-corpora/`:

    testData/exampleCorpora/        test tier: bsb, and what step 3 leaves of ulb/bdf
    testData/stressCorpora/en_ult/  benches, playground, stress — never a #[test]

`exampleCorpora` sweeps stop seeing it with no filter logic to get wrong.
The one book a test names, `en_ult/42-MRK.usfm` (diff twin, aligned `\w`
inside `\zaln` at depth), is copied into the test tier as a fixture.
Benches repoint to `stressCorpora`. Predicted: lint_corpus 223 s → ~15 s,
format/mask/html proportionally; default tier lands near 30 s.

**3. Name the job of every corpus reader.** 15 test files read
`exampleCorpora`; each gets classified in a one-line module doc and moved
to the instrument it belongs to:
- *shapes* → `testData/usfmtc` (28 MB, already the weird-shape encoder)
  plus excerpted fixtures for the pinned quirks (en_ulb ISA/MRK truncated
  `\f`, ZEC 12:7 `\v 7"`, bdf_reg ROM 3 double `\v 10`, ACT 8:17 `\v +`,
  LAM 2 missing `\v 1`, the `\s5` chunk idiom). Once excerpted, en_ulb
  and bdf_reg as WHOLE corpora are no longer load-bearing and can shrink
  to the books the fixtures came from.
- *volume* → `examples.bsb` only (4.7 MB, plain USFM, wide marker range,
  no alignment). `format_corpus` idempotence, `fast_path_identity`,
  `utf16_oracle`, partition/fold laws, galley `warmer`/`equivalence`
  sweep this and say so.
- *the eleven `#[ignore]`d tests* (diff_corpus, fold, utf16, warmer ×3,
  equivalence ×4, hygiene, substrate, mise utf16) each get ONE of three
  dispositions, written in the file:
  - *redundant*: the same law is already proven on synthetic small-scale
    input and the corpus run only repeats it at volume → delete.
  - *unique, cheap after shrink*: drop `#[ignore]`, run every time over
    the anchor.
  - *unique, inherently corpus-scale* (50 cold whole-Bible publications,
    the whole-corpus fold law): keep `#[ignore]`, keep it a real test.
    CI's release leg runs these per PR. Aim for zero, accept a handful.
  `#[ignore]` is not the smell; today's tier is, for three reasons this
  step removes: it is a dumping ground for bulk reruns of synthetic laws,
  the LOCAL gate required it, and the reason strings say when to run
  ("pass end") instead of what the test proves that nothing else does.
  Every survivor's reason string names its unique claim. If the count
  creeps back past a handful, it is a smell again.
  Distribution sweeps over `corpora/calibration-corpora/` are never
  tests: they are examples with a ledger row, run by hand or on a
  schedule.
Pinned counts in `lint_corpus` are regenerated against the new corpus and
recorded next to the step-0 numbers.

**4. galley yes, sous-chef not yet.** galley's `warmer` and `equivalence`
read the anchor like everything else in step 3. sous-core's
`substrate_reference` and `hygiene_scalar_reference` walk all 8
`corpora/*.txt` (35 MB), and they are NOT in the debug top five today
(their exhaustive halves are the `#[ignore]`d ones step 3 already
converts). Sous works cross-book and calibrates against a whole corpus,
so some of those reads are load-bearing and some are not, and which is
which is not obvious yet. Leave the 8-corpus reads alone, re-measure after
steps 1–3, and only then decide per test whether it wants the full read
or a chapter slice. `atom_conformance` (7.8 s, no corpus) walks the
whole code point table through the classifier: pure debug-mode
arithmetic, so step 1's `opt-level = 1` is the first thing to check
before anyone profiles it.

**5. Binary count: compile cost, not just startup.** ~35 integration
binaries each link the engine, and `cargo clippy --all-targets` compiles
every one of them; a builder pays that at least twice per pass. Folding
onion's `tests/*.rs` into one binary with modules cuts both the link
storm and the ~5–8 s startup tail. Measure `clippy --all-targets` warm
before and after on a two-file trial; do the rest only if it pays.

**6. Update the docs that state the gate.** CLAUDE.md's "inner loop vs
pass-end gate" section (local gate is plain `cargo test`; the ignore
tier is CI's), `.github/workflows/ci.yml` (keep the release
`--include-ignored` leg per PR; it should now be seconds), and the
candidate doc.

## Done when

- `cargo test` warm debug < 10 s wall, whole workspace, run from any cwd.
- Every remaining `#[ignore]` names the unique claim it makes; none is
  a bulk repeat of a synthetic test. Target: a handful, all in CI's
  release leg.
- Every file that reads a corpus says which of the three jobs it does.
- Step-0 counts and the pre-shrink manifest are in the ledger.
- The test tier under `testData/exampleCorpora` is ~10 MB (bsb 4.7 +
  fixtures + MRK); en_ult is still in the repo under `stressCorpora/`
  and nothing under `tests/` names it.

## Open questions

- Is MRK the right en_ult fixture, or a ~100-verse aligned excerpt
  alongside it for the shape tier?
- Which sous-core corpus reads are calibration (need all 8, whole) and
  which are shape checks (a slice would do)? Decide after re-measuring.
- Committed fixture USFM vs a recipe that re-excerpts from a corpus
  refresh. Leaning committed: hermetic, and the quirks are stable.
- Does `opt-level = 1` in the test profile interact with mbx's cache keys
  or the `incremental = false` choice? Check before adopting.

## Log

Executed 2026-09-04. Target met: warm `cargo test` 294 s → **8.4 s** at the
workspace root, **4.8 s** from inside `onion/`. All numbers M1 Max, warm,
`--no-fail-fast`, median of three.

### Step 0 — evidence frozen

`playground --lint-stats` over the 226-book tier, the per-binary baseline, and
a 259-row corpus manifest are in `planning/choices.md` and
`planning/pre-shrink-manifest.txt`.

Re-measured baseline (the brief's numbers, confirmed):

    wall 275 s  ·  lint_corpus 221.5  ·  format_corpus 13.6  ·  mask_oracle 11.2
    atom_conformance 7.8  ·  html_corpus 5.8  ·  ~35 binaries under 2 s

### Step 1a — `opt-level = 1`: 275 s → 45.2 s

`[profile.dev]` AND `[profile.test]` in the root `Cargo.toml`. Both are needed:
an integration test links the lib through `dev`, so setting `test` alone leaves
the sweeps at `-O0`. `debug_assertions` stays on (dev's default).

    binary            before    after
    lint_corpus       221.52     21.91
    format_corpus      13.60      1.54
    mask_oracle        11.17      1.14
    html_corpus         5.79      1.14
    atom_conformance    7.77      0.93

Compile cost: the flag change refingerprints once (58 s one-off, mbx stored the
new keys); warm `cargo test --no-run` then returns to 2.0 s, unchanged from the
1.7 s before. mbx is unaffected — it keys on the flags, so the old and new
objects coexist. **Kept.** This is the single largest lever in the pass, and
`atom_conformance` was entirely a debug-mode-arithmetic artifact: no profiling
needed, step 4's question answered.

### Step 1b — no more silent skips

17 files. Every `if paths.is_empty() { eprintln!("… SKIPPED …"); return }` is
now `assert!`, and every corpus path resolves through
`concat!(env!("CARGO_MANIFEST_DIR"), "/../testData/…")` rather than cwd. Three
`Option`-returning corpus helpers (`lint_corpus`, `format_corpus::corpus`,
`attr_corpus::sweep`) lost their `Option`. Scripted; the compiler caught the
rest. This is why `cargo test` from the workspace root and from `onion/` now
agree — before, `../testData` only resolved because cargo happens to set a test
binary's cwd to its package root.

### Step 2 — en_ult out of the test tier: 45.2 s → 7.1 s

`git mv testData/exampleCorpora/en_ult testData/stressCorpora/en_ult` (99 MB of
110), with `42-MRK.usfm` copied back as
`testData/exampleCorpora/en_ult-fixtures/42-MRK.usfm` — the aligned book a test
names, `\w` inside `\zaln` at depth. Tier: 226 books / 113.6 MB → 160 books /
12.8 MB.

Repointed to `stressCorpora`: `onion/benches/rewrite.rs`,
`galley/benches/warmer.rs`, `onion/benches/pipeline.rs`'s doc line. Both bench
readers now take a path relative to `testData/`, not to `exampleCorpora/`, so
the tier a bench reads is visible at the call site. `onion/src/cst.rs`'s
streaming-equivalence unit test read the first en_ult book; it now reads the MRK
fixture by name, which is what it actually wanted, and no longer defensively
skips. `grep` confirms **no file under any `tests/` reads `stressCorpora`.**

Every pinned lint quirk survived the cut — the classes that carry them are
unchanged (unclosed-note 2, orphan-closer 1, designator-malformed 1
[en_ulb ZEC 12:7 `\v 7"`], verse-duplicate 1 [bdf_reg ROM 3 double `\v 10`],
verse-without-designator 1 [ACT 8:17 `\v +`], missing-verse-one 1 [bsb LAM 2],
missing-id 1, verse-gap 28, unknown-marker 13636 [the `\s5` idiom]). Only
en_ult's own already-itemised sub-counts left. So **no excerpting was needed and
en_ulb/bdf_reg were NOT shrunk**: every pinned case still lives in a whole book
that other volume tests also sweep, and the target was met without it. Shrinking
further would have traded real coverage for headroom nothing needs.

Old → new pinned counts (also in `planning/choices.md`):

    books                     226 → 160        bytes    113.6 MB → 12.8 MB
    missing-paragraph       5,434 → 5,403      (−31, all en_ult)
    numbering-mix              52 → 50         (−2)
    empty-paragraph           787 → 728        (−59);  fixes 762 → 706
    delimiter-surplus         875 → 39         (−836)
    all findings           20,821 → 19,893     fixes  7,074 → 6,151
    format edits          108,979 → 88,648     (en_ulb 48,934 + bsb 31,087 +
                                                bdf 7,940 + ULT MRK 687)
    attribute lists     1,253,766 → 27,033     attributes 4,352,929 → 101,362
    missing-paragraph outside en_ulb  33 → 2

### Step 3 — classification and the ignore tier

Every one of the 21 corpus-reading test files now opens with an `Instrument:`
line — SHAPES (`testData/usfmtc`), VOLUME (the whole test tier), SHAPES+VOLUME,
or the sous 8-corpus tier — and the stale "skips loudly / defensive fallback"
prose is gone, because it is no longer true.

The volume sweeps were NOT narrowed to the bsb anchor. With the tier at 12.8 MB
they cost 0.1–0.5 s each; narrowing them would drop en_ulb's `\s5` idiom and
bdf_reg's shapes from `format_corpus`, `mask_oracle` and the rest for no gain
the budget needs.

Dispositions, eleven ignored tests, one line of justification each:

- `galley/tests/warmer.rs::bench_fold_en_ult` — **(a) deleted.** It asserted
  nothing; it printed timings. `galley/benches/warmer.rs` already benches the
  same fold ladder including en_ult PSA, under divan, with an allocator profiler.
- `onion/tests/diff_corpus.rs::every_corpus_book_round_trips_against_its_twin_and_its_mutations`
  — **(b) un-ignored, 0.53 s.** The mutation shapes (deleted / duplicated /
  reordered verse) over real books are not a repeat of `diff_laws`' synthetic
  round-trip.
- `onion/tests/fold_oracle.rs::fold_oracle_over_every_corpus_book` — **(b)
  un-ignored, 0.24 s.**
- `onion/tests/utf16_oracle.rs::every_corpus_boundary_matches_a_char_walk` —
  **(b) un-ignored, 0.35 s.**
- `galley/tests/warmer.rs::fold_cache_equals_fresh_over_the_corpus` and
  `::the_analyze_fold_holds_over_the_corpus` — **(b) un-ignored, 0.63 s for
  both.**
- `galley/tests/equivalence.rs` ×4 (`churn_over_en_ulb`,
  `churn_over_en_ulb_from_a_second_seed`,
  `every_chapter_of_a_whole_bible_book_replaced_in_sequence_equals_cold`,
  `parallel_publish_byte_equals_the_serial_cold_oracle_over_en_ulb`) — **(c)
  kept `#[ignore]`, 2.70 s for the four.** Inherently whole-Bible scale: 50 cold
  publications per churn test. Reason strings rewritten to name the claim
  ("only proof that 50 cold whole-Bible publications and their resident replay
  stay byte-equal under random churn"), not the schedule.
- `mise/tests/utf16_corpus.rs::the_table_equals_the_index_at_every_boundary_of_the_tier`
  — **(c) kept, 0.94 s.** Reason now names the claim: the table equals a char
  walk at EVERY boundary of the 35 MB sous tier.
- `sous-chef/core/tests/substrate_reference.rs::map_equals_the_reference_over_the_whole_tier`
  — **(c) kept, 1.72 s.** Same, for the scalar map against the reference walk.
- `sous-chef/core/tests/hygiene_scalar_reference.rs::the_lane_equals_the_reference_over_the_whole_tier`
  — **(c) kept, 0.78 s.**

Seven survivors, in three binaries, all in CI's release leg. Un-ignoring the lot
was measured first: it costs 16.3 s wall, over budget, and 6.1 s of that is
these seven. Every survivor's reason string now says WHAT it proves.

### Step 4 — sous-chef, re-measured for a later decision

The 8-corpus `corpora/*.txt` reads are untouched, as instructed. After steps
1–3:

    substrate_reference        default 0.05 s   ignored half 1.72 s
    hygiene_scalar_reference   default 0.05 s   ignored half 0.78 s
    atom_conformance           0.70 s  (was 7.77 — fixed entirely by step 1a)

Nothing here justifies cutting a read yet. The open question stands: whether the
two reference walks want all 8 corpora whole (calibration) or a chapter slice
(shape). At 2.5 s in the release leg it is not urgent.

### Step 5 — binary count: measured, NOT done

    warm `cargo clippy --all-targets`   1.92 s
    warm `cargo test --no-run`          2.04 s
    sum of in-binary test time          5.62 s   inside an 8.4 s wall
    ⇒ per-binary harness overhead      ~60 ms × 45 binaries ≈ 2.7 s

Trial: folded `fast_path_identity` into `partition_oracle` as a module, one
binary fewer. Result: clippy 1.78 s (noise), `cargo test` 8.9 s — no
improvement over 8.4 s. **Reverted.** mbx has already removed compile time as a
cost, so the link storm the plan predicted does not exist; the remaining ~2.7 s
is per-process startup, and buying it back would cost ~1.1 s for the whole
20-file fold at the price of 20 files' worth of readability. Not worth it.

### Step 6 — docs

`CLAUDE.md`'s test section rewritten (the local gate is plain `cargo test`; the
seven-test ignore tier and the rule that its reason strings name claims; the
corpus-tier table now including `stressCorpora`). `.github/workflows/ci.yml`:
the `oracles` job's comment now describes the seven survivors and the ~11 s
release run, and `test`'s timeout drops 60 → 15 minutes. The candidate doc is
marked SUPERSEDED.

### Finish line

    cargo test              root  8.4 s   ✅  green, 45 binaries
    cargo test              onion/ 4.8 s  ✅  green
    cargo test --release -- --include-ignored   10.6 s  ✅ green (was ~85 s)
    cargo clippy --all-targets                  clean   ✅
    no tests/ file reads stressCorpora          ✅ (grep)

Margin against the 10 s target is ~1.5 s, and it is spent on the four oracles
step 3 promoted out of the ignore tier — a deliberate trade of headroom for
coverage in the default run.
