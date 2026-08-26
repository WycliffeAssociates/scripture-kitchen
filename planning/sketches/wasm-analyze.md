# wasm sketch (roadmap item 2 — build when the editor pulls)

**NAMING CORRECTED 2026-08-25 (pass 10).** Everywhere below that says the
bindings crate is `galley/` — including the "ONE combined crate, no standalone
onion-wasm/sous-wasm" ruling — read **`onion-wasm/`**, and `galley.ts` as
`onion-wasm.ts`. `galley` is the RESERVED name for the fifth crate, the
workflows layer over onion + sous (ideas/committed/galley.md); this crate is
piece 2 and will be one of its dependencies. The rest of the sketch stands as
written. Also amended in pass 10: `analyze` returns a PLAIN JS OBJECT rather
than a handle, so the `.free()` discipline the sketch describes applies only to
the write-path handles (`FormatOpts`, `Edits`, `Splices`), with `--weak-refs`
as their backstop.

**BUILT 2026-08-24** (pass 7 — see planning/choices.md for the self-report and
the three points wanting Will's eyes). Where it landed: the emit layer is
`src/analyze.rs` (+ `utf16::Cursor` beside the index it mirrors), the
diagnostics side-table is `lint::catalog` → `galley/diagnostics.json` (codegen,
checked in, staleness-tested), the bindings crate is `galley/` (Will's ruling:
ONE combined crate, no standalone onion-wasm/sous-wasm), and the TS wrapper is
`galley/galley.ts`. Two amendments to what is below: the checksum export is
DEFERRED (Will, 2026-08-24 — §13.4 is the composed facade's, revisit when sous
joins), and `analyze` documents an LF-CANONICAL input contract for its UTF-16
offsets.

Rewritten 2026-08-24 in plain terms after Will's questions. Same
decisions as before (RULED markers kept); the shorthand is gone.

APPROVED IN PRINCIPLE (Will, 2026-08-24), conditioned on what the
design already claims: structurally drift-proof (one implementation,
exports derived from the native API, ids and classifications rendered
in Rust only, the TS wrapper versioned with the binary), sane code
size (~1,000–1,400 lines all-in, half of it ordinary native Rust), no
parallel API (analyze is an export SIBLING of usj/usx/html over the
same fns), and the onion_editor_chef CM probe's needs served (the
inventory table below — its hand-authored PARA/HEADING/FRONT regexes
are replaced by the registry's class byte, its blockAt by span
containment). Build trigger unchanged: when the editor pulls.

## What "wasm" means for this crate, from zero

WebAssembly is just a second compile target. `cargo build --target
wasm32-unknown-unknown` compiles the SAME Rust — lexer, CST, lint,
toc, mask, format, diff, the baked marker table, everything — into one
`.wasm` binary that a browser (or Node) loads. There is no port, no
rewrite, no second implementation: the measurement probe already
compiled this engine to wasm unchanged, because it's pure Rust with no
IO. The library IS wasm-ready today; what doesn't exist yet is the
doorway.

