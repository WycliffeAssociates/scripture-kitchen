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
  to compare; criterion then.

## Parked (do not start)

Editing/session layer (live ids, re-scan on edit, lint anchoring while
editing), multi-book project file format (toc, checksums, caches),
diff/merge, publish. Each gets designed against this base once it holds.
