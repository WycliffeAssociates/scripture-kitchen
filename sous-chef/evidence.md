# Evidence ledger

Every measurement Sous Chef 2's architecture rests on. Append rows; never
rewrite one. A row that a later run contradicts gets a *new* row saying so —
the point of a ledger is that the old number stays readable next to the new.

Rows are evidence, not production guarantees. Rerun the named command when a
production choice depends on one.

Machine `M1` is Apple Silicon, one core, the committed 8-corpus test tier in
`corpora/`. Rejected implementations measured here are kept, with their
numbers, in [experiments/](experiments/).

## Roofline and differential runs

| date | question | machine · command | observed result | consequence |
| --- | --- | --- | --- | --- |
| 2026-09-01 | roofline, production hygiene | M1 · `cargo bench -p sous-core` | scalar dependent chain 1.05 GB/s; autovectorized compare 10 GB/s; `memchr3` 31 GB/s on clean text, 5.6 GB/s on French (0xC2 NBSP/guillemet density); `hygiene::scan` 8.6 GB/s on six corpora, 6.6 GB/s Greek, 3.8 GB/s French. Replacing three `memmem` marker passes with one `memchr3` took it from 5.8 to 8.6 GB/s | every pass is measured against the vectorized ceiling; the needle filter's hit density, not the range filter, is the remaining cost |
| 2026-09-02 | roofline, Stage 1 classifier walk (medians, ns/scalar) | M1 · `cargo bench -p sous-core` | plain lookup (static two-level) 1.64 en/nya/swh, 1.83 spa, 2.00 fra, 3.66 amh, 3.90 hin, 4.17 grc. Lazy 128 KiB flat BMP 1.97/2.14/2.26/3.36/3.57/3.79 — faster on non-Latin by 8-10%, slower on Latin by 17%. Byte trie over raw UTF-8 1.31/1.51/1.65/3.23/3.46/3.89: faster than the plain lookup on every corpus. Byte trie plus 8-byte SWAR ASCII with hysteresis 0.22 en/nya, 0.36 swh, 1.49 spa, 1.76 fra, 3.40 amh, 3.62 hin, 4.01 grc. SWAR over the *decoded* lookup taxes amh/hin 24% and grc 13% | **Table:** static two-level wins the near tie on size — 28.7 KiB of `.rodata`, no heap, no `OnceLock`, nothing extra in the `.wasm`, against 128 KiB built at first use. **Fast lanes:** the byte trie ships, and the SWAR ASCII chunk ships on top of it — no corpus in the tier is slower than the plain lookup, and the chunk costs the bare trie only 3-7% on non-Latin. The decoding SWAR variant is rejected: v1's failure mode reproduced exactly, and taxing Indic and Greek to speed English is the wrong trade |
| 2026-09-02 | Stage 1 classifier walk, re-run after the losers were retired to `experiments/` | M1 · `cargo bench -p sous-core`, twice | Whole machine reads ~25% below the row above: `memchr3` 24 GB/s (was 31), autovectorized compare 7.8 GB/s (was 10). Within that, the plain two-level walk and the bare byte trie **swap order** — 1.12 vs 1.64 ns/scalar on English, stable across both runs, where the row above has 1.64 vs 1.31. The shipped `walk_trie_swar` lane is unchanged at 0.27 ns/scalar English and still fastest on every Latin corpus | **Not adjudicated.** No shipped code path changed in the retirement; `hygiene::scan` still rides the trie plus SWAR chunk. Either the machine state or the removal of the `Lookup` enum from the bench's own subject moved the two-level row. Re-measure both walks on a quiet machine before any promotion argument leans on their order |
| 2026-09-02 | atom rule fleet differential | `cargo run -p sous-core --release --example atom_fleet`, over the donor's `ebible-main/corpus` checkout because `corpora/calibration-corpora/` is not present on this machine | 1,079 corpora, 1,809,448,088 UAX #29 clusters, 0 split by the atom rule, 0 corpora affected | the conservative widening rule is safe to ship with no runtime segmenter. GB9c is the one place it needs help: without the linker bit, 102,139 Hindi clusters (3.6% of `hin2017`) split |
| 2026-09-02 | JS→WASM string marshaling cost (`TextEncoder.encodeInto` into a `WebAssembly.Memory` view, the wasm-bindgen path; medians of 10, 3 for 250 MB) | M1 Max · Node 24.4.1, throwaway script in the scratchpad | ~5 KB chapter 4–5 µs; ~150 KB book 0.10 ms ASCII / 0.15 ms 50% Devanagari; ~5 MB corpus 3.5 / 4.9 ms; 250 MB pathological 179 / 258 ms. Throughput flat with size: ~1.4 GB/s UTF-8 out ASCII, ~1.0 GB/s mixed. `encodeInto` is 1.4–1.6× faster than allocating `encode`; pure scan (`Buffer.byteLength`) is 4–5× faster than either, so the write dominates. `Memory.grow` for 250 MB: 0.25 ms once. Peak RSS with both 250 MB variants held: 1.5 GB | Resending a whole 5 MB corpus per keystroke costs 3.5–5 ms of a 16 ms frame — confirms keyed whole-book `update` (0.1–0.15 ms per book). A `find` that passes one book's text is free; whole-corpus find on a normal Bible is a few ms per query, acceptable for an explicit action; the 250 MB aligned target is ~0.2 s per query. Text pinning stays deferred: WASM linear memory never shrinks, so pinning inflates the instance permanently, a device decision no library should make |

## Probe evidence behind the starting architecture

These are v2 exploratory probes, run before the production crates existed.
Probe entry points live under `spikes/probes`; `cargo test -p probes` and the
Criterion benches are the local starting points.

| date | question | observed result | consequence |
| --- | --- | --- | --- |
| pre-v2 | byte hygiene cost | SWAR range scans about 5.3 GiB/s; fixed needles 2.3-41 GiB/s across stress corpora | rescan; do not retain hygiene state initially |
| pre-v2 | stream versus scalar tape | streaming was about 1.3-2.3× faster; tape cost worsened with corpus size | no ambient materialized tape |
| pre-v2 | full shared substrate | about 28 ms on the English probe corpus versus roughly 257 ms v1 cold analysis | simple whole-corpus/whole-book passes are viable |
| pre-v2 | expanded feature substrate | roughly 12-31% above the first shared-counter cut | counter-shaped additions can share the walk cheaply |
| pre-v2 | grapheme atom differential | after fixing extender classification, one nonletter-involving mismatch over 1,504 corpora | fast walk plus grapheme-safe emission is viable |
| pre-v2 | per-chapter row reduce | about 17-25 µs for 8k-12k glyph rows; edit plus reduce under 50 µs | derive aggregates on read; no rollup cache |
| pre-v2 | band sweep | default staircase gave about p50 10, p90 36, p95 44 rows/corpus; low cloud tracked `0.8/sqrt(n)` | readable fraction bands are credible shipping candidates |
| pre-v2 | dispersion | clustering changed gradually with share; genre remained a confound; above-band clustered rows about 0.3/corpus | annotation/ranking only, never a hard gate |
| pre-v2 | packed v1 wire donor | fixed 16-byte buffers made wasm/transfer/decode nearly flat and far cheaper than object arrays in the old measurements | preserve fixed binary snapshots, but redesign addressing |
| pre-v2 | proportionality paired survey | project scope covered small books; 10-20% chops were nearly invisible; source choice materially changed results | keep dual scopes and narrow claims; reproduce before defaults |

The old repository's calibration notes remain historical evidence in git and in
the donor checkout; they are intentionally not duplicated here.