**The doorway is `#[wasm_bindgen]`, and tagged = the API.** JS cannot
call arbitrary Rust; it can call the specific functions we tag. What
you get for a tag (Will's question, answered): `wasm-pack build` emits
an npm package containing the `.wasm`, a JS glue module whose exports
ARE the tagged functions, and a **first-class `.d.ts`** with a typed
signature for each — `&str` becomes `string`, `u32` becomes `number`,
`Vec<u32>` becomes `Uint32Array`, a tagged struct becomes a JS class
with typed getters. So yes: the tagged set is exactly the TypeScript
surface a consumer sees and autocompletes against. We do NOT tag
types, traits, or internals — those live inside the binary. The
exports list below is maybe ten functions; the bindings crate holding
them is ~100–200 lines, plus a small hand-written TS wrapper that
ships IN the same npm package (below).

**Why it's a separate tiny crate instead of tags in this one:** a
wasm-callable binary needs `crate-type = ["cdylib"]` in Cargo.toml and
a dependency on `wasm-bindgen`. Putting that here would make every
native consumer carry wasm-bindgen for nothing. So: `usfm_onion_2`
stays a plain library, and a sibling `onion-wasm/` crate (a workspace
member) depends on it, tags the exports, and is what `wasm-pack build`
runs against. The substance stays in the library; the bindings crate
is a doorway you could delete and recreate in an afternoon.

## Where the costs are (and are not)

There is NO process hop. The wasm binary runs inside the same JS
thread that calls it — a call into wasm is a function call, nanoseconds
of overhead. (If we ever move it to a Web Worker for off-main-thread
work, the hop becomes a `postMessage` copy; a native sidecar process
over IPC would be a real process hop. Both are RULED parked: sync
main-thread first, worker later, IPC behind both.)

The real costs, measured (perf-notes §5):

1. **Strings crossing INTO wasm.** JS strings are UTF-16; Rust wants
   UTF-8. Passing a document in = one encode pass, ~1 GB/s in V8:
   296µs for all of Psalms, 38µs for a median book. This is the
   biggest boundary cost and it's still ~100x under a frame.
   wasm-bindgen handles the conversion when the export takes `&str`.
2. **Data crossing OUT.** Copying already-produced bytes out of wasm
   memory is effectively free (tens of GB/s — 4.5µs for all of
   Psalms). What matters is only HOW MUCH we choose to copy, which is
   why the `wants` bitmask exists (below).
3. **Offsets mean different things on each side.** Rust speaks byte
   offsets into UTF-8; the editor (CodeMirror) speaks UTF-16 code-unit
   offsets. Every offset we hand JS must be converted. This is solved
   and cheap: converting every emitted offset in one document-order
   sweep costs ~0.4ms on the worst book, and the random-access index
   (for cursor positions coming the other way) is 1.6% of source size,
   ~100ns per query, built in under half a millisecond.

## What crosses well, what crosses badly (the onion lesson)

- **Numbers and arrays of numbers**: perfect. A `Vec<u32>` becomes a
  JS `Uint32Array` copy automatically. This is the shape of almost
  everything we ship.
- **Strings**: fine in moderation — pay the encode once per call.
- **Rich structs/objects**: this is where onion's wasm crate became a
  beast, and the trap to avoid. If JS holds a structured object (a
  token, a skeleton), you need a JS-facing mirror type, a mapping
  layer, serde, and tests for the mapper — and the mapper silently
  drops any field it doesn't know about (onion's attribute-loss bug
  lived exactly there). Our RULED contract — **JS never holds a
  token** — deletes that entire category. Nothing rich crosses;
  everything is flat arrays of numbers plus a few strings.

**"Isn't a flat stride array just a binary format?" Honestly: yes**
(Will called this out, 2026-08-24, and the earlier draft oversold the
distinction). A stride-7 Uint32Array of diagnostics IS a fixed-width
record format. The real distinctions are narrower and worth stating
plainly:

- **Width discipline**: every field is a whole u32 slot, so JS reads
  plain numbers out of a typed array — no DataView, no endianness, no
  mixed-width unpacking. That's the difference from freezing Rust
  struct MEMORY layouts (`#[repr(C)]`) and letting JS reinterpret wasm
  memory — which we never do; repr(C) on Toc rows stays internal
  hygiene, not a wire promise.
- **Who decodes (RULED 2026-08-24: we do)**: raw `arr[i*7+1]` indexing
  is NOT the consumer experience. The npm package ships a thin
  hand-written TS wrapper over the arrays — `analysis.diagnostics()`
  yields `{code, from, to, aux, fix}` objects (lazily, off the array,
  no copy of the array itself), same for chapters/blocks/etc. The
  schema then lives in ONE place we own and version with the binary;
  consumers never see a stride. The arrays stay the wire because
  they're the cheap thing to copy out; the wrapper is ergonomics on
  the JS side of the wall, ~a hundred lines, in the same package so it
  can never drift from the .wasm it wraps.

**The marker registry never crosses.** `marker_idx` indexes the baked
codegen table, and that table is static data compiled INTO the .wasm
binary — lookups happen inside, in Rust. JS never decodes an idx.
What JS actually needs it gets two other ways: the marker NAME is
bytes the editor already has in its own document (`doc.sliceString`),
and the coarse rendering class rides packed inside the spans we ship.
The ONE build artifact resembling a table crossing: a small `.json`
side-table for diagnostics (per lint code: name, severity, message
template, fix label), codegen'd from LINT_ROWS by the existing codegen
bin and shipped in the same bundle — so no strings need to cross per
finding, and the JS bundle can render "\\f was never closed" itself.

