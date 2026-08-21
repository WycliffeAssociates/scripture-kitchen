# wasm `analyze()` sketch (roadmap 6 — NOT APPROVED, react to it)

The stateless wasm surface. RULED foundation (settled-facts, 2026-08-18):
one call, full re-lex + build + lint inside, UTF-16 index computed fresh
per call, reads are typed arrays of UTF-16 numbers, JS never holds a
token. The stateful `BookSession` stays a demoted, unbuilt optimization.
Everything below either restates a ruling (marked RULED) or proposes a
shape for Will to mark up (PROPOSED).

## The one call

```rust
#[wasm_bindgen]
pub fn analyze(text: &str, wants: u32 /* read-selection bitmask */)
    -> Analysis;   // a JS object of typed-array getters, all fresh copies

// Inside, always the same three lines + the emitters:
//   lex → cst::build → lint → emit(wants)
// The result is stamped with nothing — JS stamps it: the caller pairs
// the returned Analysis with the doc version it sent (stale ⇒ discard).
```

- RULED: stateless, race-free, sync main-thread first; worker later;
  Rust-over-IPC parked behind both.
- PROPOSED: the `wants` bitmask. Reads have very different costs
  (diagnostics ≈ hundreds of entries; token_spans ≈ 12 bytes × every
  token — ~4 MB for aligned GEN). The editor asks for what it renders;
  nothing is computed or copied for an unset bit. Costs nothing to the
  stateless contract.
- PROPOSED: an optional clip range (`clip_from`, `clip_to`, UTF-16) that
  bounds only the EMIT step — analysis is always whole-book (the walker
  needs the whole book anyway), but token-granularity reads can be
  clipped to the viewport. Blocks/chapters/diagnostics stay whole-book.

## The reads (each a flat typed array; stride in u32s unless noted)

| read | stride | fields | source artifact |
|---|---|---|---|
| `chapters` | 5 | ordinal, label_from, label_to, from, to | TOC chapter rows (sketches/toc-vref-slab.md) |
| `blocks` | 4 | class+marker (packed, below), from, to | CST paragraph-kind nodes + extents |
| `note_extents` | 3 | kind (f/x/ef/ex/fe row), from, to | `Cst::extent` over Note nodes |
| `token_spans` | 3 | packed kind+judgments, from, to | tokens + the rows stamp |
| `text_runs` | 2 | from, to | Text tokens (∩ clip) |
| `verse_anchors` | 3 | chapter ordinal, num_from, num_to | TOC verse anchors |
| `diagnostics` | 7 | code, from, to, second_from, second_to, aux, fix | LintReport (below) |

All offsets UTF-16 (conversion below). All arrays are COPIES made fresh
per call (RULED — no views into wasm memory that a later call would
invalidate).

### The judgments stamp (the "no JS table" ruling, resolved)

PROPOSED resolution of the Category-26>4-bits question: ship a COARSE
CLASS, 3 bits, on every stamped range:

```text
0 para  1 char  2 note  3 milestone  4 chapter/verse  5 sidebar  6 table  7 other
```

packed per range as: `class (3) | payload_kind (2) | closing (2) |
nested-spelling (1)` = one byte, u32-aligned in the arrays above. The
app's shape inference needs only the class (its TEXT/FIELD/ATOM/CHROME
vocabulary is derived app-side — RULED); per-marker CSS comes from the
marker NAME, which is bytes the app already has (`doc.sliceString`).
No table crosses; no codegen change (the class is a match over
Category at emit time).

### The diagnostics record

