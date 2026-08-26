# onion-wasm

The JS doorway over the USFM engine (`onion`) — **piece 2 of the five-crate
layout**: bindgen, the `.d.ts`, and the JS-environment utilities (UTF-16 walls,
the decoder wrapper) for THIS engine and nothing else.

The five pieces (`planning/ideas/committed/galley.md`): **onion** (the engine),
**onion-wasm** (this), **sous** (proofreading), **sous-wasm**, and **galley**.

`galley` is the RESERVED name for that last one — the opinionated WORKFLOWS
crate over onion + sous: dirty-marking, checksums, ingest recipes, find,
onion↔sous coordination, and the composed-analysis-host binary the editor
vision describes (§4.4, §9.4). This crate will be one of galley's dependencies.
It was briefly named `galley` itself; that was a naming error, corrected here.

**Nothing is implemented here.** Every export is a `#[wasm_bindgen]` tag over a
library function, plus — where the wall demands it — an offset conversion or a
small wire struct. If an export starts to have logic, the logic belongs in a
library. You could delete this crate and recreate it in an afternoon; that is
the point.

## What's in the package

| file | what it is | who writes it |
|---|---|---|
| `src/lib.rs` | the tagged exports | by hand |
| `onion-wasm.ts` | the decoders — **the JS-side schema**: strides, bit layouts, the sentinel | by hand |
| `diagnostics.json` | per lint code: name, severity ladder, category, message template, aux kind, fix label | `cargo run --bin codegen` |
| `package.json` | the subpath map over the two builds | by hand |
| `pkg-bundler/` | `--target bundler` output — **committed** | `wasm-pack build` |
| `pkg-web/` | `--target web` output (explicit `init()`) — **committed** | `wasm-pack build` |
| `tests/node.rs` | the ONE boundary test | by hand |

`onion-wasm.ts` and `diagnostics.json` sit beside the two builds and are reached
through the same `package.json` — they are versioned WITH the binary and must
never be fetched from anywhere else.

## Build

```sh
wasm-pack build --target web     --release --weak-refs --out-dir pkg-web
wasm-pack build --target bundler --release --weak-refs --out-dir pkg-bundler
rm -f pkg-web/.gitignore pkg-bundler/.gitignore   # wasm-pack writes `*`

wasm-pack test --node                      # the one boundary test
cargo build --target wasm32-unknown-unknown -p onion-wasm   # compile check
cargo test -p onion-wasm                   # native tests (rlib, no wasm needed)
```

The spike's `scripts/sync-engine.sh` runs exactly that pair and then vendors
`pkg-web/` — keep the two builds in step, they ship together.

`--weak-refs` is not optional in a real build. `analyze` returns a plain object
and retains nothing, but the exports that still hand out a HANDLE (`FormatOpts`,
`Edits`, `Splices`) rely on it as the backstop for a missed `.free()`. It is a
backstop, not the paved path: the wrappers still free explicitly. (`using` /
`Symbol.dispose` is banned — it crashes older webviews.)

`RUSTFLAGS="-C target-feature=+simd128"` is a free ~8% on the lexer; expect no
more than that.

## Distribution

Not on npm. Consumers install from a GitHub tag, which is why the two `pkg-*`
directories are committed rather than gitignored. `package.json` maps the
subpaths: `.` → `pkg-bundler`, `./web` → `pkg-web`, `./web/wasm` → the binary,
`./schema` → `onion-wasm.ts`, `./diagnostics.json` → the side-table.

**One build per target, full default features** (`usj`/`usx`/`html` on). There
are no all-vs-lean prebuilt variants: the binary is ~370 KB (~152 KB gzipped)
and no size-sensitive consumer exists. A consumer who wants less builds from
source with `default-features = false` and drops whichever exports it does not
need; prebuilt lean variants are a later decision if one ever asks.

Open questions are parked in `planning/sketches/wasm-analyze.md` §Distribution.

## The reads, and how much each costs