## Why not just tag lex/cst/lint/mask and compose in JS like the playground?

Will's question, and the answer is a hard technical wall plus a soft
one — not taste:

- **Lifetimes cannot cross.** The core types borrow: `Token<'a>` spans
  the source, `Cst` borrows the tokens, `Mask` borrows all of it.
  `#[wasm_bindgen]` can only export OWNED, `'static` values — there is
  no way to hand JS a borrowed struct. Making the pipeline
  JS-composable would mean making every intermediate owned (cloned
  tokens, cloned text) — which is exactly onion's shape, the thing the
  no-JS-holds-a-token ruling exists to kill.
- **Opaque handles are the other trap.** wasm-bindgen CAN export a
  struct as an opaque JS handle with methods — but every handle is
  wasm-side state JS must manually `.free()` (GC does not manage wasm
  memory; forget one and it leaks), and now the "stateless, race-free"
  contract is gone: a handle from text-version-5 can be queried after
  the text became version 6.
- **Each JS→wasm call pays the string encode again** unless state is
  retained, so JS-side composition would re-send the text per stage or
  force the handle problem above.

So the composition happens in Rust — `analyze()` IS the playground
pattern (lex → build → lint → emit) moved behind one tagged function,
borrows alive inside, nothing owned, nothing retained. To be explicit
about status: **the `Analysis` type does not exist today.** Building it
— the emit layer that walks the artifacts and writes the arrays — is
the actual work of this milestone (~a few hundred lines of pure,
natively-tested Rust). The tag is the trivial part; nobody should read
`analyze()` in this sketch as existing code.

## The JS-facing API (what gets the `#[wasm_bindgen]` tag)

All stateless: text goes in, arrays/strings come out, nothing is
retained between calls (RULED — race-free by construction; the caller
pairs each result with the document version it sent, stale = discard).

```rust
// The one read call. `wants` is a bitmask: one bit per read below.
// Nothing is computed or copied for an unset bit.
analyze(text: &str, wants: u32) -> Analysis   // typed-array getters

// The write paths (both exist natively today):
format_edits(text, opts) -> edits wire        // offsets utf16, eager
format_edits_in(text, from, to, opts)         // the same, scoped to a
                                              // utf16 window (pass 14)
format(text, opts) -> String
diff(baseline, current) -> skeleton wire      // shape decided at build
merge(baseline, current, decisions_json) -> String (or splice wire)

// Offset translation for the stragglers (cursor → sid, etc.):
to_byte(text, utf16: u32) -> u32
to_utf16(text, byte: u32) -> u32
// Rebuilds the 1.6% index per call (~0.1ms) — honest and stateless at
// a handful of calls per user interaction. If that ever measures
// dumb, an opaque handle is the fallback; don't pre-design it.
```

**What `analyze` ships (the "reads"):** each is one flat u32 array,
fixed stride, offsets in UTF-16. The editor asks only for what it
renders — reads differ in cost by 1000x (diagnostics: hundreds of
entries; token_spans: 12 bytes × every token ≈ 4MB on aligned GEN,
hence the bitmask, plus an optional clip range that bounds
token-granularity reads to the viewport while analysis stays
whole-book).

| read | numbers per entry | the fields, spelled out | serves |
|---|---|---|---|
| `chapters` | 7 | chapter ordinal (+ the number-shaped flag in bit 31); where its `\c` starts; where its label ("12b") starts/ends; where its content starts; where the whole chapter starts/ends | nav grid, chapter clamp, `\c` chrome |
| `blocks` | 4 | packed class word (below); where the paragraph-level block starts/where content starts/ends | paragraph rendering |
| `lines` | 4 | packed class word; where the MARKED line starts/where content starts/ends — one row per line whose first non-whitespace token is an opening marker | every line-level editing rule |
| `note_extents` | 3 | which note family (\f/\x/\ef/\ex/\fe); where the whole note starts/ends | footnote widgets |
| `note_parts` | 4 | which note; which part (caller/origin/body/markup); where it starts/ends | the note apparatus |
| `token_spans` | 3 | class word + kind byte (`kind << 16`); where the token starts/ends | syntax highlighting |
| `text_runs` | 2 | where a run of plain text starts/ends | search/proofing views |
| `verse_anchors` | 5 | which chapter it's in (+ the number-shaped flag in bit 31); where the `\v` starts; where the verse NUMBER starts/ends; where content starts | verse number widgets, `\v` chrome |
| `diagnostics` | 7 | which lint code; primary span start/end; secondary span start/end (MAX = none); the code's aux integer; index of its fix (MAX = none) | squiggles + panel |