One observation = 7 u32s: `code` (u16 widened — per-build number, valid
ONLY against this build's JS side-table, per the name-is-identity
ruling), `from`/`to` (the anchor token's span), `second_from`/`second_to`
(u32::MAX sentinel when absent), `aux` (per-code integer, meaning in the
side-table), `fix` (index into the fixes reads, MAX = none).

The JS side-table is CODEGEN'D FROM LINT_ROWS at build time and ships
inside the same bundle as the wasm: `{ name, categoryClass, severity,
template, auxKind, fixLabel }` per code, indexed by the same per-build
number. Severity maps 1:1 onto CM's ladder (RULED). Message rendering =
the template + `doc.sliceString(from, to)` for `{anchor}`/`{second}` +
`aux` — zero strings cross the boundary (RULED).

### Fixes crossing (PROPOSED: lazily)

Fixes are rare (≈ tens per damaged book) and applied rarer. The hot
`analyze` path ships only the `fix` index; a second export fetches one
fix's edits on demand:

```rust
#[wasm_bindgen]
pub fn fix_edits(text: &str, observation: u32) -> Uint32Array
// [from, to, insert_from, insert_to] × n — insert ranges into…
pub fn fix_inserts(...) -> String   // …one concatenated ASCII string
```

…OR, simpler and probably right: re-run is cheap, so `fix_edits`
re-analyzes and returns `[from, to] × n` (UTF-16) plus one JS string of
inserts with a length array. The label is the side-table's `fixLabel`.
CM applies as one transaction; offsets are pre-edit, applied right-to-
left (the fix oracle's own discipline). OPEN: whether re-running inside
`fix_edits` (≈ 2 ms worst) beats carrying the edit arrays eagerly
(≈ 100 bytes) — eager is simpler than it looked, decide at build time.

## UTF-16 at the wire (all measured, 2026-08-19/20)

- BULK OUT (every array above): the streaming counter rides the emit
  loop — offsets are written in document order, so one running
  `(byte, utf16)` cursor + a SWAR gap-count converts EVERYTHING in one
  sweep of the source (~0.4 ms worst book). No index consulted.
- RANDOM IN (cursor positions, fix application): the stride-256 index —
  1.6% of source on every script, built in 0.04–0.45 ms, ~100 ns per
  utf16→byte query. Built inside `analyze` only if a `wants` bit asks
  for the queryable index… PROPOSED: it never crosses; instead exports
  `to_byte(text, utf16: u32) -> u32` / `to_utf16(text, byte: u32) -> u32`
  that build the index on demand — at a handful of calls per
  interaction, rebuilding per call (~0.1 ms) is honest and stateless.
  If that measures dumb, an opaque handle is the fallback; do not
  design it in ahead of the measurement.
- `str_indices` is APPROVED-IN-PRINCIPLE (Will, 2026-08-20) as the
  counting primitive at production time; the experiment's hand-rolled
  SWAR is the fallback and the test oracle either way.
- Strings IN cross via wasm-bindgen `&str` (TextEncoder → UTF-8, valid
  by construction — no validation pass; debug_assert at most).

## Perf budget (RULED numbers, restated)

~31 ns/token native staged; ×2–3 wasm ⇒ worst aligned book ≈ 20 ms,
prose book ≈ 200 µs, behind the probe's 150 ms debounce. The emit copies
are the only new cost: token_spans for aligned GEN ≈ 4 MB — which is why
`wants` + clip exist. En_ulb whole-66-book corpus ≈ 8.5 ms native.

## What the CM prototype actually consumes today (inventoried 2026-08-20)

onion_editor_chef/src/cm builds a JS stand-in `Scan` per change:

| prototype need | engine read that serves it |
|---|---|
| `blocks` (kind, from, to) | `blocks` (class + marker name from doc bytes) |
| `chapters` (ordinal, label, range) | `chapters` |
| per-line marker/contentFrom | `token_spans` (marker token end = contentFrom) |
| `\v`/`\c` num ranges (`numFrom/To`) | `verse_anchors` / Designator spans in `token_spans` |
| note spans + kind | `note_extents` |
| note ref/body DISPLAY text | app-side: slice the extent, strip via `token_spans` within it (interpretation stays app-side — RULED) |
| `\w` surface + attr-tail ranges | `token_spans` (Marker/Text/AttrList/Closer kinds) |
| milestones (from, to, name) | `token_spans` class 3; name = doc bytes |
| its fabricated verse lint | `diagnostics` (replaces it outright) |
| transforms (rewrite tr against startState) | the Analysis paired with startState's version — JS keeps the last result per version; nothing new needed |

Gaps found: NONE engine-side — every prototype need decomposes onto the
planned reads. Unused engine-side: `text_runs` (search/proofing
consumers, not the editor — keep behind its `wants` bit).

## Test methodology (intentionally minimal)

1. THE identity oracle, native: `analyze_native(text, wants)` (the same
   emit code path compiled natively) equals the three-liner's artifacts
   re-derived by a dumb reference emitter, over the corpus. The emit
   layer is pure Rust — test it natively, not through wasm.
2. ONE wasm smoke test (`wasm-pack test --node`): a real book in,
   assert a handful of known values out (chapter count, one diagnostic,
   one utf16 offset against a hand-computed value on a non-ASCII book —
   hindi-IRV is the fixture). The boundary is thin; don't build a JS
   harness beyond this.
3. UTF-16 correctness already lives in experiments/utf16.rs's oracle;
   production adoption lifts those tests wholesale.

## Open

1. `wants` bitmask + clip range — confirm the shape.
2. Coarse-class 3-bit table above — confirm the 8 classes.
3. Fixes: lazy `fix_edits` vs eager arrays (lean: eager if it stays
   ~7 u32s + small strings; lazy only if eager gets fiddly).
4. Index-free `to_byte`/`to_utf16` exports vs an opaque handle —
   measure first.

## Galley method list (accumulating, 2026-08-21 — the composed bindings crate)

The one-module composition (ideas/other_repos/sous.md): onion and sous
stay independent library crates; "galley" is the thin stateful bindings
crate depending on both — the retained source String, the shared
Utf16Index, coordination, any caching. Methods pulled so far by real UI
asks, all query-shaped (no bulk DataView until a profiler asks — the
#[repr(C)] rows keep that door open):

- `new(text: String) -> Galley` — the one string crossing
- `diagnostics()` — onion lint + sous proofread, one stream, source bytes
- `locate(byte: u32) -> String` — "MRK 6:3" (status bar, diagnostic labels)
- `chapters()` — ≤151 rows for the navigation grid: number + start
  (+ raw label via ChapterRow.token when the grid wants "12b")
- `book() -> String` — the \id code; manifests own project-level naming
- `to_utf16(byte) / to_byte(utf16)` — the wire translation, shared index
- exports on demand: `usj() / usx() / html() -> String`
