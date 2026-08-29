# The wasm benches

`galley/benches/fold.rs` measures the fold natively; these measure the SAME
shapes through the JS wall, so a native number and a wasm number are comparable
rather than two different experiments.

    wasm-pack build --target nodejs --release --out-dir pkg-node -- --features wasm

    node keystroke.mjs     pkg-node ../../../testData/exampleCorpora   # folded vs fresh, per keystroke
    node analyze.mjs       pkg-node ../../../testData/exampleCorpora   # stateless analyze, one book at a time
    node wants-decomp.mjs  pkg-node ../../../testData/exampleCorpora   # per-read marginal cost

`analyze.mjs` and `wants-decomp.mjs` take EITHER build — galley's module carries
onion-wasm's exports through linking, so `analyze` is in both, and pointing them
at an `onion-wasm` pkg measures the engine without galley in the picture.
`keystroke.mjs` needs the galley build; `Galley` only exists there.

`keystroke.mjs` mirrors `fold.rs`'s bench exactly: a ring of successive typing
states so every measured call is a NEVER-SEEN text, one dirty chapter, the rest
served from the cache. It also PROVES three things before timing anything and
exits non-zero if any fails — the folded object JSON-equals the stateless one,
one keystroke is one miss, and a book under the chunk gate caches nothing.

## Why not hyperfine, why not mitata

hyperfine measures whole-process wall clock, and node's ~40 ms startup swamps a
sub-20 ms operation — it cannot see these at all. mitata would work; it is a
dependency to do what ten lines of `performance.now()` already do, and the plain
harness matches how the native numbers are taken (min of N, which is Divan's
`fastest` column).

## Reading them

Take `fastest`. A loaded machine can only inflate a minimum, never deflate it —
so the lowest figure across runs is the honest one, and a run taken under load
is a ceiling on what the code does, not a measurement of it.