Plus one SCALAR beside the reads: `usfm_version`, the ladder index the `\usfm`
line declares (MAX = undeclared). Several diagnostics are GATED on it — silent,
not merely quiet — so a consumer that could not see it had to re-read the
document's header to know whether a finding was showable at all.

**Two designator facts the reads carry** (added 2026-08-24, pass 8, from the
spike's integration): the marker/content offsets, because an editor hides
`[marker_from, number_from)` and `[number_to, content_from)` and puts the caret
at `content_from`; and the NUMBER-SHAPED flag in the number field's high bit,
because a designator is positional — delete the number and the next word becomes
the designator, so a read without that flag styles scripture as a verse number.
An ABSENT designator reports its empty span AT `content_from`, the propped-open
slot, not at the marker's start.

**`content_from` is the label's end plus ONE delimiter byte** (fixed 2026-08-25,
pass 11, from Will driving the editor). The SCANNER folds a marker's or
payload's whole horizontal-whitespace run into that token and always will — that
is the token contract. The READS do not: every `content_from` on `chapters`,
`blocks`, `lines` and `verse_anchors` is `label_end + 1` when a run followed, and
`label_end` when none did (a line ending after `\v 1` leaves no delimiter to step
over). So `[label_end, content_from)` is the DELIMITER, derivable, one byte or
empty; bytes past it are leading CONTENT whitespace — visible, editable, and
format's delimiter-single row's to trim. Emitting the token's end instead meant a
space typed at `content_from` was absorbed into chrome on the next analysis and
the editor hid the byte the author had just typed. The same rule governs
`note_parts`: CALLER is the label, MARKUP is the marker plus one delimiter, and
the remainder of a folded run comes back as ORIGIN/BODY.

("Where X starts/ends" is always a UTF-16 offset pair into the
document JS already holds — the editor slices its own text for any
display string, e.g. the marker name or the label. That, not the
array format, is the load-bearing trick: spans instead of strings.)

The packed class is 3 bits (para/char/note/milestone/chapter-verse/
sidebar/table/other) + six flag bits (heading/front/poetry/closer/
unknown/meta) — a 16-bit WORD since pass 8, riding a whole u32 slot;
enough for the editor's shape decisions, and per-marker CSS keys off
the marker name, which is document bytes JS already holds. META splits
FRONT into the machine header (`\id \usfm \h \toc1`, the
Identification + DocumentStructure categories) and introduction prose
the reader sees (`\ip \iot \is \imt`) — the one marker-name regex
the class byte was supposed to delete, answered from the table. Diagnostics carry a
per-build `code` number that indexes the codegen'd JSON side-table;
message text renders JS-side from the template + document slices —
zero strings cross per finding (RULED). Fixes cross EAGERLY (ruled
2026-08-24): format_edits proved the shape at 1000x the volume —
`[from, to] × n` + one concatenated ASCII insert string + a length
array.

Everything above was checked against what the CodeMirror prototype
(onion_editor_chef) actually consumes — every prototype need maps onto
a read, no gaps.

## What lives in THIS repo vs the combined crate (RULED 2026-08-24: galley, and only galley)

