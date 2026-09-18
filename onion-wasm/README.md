# onion-wasm

The JS doorway over the USFM engine (`onion`) — **piece 2 of the five-crate
layout**: bindgen, the `.d.ts`, and the JS-environment utilities (UTF-16 walls,
the decoder wrapper) for THIS engine and nothing else.

The five pieces: **onion** (the engine), **onion-wasm** (this), **sous**
(proofreading), **sous-wasm**, and **galley**.

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
| `onion-wasm.ts` | the WRITE path's decoders — the format transaction's handle wrapper | by hand |
| `reader.ts` | the typed door over the wire — every stride and offset, emitted from `wire::schema` | `cargo run --bin codegen` |
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
./build.sh                                 # BOTH committed packages — use this

wasm-pack test --node                      # the one boundary test
cargo build --target wasm32-unknown-unknown -p onion-wasm   # compile check
cargo test -p onion-wasm                   # native tests (rlib, no wasm needed)
```

`build.sh` is the only supported way to produce `pkg-web`/`pkg-bundler`, because
the committed `.wasm` has to be BYTE-IDENTICAL on any machine — that is what
lets CI gate the binary and not merely its interface. Two things leaked the
build host into it, and both are now handled:

- **Absolute paths.** Panic locations embed the tree that compiled them — a
  registry checkout under `$CARGO_HOME`, and the rustup sysroot, whose name
  carries the host triple. `build.sh` exports `--remap-path-prefix` for both, so
  they read `/cargo/...` and `/rust/...`. Before this the binary also shipped
  the builder's home directory to every consumer.
- **The `producers` section**, which records the BUILD of wasm-bindgen and
  walrus that processed the module (`0.2.127` on one machine, `0.2.127
  (a579ee62b)` on another). `--strip-producers` in `Cargo.toml`'s wasm-opt args
  removes it, inside wasm-pack's own pass so nothing is re-emitted twice.

Verified end to end: the same source builds to `390672` bytes with one sha on
macOS/arm64, Linux/arm64 and Linux/x86_64. The codegen was always deterministic;
only this metadata was not — which is why CI can fail on a stale binary rather
than merely warn.

The spike's `scripts/sync-engine.sh` runs exactly that pair and then vendors
`pkg-web/` — keep the two builds in step, they ship together.

`--weak-refs` is not optional in a real build. `parse` returns bytes and
retains nothing, but the exports that still hand out a HANDLE (`FormatOpts`,
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

That map is not what a consumer resolves against, though: npm cannot install a
subdirectory of a git repo, so the REPO-ROOT `package.json` is the installable
identity (`@wycliffeassociates/scripture-kitchen`). It is derived from this
file and `galley/package.json` together, by `node package-root.mjs >
package.json`: these subpaths land under `./onion`, and `.` / `./web` are
`usfm-galley`, the superset module. Edit the map here and regenerate, never
both by hand. CI diffs the derived file, because a stale `sideEffects` is
silent: a bundler that does not see it tree-shakes the wasm glue out of a
consumer's app.

**One build per target, full default features** (`usj`/`usx`/`html` on). There
are no all-vs-lean prebuilt variants: the binary is ~370 KB (~152 KB gzipped)
and no size-sensitive consumer exists. A consumer who wants less builds from
source with `default-features = false` and drops whichever exports it does not
need; prebuilt lean variants are a later decision if one ever asks.

## The read path

One call, one buffer, one tree. `parse(text, diagnostics, toc, utf16)` returns
the dish — nine little-endian sections behind an `ONWR` header — and `reader.ts`
is the typed door over it:

```ts
import { reader } from "onion-wasm/schema";
const onion = reader(rawParse);
const { tree, tokens, diagnostics, toc } = onion.parse(text, { diagnostics: true });
```

| section | carries |
|---|---|
| `tokens`, `nodes`, `childIds` | the tree — every token, and the CST over it |
| `diagnostics`, `fixes`, `edits`, `fixText` | squiggles, the panel, and each fix's transaction |
| `chapters`, `verses` | the chapter/verse index (`toc`) |

**No strides are written down here, on purpose.** `reader.ts` and
`onion/src/wire/generated.rs` are both emitted from `onion/src/wire/schema.rs`,
so the two ends cannot disagree and nothing is mirrored by hand — a stride in
this README would be the one copy free to rot. `cargo run --bin codegen`
regenerates both; `codegen_output_matches_input` fails if either is stale.

`diagnostics` is the one expensive optional (it runs the lint walk), so it is
the debounced second call. `toc` is cheap and needs no tree.

## User `\z` markers

| export | what it gives back |
|---|---|
| `extensionsFromMarkersExt(text)` | `{ markers, malformed }` as JSON — a `markers.ext` file, read. Installs nothing |
| `setExtensions(json)` | installs the LIST (not the file) process-wide; returns the entries it could not keep, as JSON |

A registered `\z` marker behaves as its `\category`: the engine resolves it to
the spec row that category behaves as, so a `footnote` extension takes a caller
and a note scope, a `milestone` pairs `-s`/`-e` and takes attributes. An
unregistered one is row 0, exactly as before. Reports are values — one bad
entry costs one entry — and only malformed JSON throws.

**Installing invalidates every product derived under the old rows**, so it
belongs at composition rather than between edits. Both doors stand on
`usfm-galley` too, which is the package a host vendoring one module gets.

## The write path

| export | what it gives back |
|---|---|
| `formatEdits(text, opts)` | the whole-book transaction — `Edits`, spans in UTF-16 |
| `formatEditsIn(text, from, to, opts)` | the same transaction, scoped to a UTF-16 window |
| `format(text, opts)` | the formatted document, in one call |
| `diff(baseline, current, text_mode)` / `merge` | the review path (see below) |

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
from the `chapters` section.

## The contract

- **Stateless.** Text in, numbers and strings out, nothing retained between
  calls. Pair every result with the document version you sent; stale = discard.
- **`parse` returns BYTES.** The whole dish is built eagerly inside the binary,
  so nothing wasm-side outlives the call and there is nothing to free. The
  write path still hands out handles (`FormatOpts`, `Edits`, `Splices`); those
  are freed explicitly, with `--weak-refs` as the backstop.
- **Nothing rich crosses.** Flat `Uint32Array`s (copies, never views into wasm
  memory) plus a few strings. JS never holds a token. The one structured export
  is the diff skeleton — a cold, modal-open path where serde JSON buys back the
  old editors' camelCase contract verbatim.
- **Intra-verse highlighting is opt-in.** `diff`'s `text_mode` is `"none"`,
  `"words"` (UAX-29) or `"chars"` (graphemes); an unknown name REJECTS rather
  than defaulting. `"none"` builds no CST and no mask and yields the JSON the
  door returned before runs existed. The runs ride on a `modified` unit's
  `text: {baseline, current}` as `{from, to, kind, what}`: UTF-16 spans into
  that side's own document, `what` one of `"markup"` | `"text"` |
  `"whitespace"`. They TILE the unit's span, so a consumer decorates
  `source.slice(from, to)` instead of searching for a run's text; dropping
  `what == "markup"` and concatenating the rest gives the `"text"` mask cut of
  the same span. `unchanged` and `moved` units carry NO `text` key — a pure
  move must not highlight, and a whole-Bible skeleton is mostly those.
- **UTF-16 offsets, LF-canonical input.** Every offset out is a CodeMirror
  code-unit offset. That assumes the text is LF-normalized: CodeMirror counts a
  line break as ONE position, a literal `\r\n` is TWO UTF-16 code units, so
  CRLF input yields offsets one ahead of the editor's from the first line
  onward. The vision canonicalizes at ingress (§6.3) — LF-in is the contract,
  not something `parse` repairs. Debug builds assert it.
- **The marker registry never crosses.** Marker names are bytes the editor
  already has (`doc.sliceString`); the coarse rendering class rides packed in
  the spans; lint codes index `diagnostics.json`.

## Deliberately absent

- **A checksum export.** Vision §13.4 puts the canonical-source checksum on
  this facade; Will deferred it (2026-08-24) as a higher-level concern — revisit
  when sous joins and the host owns a source version. Every dish's header does
  carry an xxh3-64 of the source it was plated from (`Dish.sourceHash`), which
  answers "is this the parse of that text"; it is not a source identity the host
  can hand around.
- **Any stateful handle.** Ruled: stateless first. If a profiler ever catches
  the per-call index build mattering, an opaque handle is the documented
  fallback — do not pre-design it.
