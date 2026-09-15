# Testing

- Local gate: `cargo nextest run`, whole workspace, ~5s warm (plain
  `cargo test` ~9s). Run it, not a filtered subset; there is nothing to save.
- The wall, by hand: `cd galley && wasm-pack test --node --features wasm
  --test wasm_wall` publishes the goldens `tests/sous_goldens.rs` pins.
  `tests/sous_conformance.mjs` over a `--target nodejs` build is the JS half,
  and the one place the module's exported names are pinned as a list.
- Finish line for a pass: that plus `cargo clippy --all-targets` clean.
  `cargo test --release -- --include-ignored` (~11s) is what CI runs anyway;
  `.github/workflows/ci.yml` runs debug for the `debug_assert!`s and release
  for the ignored tier.
- Dev and test profiles are `opt-level = 1`. Debug asserts stay on; the
  corpus sweeps would be minutes at level 0.

## The ignore rule

- Sixteen tests are `#[ignore]`d, ten of them in `galley/tests/equivalence.rs`
  — eight whole-Bible churns, two seeds per variant. Each reason string names
  the claim nothing else makes (whole-Bible publication equality under churn,
  under a config that moves every step, under a source replaced and withdrawn
  mid-run, and under the smallest hot set and generation ring; mise's UTF-16
  boundary sweep of the 8-corpus tier and onion's of the whole test tier;
  sous-core's four reference walks).
- Adding an `#[ignore]` means writing that sentence. A reason that says WHEN
  to run instead of WHAT it proves is the smell. A bulk rerun of a law
  already proven synthetically is not a test; delete it.

## Corpus tiers

The boundary is always a directory, so a sweep needs no filter logic. No
`#[test]` may read a stress or calibration tier.

    testData/exampleCorpora/       onion's test tier: 160 books, 12.8 MB
    testData/usfmtc/               the committee's weird-shape encoder
    testData/stressCorpora/        en_ult, 99 MB: benches, playground, stress
    corpora/*.txt                  sous's test tier: 8 corpora, script spread
    corpora/calibration-corpora/   ~1500 bibles, gitignored, R2-fetched

- Sweeps over a stress or calibration tier are a bin, bench, or example you
  run deliberately. Record what they found in the ledger; do not assert on it.
- Every corpus-reading test file says in its module doc which instrument it
  is: SHAPES (usfmtc) or VOLUME (the whole test tier).
- The trap is a silent test, not a failing one. `if dir.exists()` around a
  corpus read passes everywhere by doing nothing. Every corpus read asserts
  its paths and resolves them from `CARGO_MANIFEST_DIR`, so the suite is
  cwd-independent. Let a missing corpus fail.