Will's ruling supersedes the three-doorways option: NO standalone
onion-wasm, NO standalone sous-wasm — ONE combined bindings crate,
named **galley** (wins over "braid"; the vision's "composed analysis
host" gets its name). Two fat libraries under it:

- **`usfm_onion_2`** (this repo) — plain Rust library. No wasm
  anything, ever.
- **`sous`** (sibling repo) — plain Rust library, same deal.
- **`galley`** — the crate the EDITOR actually loads: today it wraps
  onion alone; sous joins it later; linked into ONE wasm binary.

Checksum (vision §13.4 asks the composed facade for xxh3-128-v1):
RULED OUT of this build by Will — a higher-level concern; deliberately
absent, revisit when sous joins galley.

LF contract at the galley wall: emitted UTF-16 offsets assume
LF-normalized input — CodeMirror counts every line break as ONE
position (it normalizes to LF internally) while a literal `\r\n` is
TWO UTF-16 code units, so CRLF input would disagree with editor
positions. The vision (§6.3) commits the editor to LF canonicalization
at ingress; galley documents the contract rather than repairing input.
The engine stays byte-honest for any input; format's normalize-newlines
row is the repair tool when a consumer needs one. This is where the composed
  flow lives (analyze + proofread over the same mask, one diagnostic
  stream, all offsets in source bytes until the one UTF-16 wall), and
  crucially it is where STATE lives if any ever exists: the retained
  source string, the shared Utf16Index, caching, debounce
  coordination. The engines stay stateless forever; galley is braid's
  stateful ancestor.

Why one binary instead of editor-ferrying between onion.wasm and
sous.wasm: every hop between two wasm modules re-copies and re-encodes
the text. Rust→Rust inside one binary is a borrow — sous reads the
mask onion built with no boundary at all. Two modules is justified
only if they must DEPLOY independently, which nothing requires.

So the decision rule for "where does an API live": if it's engine
truth (lint, format, diff, exports), it's a library fn here, and the
doorway crates merely tag it. If it needs both engines or any retained
state, it's galley's. Nothing is ever implemented IN a bindings crate.

## Galley method list (accumulating as real UI asks arrive)

- `new(text: String) -> Galley` — the one string crossing
- `diagnostics()` — onion lint + sous proofread, one stream, source bytes
- `locate(text, utf16) -> String` — "MRK 6:3" (status bar, labels) — EXPORTED
  2026-08-24 (pass 8); the sid RENDERING has rules (bridges, an absent chapter,
  an unknown book) and a JS re-implementation in every consumer is exactly the
  drift no-strings-cross exists to prevent
- `chapters()` — the navigation grid rows
- `book(text) -> String` — the \id code (EXPORTED 2026-08-24, pass 8)
- `to_utf16(byte)` / `to_byte(utf16)` — shared index, built once per text
- `usj()` / `usx()` / `html() -> String` — exports on demand
- `format_edits(opts)` / `format(opts)` — the opt-in write path (2026-08-24)
- `format_edits_in(from, to, opts)` — the SCOPED write path (2026-08-25,
  pass 14): whole-book analysis, filtered to a window. Straddling edits
  are dropped whole and atomic claims kept whole, which is exactly what
  a JS-side filter of the whole-book list cannot do.
- `diff(other)` / `merge(other, decisions)` / `revert(other, unit)` —
  the two-input pair (2026-08-24)
- `find(query)` — memmem/regex over a mask's text, hits mapped to
  source spans via to_source; regex is a GALLEY dep (2026-08-24,
  ideas/candidates/find-and-overlay.md — no new onion capability)
- overlay workflow (skeleton refill) — the Sid-keyed splice PRIMITIVE
  is an onion candidate beside diff; the command/policy is galley's
  (same note)

## The one rich structure: the diff skeleton (RULED 2026-08-24: serde JSON, B′ fallback)

Will's ruling: the skeleton crosses as serde JSON (option A) — cold
path, zero drift, the old editors' contract back for free. The
documented fallback if a profiler ever catches the modal parse
mattering: flat unit arrays PLUS one Rust-rendered id-string blob with
offsets (B′). Plain flat arrays with JS-rendered ids are REJECTED —
re-rendering "MRK 6:3_dup_1@2" in TS recreates the mapper-drift class
in the one place identity is load-bearing; the id renderer stays Rust.


The diff skeleton is the single API where JS wants structure (units
with ids, statuses, slots) rather than spans — and it's a cold path:
invoked when a diff modal opens, behind async loading, not per
keystroke. Will's ruling: serde is fine for exactly this. The clean
way to take it: serde (+ serde-wasm-bindgen or serde_json) becomes a
dependency of the BINDINGS crate only, deriving on small wire structs
defined there and converted from the engine's `DiffSkeleton` — the
engine library stays serde-free, and the wire structs are the bindings
crate's to version. That gives the old editors' camelCase JSON
contract back nearly verbatim without hand-writing a serializer.
Replay splices still cross like format edits (spans + ranges, hot-path
shaped).

## Tests (intentionally minimal)

The emit layer — building the arrays, the UTF-16 sweep — is pure Rust,
so it's tested NATIVELY like everything else (reference-emitter
equality over the corpus). The boundary itself gets ONE smoke test
(`wasm-pack test --node`): a real book in, a handful of known values
out, including one UTF-16 offset on a Hindi book checked by hand. No
JS test harness beyond that — with no mirror types there is nothing
boundary-specific left to drift. No wasm benches in-repo: boundary
costs are recorded in perf-notes §5; the probes stay scratch (RULED).