| read | stride | serves |
|---|---|---|
| `chapters` | 7 | nav grid, chapter clamp, `\c` chrome |
| `blocks` | 4 | paragraph grouping |
| `lines` | 4 | every line-level editing rule |
| `noteExtents` | 3 | footnote widgets |
| `noteParts` | 4 | the note apparatus (rides `NOTE_EXTENTS`) |
| `tokenSpans` | 3 | syntax highlighting — the one read `clip` exists for |
| `textRuns` | 2 | search / proofing views |
| `verseAnchors` | 5 | verse-number widgets, `\v` chrome |
| `diagnostics` | 7 | squiggles + panel (with `fixes`/`fixEdits`/`fixLens`/`fixText`) |

There is no per-change "commit" set here: which reads an editor needs on each
accepted change is that editor's opinion, not a library fact, so an app composes
its own from `WANTS`. Diagnostics are the debounced second call — lint is the
expensive artifact.

## The write path

| export | what it gives back |
|---|---|
| `formatEdits(text, opts)` | the whole-book transaction — `Edits`, spans in UTF-16 |
| `formatEditsIn(text, from, to, opts)` | the same transaction, scoped to a UTF-16 window |
| `format(text, opts)` | the formatted document, in one call |
| `diff` / `merge` | the review path (see below) |

**Scope it engine-side, never in JS.** `formatEditsIn` runs the SAME whole-book
analysis — lint needs the book, and which rule owns a contested byte is settled
over the whole document — and then filters, so the scoped list is always a
subset of `formatEdits`. The policy JS cannot reproduce:

- an edit is kept only if its ENTIRE span is inside the window; one straddling
  the boundary is dropped whole, never cut (half an edit corrupts);
- a multi-edit claim (`bridge-empty-verses` writes a range AND deletes the
  verses it swallowed) is kept only if ALL of it is inside — a JS `filter` sees
  a flat list and would keep half of one;
- a pure insertion sitting ON either edge is inside — a caret at the window's
  edge is in the window.

Chapter scope needs no second entry point: the window is `chapters[i]`'s span
from the `chapters` read.

## The contract

- **Stateless.** Text in, numbers and strings out, nothing retained between
  calls. Pair every result with the document version you sent; stale = discard.
- **`analyze` returns a PLAIN OBJECT.** Every read is built eagerly inside the
  binary, so nothing wasm-side outlives the call and there is nothing to free.
  `analysis(analyze(text, wants))` from `onion-wasm.ts` wraps it in the
  decoders. The write path still hands out handles (`FormatOpts`, `Edits`,
  `Splices`); those are freed explicitly, with `--weak-refs` as the backstop.
- **Nothing rich crosses.** Flat `Uint32Array`s (copies, never views into wasm
  memory) plus a few strings. JS never holds a token. The one structured export
  is the diff skeleton — a cold, modal-open path where serde JSON buys back the
  old editors' camelCase contract verbatim.
- **UTF-16 offsets, LF-canonical input.** Every offset out is a CodeMirror
  code-unit offset. That assumes the text is LF-normalized: CodeMirror counts a
  line break as ONE position, a literal `\r\n` is TWO UTF-16 code units, so
  CRLF input yields offsets one ahead of the editor's from the first line
  onward. The vision canonicalizes at ingress (§6.3) — LF-in is the contract,
  not something `analyze` repairs. Debug builds assert it.
- **The marker registry never crosses.** Marker names are bytes the editor
  already has (`doc.sliceString`); the coarse rendering class rides packed in
  the spans; lint codes index `diagnostics.json`.

## Deliberately absent

- **A checksum export.** Vision §13.4 puts the canonical-source checksum on this
  facade. Will deferred it (2026-08-24) as a higher-level concern — revisit when
  sous joins and the host owns a source version. No hashing dependency is here,
  on purpose.
- **Any stateful handle.** Ruled: stateless first. If a profiler ever catches
  the per-call index build mattering, an opaque handle is the documented
  fallback — do not pre-design it.
