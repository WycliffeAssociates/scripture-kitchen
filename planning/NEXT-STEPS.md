# Next steps

Slow on purpose. Plain language so this can be picked up cold after a break.
Design state and open questions live in QUESTIONS.md / GLOSSARY.md; this file
is only "what code to write next, in what order."

## 1. Stub the core structs (agreed shape)

- **Token** — one compact row per token: `start u32 · len u16 · kind u8 ·
  markerIdx u8` (8 bytes). Text is always a slice of the source; nothing
  address-shaped lives on the token. `markerIdx` indexes the marker table;
  one reserved value means "custom marker — read the span."
- **Header** — what one scan of a book discovers about its structure: the
  book code as a *slice* of whatever came after the first `\id` (any length,
  invalid included, never truncated), plus the chapter run table: one entry
  per `\c` (row range, label slice, repeat-ordinal for reopened chapters).
  This doubles as the editor's nav toc and the "materialize just one
  chapter" index.
- **The scan signature** — one fused pass, several outputs:
  `scan(source) -> (Slots, Header)`, with an optional event/diagnostic
  sink for listeners (editor tree-building, lint) that costs nothing when
  nobody is listening. **Columns are grouped per chapter SLOT with
  slot-relative spans from day one** (slot 0 = front matter) — the run
  table IS the slot list, and this is what makes the future stateful layer
  a retention of the same types instead of a migration (see QUESTIONS.md
  "E — Statefulness and granularity"). `to_tokens(slot)` is where owned
  allocation happens; a stateless whole-book call is just the loop.
- Playground prints all three so driving stays visible.

## 2. Split the arms (the fused shape)

Restructure the current ~200 lines so each arm has one job:

- **Cursor movement** is its own thing (`find_boundary` — the memchr work).
  Only this code advances position.
- **Classification** names the shape of a slice. It MAY read marker-table
  columns and keep pass state (the ws-fold flag today; a small open-marker
  stack later). It may NEVER parse the inside of a payload (attribute
  key/values, verse numbers) — that's an interpreter's job, later, on
  demand.
- **One fused pass** — the walker runs inside the scan, not as a second
  pass. The same walk emits tokens + header (+ events when a listener is
  attached).

Rules this step must encode (agreed today):