## Standing decisions (compressed history)

- Stateless one-call analyze; JS never holds a token; typed-array
  copies, never views into wasm memory. (RULED 2026-08-18)
- `wants` bitmask + viewport clip: confirmed. Coarse 3-bit class:
  confirmed. Eager fix crossing: ruled 2026-08-24.
- `str_indices` crate approved-in-principle as the UTF-16 counting
  primitive; the hand-rolled SWAR is fallback + test oracle.
- Serde: never in the engine library. Allowed in the BINDINGS crate
  for the diff skeleton (ruled 2026-08-24 — cold path, modal-open
  shaped). The other future argument is usj/usx INGEST, parked as
  not-first-class.
- Perf envelope: engine ×2–3 native cost under wasm ⇒ worst aligned
  book ≈ 20ms, prose book ≈ 200µs, against a 150ms debounce. simd128
  RUSTFLAG is a free ~8% on lex; turn it on, expect no more.
- Build when the editor pulls; pure Rust until then. (Standing)

## Distribution (RULED 2026-08-25, pass 10)

**Not publishing to npm yet.** Consumers install from GitHub tags; the spike
vendors the package with `scripts/sync-engine.sh`. The built output is
COMMITTED, mimicking the old `../usfm_onion` layout (and wa-sqlite's):

- `onion-wasm/pkg-web/` — `wasm-pack build --target web --weak-refs`, the
  explicit-`init()` path the CM spike uses.
- `onion-wasm/pkg-bundler/` — `--target bundler --weak-refs`, the default.
- Both are in the tree (`.gitignore` keeps only the scratch `pkg/` and
  `pkg-node/` out); `wasm-pack`'s own `.gitignore` of `*` is deleted from each
  out-dir by the sync script, or the artifacts never reach a tag.
- `onion-wasm/package.json` (hand-written, above the two generated ones) maps
  the subpaths: `.` → pkg-bundler, `./web` → pkg-web, `./web/wasm` → the binary,
  `./schema` → `onion-wasm.ts`, `./diagnostics.json` → the side-table. The
  wrapper and the side-table are reachable from BOTH builds, as they must be:
  they are versioned with the binary.

**One build per target, full default features** (usj/usx/html on). No
all-vs-lean prebuilt variants: the binary is ~370 KB / ~152 KB gzipped and no
size-sensitive consumer exists. A consumer who wants a lean binary builds from
source with `default-features = false`; prebuilt lean variants are a later
decision if one ever asks.

Still open when npm publishing matures:

- **The package lives in a SUBDIRECTORY of the engine repo.** `npm i
  github:org/usfm_onion_2#tag` installs the repo ROOT, not `onion-wasm/`, so a
  tag-install today needs a subdirectory-aware tool or a split publish repo.
  Decide before telling a consumer to install from a tag.
- **`sideEffects`** — wasm-pack emits `["./snippets/*"]` per generated package;
  the hand-written one names the two glue modules explicitly so a bundler does
  not tree-shake the module's init away. Worth re-checking against a real
  bundler consumer.
- `"private": true` sits on the top-level package.json so nothing can be
  `npm publish`ed by accident; it does not affect a git install.