- The whitespace fold (delimiter space absorbed into the marker's span) is
  **per marker class, from the table** — not unconditional like today.
  Closing markers don't take it; their trailing space is content.
- **Kinds must be enough to render from.** A consumer looking only at
  `kind` should never need USFM trivia (is this space a delimiter? is this
  pipe an attribute?). That's why the fold happens here, and why an
  attribute list becomes ONE `AttrList` token once the stack exists — the
  stack knows a `\w` or milestone is open; a bare pipe in ordinary text
  stays content.
- **Standing test, added now: `concat(all spans) == source`**, byte for
  byte, over testData + example-corpora. If a change wants to break it,
  that's a design event — stop and log it in QUESTIONS.md.
- Log every granularity cut (marker vs number separate, AttrList as one
  token, etc.) in QUESTIONS.md's Q6 as it's made.

## 3. Pull static data from onion (only what's demanded)

DECIDED (2026-08-09): table BEFORE attributes — the attribute parser needs
the open-marker stack, the stack needs the kind column, so attributes-first
would hardcode a shadow table. Session order:
1. Author `marker_rows` in this repo: mechanically translate onion's
   marker_defs_data into the new schema (throwaway script), then AUDIT
   category-by-category (paragraphs, then char, then notes...) — judgment,
   not typing. ws enums may split into their own module.
2. Codegen binary: rows → packed u128 table + strip-digits-then-match
   name→idx fn, emitted as a generated file. (JS registry later, same
   source.)
3. Wire into lexer: marker_idx assignment · per-class ws fold (kills the
   ScanMode TODO) · payload column → NumberRange token kind (9th shape:
   slide NESTED_BIT to bit 4). Fast-path prelude AFTERWARD, one pattern at
   a time, measured (`\v `+SWAR-digits first); table stores facts, codegen
   emits the checks; general path stays the definition, oracle-verified.
4. Then attributes, on the stack the kind column enables.

Extract columns from onion's marker_defs as arms actually ask — never the
whole table on spec:

- **Now:** the spine (marker name → index) — required the moment tokens
  carry `markerIdx` — and the **role class** column (paragraph / character /
  note / milestone / closing), which is probably where the whitespace-fold
  rule comes from.
- **Verify rows against the spec docs (tcdocs/) as they're pulled** — this
  IS the table audit, done as demand-driven extraction instead of an
  up-front review of onion's 1,700 lines.
- **Open sub-question, don't decide yet:** what shape the table compiles to
  for fast scanning (plain static array first; codegen / perfect-hash only
  if measurement says lookup is hot — onion's double-HashMap-per-marker is
  the known smell). The JS copy is generated from the Rust table, never
  hand-written.

### MarkerRow schema notes (audited 2026-08, from onion's marker_defs)

Facts gathered so the schema draft doesn't re-derive them:

- **~170 canonical rows** after collapsing numbered spellings (219 unique
  markers, 68 are numbered variants like `q1..q4`). Fits `marker_idx: u8`
  with room; 0 stays "unresolved/custom".
- **Name → index lookup: zero alloc.** Longest spec name is 6 bytes
  (`periph`), so: strip trailing digits, load the name into a u64,
  codegen'd integer `match` (compiler emits the decision tree). Digits are
  validated against the row's numbered-max column; the number itself is
  never stored — it's in the token's span.
- **Everything enumerable fits ~77 bits** (kind 4 · context-mask 20 ·
  ws requirements 16 · paragraph-category 4 · family/note/inline/block/
  scope/closing ~18 · payload-id 2 · numbered-max 4 · flags). Row is a
  u128 in spirit (u64 + u32, or two-u64 windows — codegen's choice), which
  leaves ~50 spare bits. Approved use of spare bits: default HTML element
  class (4 bits, ≤16 element names; numbered markers store the element
  CLASS, export computes the level from the span's number). Overridable —
  it's a default.
- **Three side arrays, not bits**: marker names (idx → name), default
  attribute strings (only 6 distinct: lemma/gloss/link-href/loc/src/who),
  doc paths (codegen-only). Flatten onion's `qt*-s/-e → who` suffix RULE
  into plain rows at codegen time.
- **Authoring format**: Rust rows with named struct fields (compiler-checked,
  like onion's marker_defs_data); a generator emits the packed runtime
  table, the u64 match, and the JS/TS registry (editor-side "is this a
  marker / what kind / own line?"). Bit layout is generator output — no
  hand-maintained masks. `priority: u8` may ride as a codegen-only column
  ordering a hot-marker fast-path prelude — build only if the lookup
  profiles hot.
- **Open**: can context-machine TRANSITIONS (what pushes/pops/implicitly
  closes) be fully table-encoded, or do a few hand rules remain around the
  table? (onion: `structural_marker_info` + `closes_unclosed_note` suggest
  mostly-table-with-exceptions.)
- **Open**: number-PAYLOAD tokens (`\v 12`) — separate NumberRange token
  kind (onion's way; editor re-fuses at node level, proven in proto-2's
  USFMNumberedMarkerNode) vs fused marker+number token. Either needs the
  payload column + a pending-payload flag on ScanMode. Lean: separate.

## Later (when we're actually writing code again)

- **Verse designator interpreter** — pure text rules (digits, ranges,
  suffixes, junk → "not cleanly numeric" flag). Zero table. Writing its
  doc-comment IS writing the comparison rules lint/vref need.
- **Structure events → editor tree.** Measure the wasm route FIRST before
  building any JS twin; consumer-sufficient kinds may have shrunk this
  problem a lot.
- **Observation (lint finding) shape**: `{ code, anchor token, optional
  second token }` — severity/category/template live in a rules table;
  message rendering and localization happen on the consumer's side. Audit
  onion's messageParams first to confirm nothing non-derivable is lost.
- **Benchmarks**: playground timing until there are two real alternatives
  to compare; criterion then. Baselines banked (2026-08, src/experiments/):
  serial 1.3 GiB/s prose / 1.15 GiB/s aligned; scalar floor 441 MiB/s (no
  SIMD is still fine — wasm worst case covered); chapter-par hits the
  machine ceiling (~4 GiB/s) on big books, LOSES on small ones — if ever
  shipped it's a measured size threshold, and only a whole-project-open
  tool. The `--chunked` verify (token-identical split at `\c`) is standing
  evidence that no scan state crosses a chapter — the slot model's
  independence claim, proven on both corpora.
- **Two-stage structural indexing (the simdjson trick): TRIED, a wash.**
  Built as `experiments/staged.rs` (2026-08-09), verified token-identical:
  +6% on prose, −4% on marker-dense ult. Why it doesn't transfer: the
  position tape is ~50MB of extra write+read traffic on ult, and our
  per-token work is too thin to amortize it (simdjson's stage 2 does heavy
  parsing per position; ours is an 8-byte push). REFINED CONCLUSION: the
  lexer's cost is the token pushes + arm logic themselves, not scan-call
  overhead — so future speed comes from EMITTING FEWER TOKENS (e.g.
  AttrList as one token), which is the consumer-sufficiency direction
  anyway. ~1.1-1.3 GiB/s/core is what this granularity costs; accept it.
- **Feature-gate exports to shrink the wasm bundle** (e.g. no USJ/HTML/XML
  export unless built with it). Cargo features work the same for wasm
  targets (`wasm-pack build -- --features …`); compiled-out code never
  enters the .wasm. The catch: a wasm blob does NOT tree-shake like JS —
  whatever is compiled in ships to every user — so features matter MORE
  here. Feature combos = separate build artifacts; the JS wrapper can
  dynamic-import the right blob.
- **Codegen USJ types** as the product of Token × marker table (the lossy
  normalized kv projection): the discriminated union + serializer fall out
  of the same generator that emits the JS registry. MUST map to official
  USJ type names (`para`, `char`, `note`, `ms`, `book`, `chapter`,
  `verse`, `optbreak`…), which differ from our vocabulary — check the USJ
  JSON schema in usfm-grammar before naming anything.

## Parked (do not start)

Editing/session layer (live ids, re-scan on edit, lint anchoring while
editing), multi-book project file format (toc, checksums, caches),
diff/merge, publish. Each gets designed against this base once it holds.
