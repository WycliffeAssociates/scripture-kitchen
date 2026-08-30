# FlatBuffers analysis wire candidate

**State:** candidate only; discussed 2026-08-28, not committed or scheduled.

**2026-08-29 — partly superseded.** This document asks how to serialize
`analyze`'s output. `parse-handle-boundary.md` asks whether that output should
exist, and measures that (a) one buffer instead of thirteen typed arrays saves
9 microseconds on the largest book, 0.2% of a wasm `analyze`, and (b) a
wasm-bindgen accessor call is ~5 ns, so an editor can ask questions instead of
decoding planes. A first draft of that document kept this
question alive for the **Tauri/native** path, where a call is an IPC round trip.
That was withdrawn on 2026-08-29: wasm ships in the editor on every platform,
Tauri included, so the keystroke path never crosses IPC and no read on it is
async.

IPC keeps a real job — the COLD project load, which is native because the
filesystem and the thread pool are (66 books read, lexed, built and linted in
28.80 ms across 10 threads). But its payload was measured too, and it does not
want this format either: per-book severity counts for an entire Bible are
**4.3 KB of JSON**, and even a full project-wide findings list with messages
rendered native-side is 694 KB one time. The binary-row alternative saves 25% of
those bytes and costs a seven-u32 stride mirrored into TypeScript — a bad trade
against the one-definition goal.

**Neither door needs this format for its PAYLOAD.** One narrow case survives and
is worth reading in `parse-handle-boundary.md`: if the CST buffer is shipped for
JS-side traversal, five structural layout facts must be known on both sides, and
generating them beats writing them. flatc covers two of the five (the packed bit
fields are inexpressible), or four if the packing is abandoned for a
byte-identical flat struct. But it puts the SSOT in a `.fbs` when this repo's
SSOT is the Rust, cannot generate the 153-row marker table at all, and adds a
C++ build tool — so extending `onion/src/bin/codegen.rs` covers 5 of 5 with no
new dependency. **Superseded again (same day):** a wasm-bindgen'd deserializer that OWNS its
bytes reads at 4.6 ns and leaves JS with ZERO layout facts, so nothing needs
generating for the JS side at all. This format wins only if consumers outside
this repo, in languages we do not ship, need to read the tree — and even then,
only for those consumers. Two further corrections to the sketch below: its struct vectors for
`blocks`/`tokenSpans`/`diagnostics` would regress the hot path (flatc gives a
`Uint32Array` accessor for `[uint]` and not for a struct vector), and the
constant/bit-layout duplication it is partly motivated by is a codegen job
(`onion/src/bin/codegen.rs`), not a wire-format one.

## The question

Onion already returns nearly wire-shaped reads from `onion/src/analyze.rs`:
flat, fixed-stride `Vec<u32>` planes, indices instead of nested objects, one
sentinel, and strings only where the engine invented text. Sous Chef v2 is
expected to add corpus findings, variable rule arguments, source keys, and a
native/WebAssembly split like scripture-editor-proto-2:

- Tauri should run the Rust implementation and return binary IPC to the UI;
- a web-only build should run the same semantic implementation in wasm;
- TypeScript should decode one contract rather than maintain native and wasm
  mirror objects;
- the same bytes may eventually be useful as the persisted validated-findings
  cache, while resident engine state remains rebuildable and unpersisted.

Would FlatBuffers be a better boundary than either normal Serde/JSON IPC or a
hand-authored ArrayBuffer directory?

## Terms: flat words are not the same as bitpacking

Ordering is not what makes a representation bitpacked. Ordering matters for
delta encoding and operations such as Onion's ascending UTF-8-to-UTF-16 sweep;
it is not a prerequisite for packing fields into bits.

The current `Analysis` is best described as a mature **flat-word protocol**:

- every read is a fixed-stride `Vec<u32>`;
- `NONE == u32::MAX` is the one absent-value rule;
- related arrays use indices rather than owned subobjects;
- fix inserts are one concatenated string with parallel length and span planes;
- token class/kind, flags, and the shaped-number bit are genuinely bitpacked;
- most offsets, codes, indices, and auxiliary values still occupy a full u32.

For example, a diagnostic is seven words, or 28 bytes:

```text
[code, from, to, second_from, second_to, aux, fix]
```

It is reasonable to call Onion roughly 90% of the way to a purpose-built
binary protocol. It is not 90% bitpacked.

FlatBuffers also does **not** use Protobuf-style varints. Scalars are
fixed-width. Its space overhead comes from alignment, table offsets and
vtables, and the length/offset metadata for vectors and strings.

## Current boundary and the possible replacement

`onion-wasm` currently makes a plain JS object and runs
`Uint32Array::from(rows)` separately for every numeric read. That is a simple
and explicit contract, but a full analysis crosses as many separately-created
typed arrays plus `fixText`.

A FlatBuffer would cross as one byte buffer:

```text
FlatBuffer bytes
|
`-- Analysis root table
    |-- formatVersion
    |-- presentReads
    |-- lenUtf16 / usfmVersion
    |-- chapters ---------> vector
    |-- blocks -----------> vector of fixed-width Block structs
    |-- lines ------------> vector
    |-- tokenSpans -------> vector of fixed-width TokenSpan structs
    |-- diagnostics ------> vector of fixed-width Diagnostic structs
    |-- fixes / fixEdits / fixLens
    |-- fixText ----------> UTF-8 string
    `-- optional catalog or variable rule data
```

The root table is the header/directory. Its generated accessors follow the
stored offset directly to `blocks`, `diagnostics`, or any other requested
section. Reading diagnostics does not decode or walk blocks first. A vector of
fixed-width structs also supports constant-time access to row N.

`presentReads` should preserve a distinction the current wasm object does not:

- requested and legitimately empty;
- not requested, therefore absent.

Whether consumers need that distinction is a schema decision, not an assumed
requirement. If they do not, omit the extra bitmask and retain the existing
"empty means either" contract.

## Tiny schema sketch

This is illustrative, not an accepted `.fbs` file:

```fbs
namespace onion.wire;

struct Block {
  class:uint;
  from:uint;
  contentFrom:uint;
  to:uint;
}

struct TokenSpan {
  packed:uint;
  from:uint;
  to:uint;
}

struct Diagnostic {
  id:ulong;
  from:uint;
  to:uint;
  secondFrom:uint;
  secondTo:uint;
  aux:uint;
  fix:uint;
}

table DiagnosticKind {
  id:ulong;
  name:string;
  messageTemplate:string;
  category:string;
  defaultSeverity:ubyte;
}

table Analysis {
  formatVersion:uint;
  presentReads:uint;

  lenUtf16:uint;
  usfmVersion:uint;

  chapters:[uint];
  blocks:[Block];
  lines:[uint];
  noteExtents:[uint];
  noteParts:[uint];
  tokenSpans:[TokenSpan];
  textRuns:[uint];
  verseAnchors:[uint];

  diagnostics:[Diagnostic];
  fixes:[uint];
  fixEdits:[uint];
  fixLens:[uint];
  fixText:string;

  diagnosticCatalog:[DiagnosticKind];
}

root_type Analysis;
file_identifier "ONAN";
```

The likely production schema should use named fixed-width structs for all
stable rows rather than leave some reads as `[uint]`; the mixed sketch keeps
the correspondence with today's `Analysis` visible.

### Struct versus table

Use a FlatBuffers **struct** for a fixed-width, dense record:

- block;
- token span;
- source span;
- a diagnostic whose optional values retain Onion's `NONE` sentinel.

Structs live inline. A vector of them is contiguous and compact.

Use a FlatBuffers **table** when a record genuinely has optional or
variable-width children:

- diagnostic catalog metadata;
- a Sous pattern carrying an optional display string;
- a rule-specific variant;
- a source containing key and text strings.

Tables are offset-based and independently extensible, but a vector of tables
has more indirection than a vector of structs. Do not turn every numeric row
into a table merely to make the schema look object-oriented.

## Variable-width data

FlatBuffers natively represents:

- `string`: length-prefixed UTF-8;
- `[ubyte]`: arbitrary binary data;
- `[string]`: a vector of offsets to strings;
- `[SomeTable]`: variable-sized records;
- unions: discriminated variants;
- a nested FlatBuffer carried in a byte vector when separate composition is
  genuinely useful.

Strings do not force every diagnostic to become variable-width. Repeated or
catalog-owned text should remain out of the finding row.

For Onion, diagnostic names, message templates, category, and default severity
are generated catalog facts. Shipping those strings with every analysis would
be wasteful. Prefer:

```text
analysis:
  catalogVersion
  findings containing compact code indices

package/startup catalog:
  code -> name, template, category, severity ladder
```

Sous strings that genuinely depend on the corpus--a glyph, word, or rule
argument--can use a string pool:

```fbs
table SousAnalysis {
  strings:[string];
  patterns:[Pattern];
}

struct Pattern {
  ruleId:ushort;
  argument:uint; // index into strings
  numerator:uint;
  denominator:uint;
  firstSite:uint;
  siteCount:uint;
}
```

That crosses a repeated string once rather than once per site.

## Eight-byte diagnostic IDs

FlatBuffers supports an unsigned 64-bit `ulong`, which Rust reads as `u64`.
TypeScript must treat a full-range 64-bit value as `bigint`, never `number`:
JavaScript numbers cease to represent every integer above `2^53 - 1`. Pin the
FlatBuffers compiler and TypeScript runtime together and make an above-`2^53`
round trip a boundary test.

An alternative that avoids `bigint` in UI code is a fixed pair:

```fbs
struct DiagnosticId {
  low:uint;
  high:uint;
}
```

First decide what the ID means:

- **diagnostic kind:** prefer a compact u16/u32 `code` per finding plus one
  schema/catalog version; paying eight bytes on every finding buys little;
- **stable finding or suppression identity:** a u64 or 128-bit checksum per
  finding may be justified;
- **human-readable enum mapping:** keep the names/templates in the generated
  catalog and carry only its index on the hot path.

The wire format must not choose the identity semantics accidentally.

## What "building a new buffer" means

FlatBuffers generates the byte layout, offset calculations, readers, writers,
and schema-specific accessors. It does not calculate an Onion block or a Sous
finding.

If Onion remains unchanged, encoding is:

```text
analyze(text)
  -> owns the current Vec<u32> reads
  -> boundary adapter groups rows into generated FlatBuffers structs
  -> FlatBufferBuilder owns the final Vec<u8>
```

In abbreviated Rust-shaped pseudocode:

```rust
let analysis = onion::analyze(text, wants::ALL, None);

let blocks: Vec<wire::Block> = analysis
    .blocks
    .chunks_exact(stride::BLOCKS)
    .map(|row| wire::Block::new(row[0], row[1], row[2], row[3]))
    .collect();

let mut builder = flatbuffers::FlatBufferBuilder::new();
let blocks = builder.create_vector(&blocks);
let fix_text = builder.create_string(&analysis.fix_text);
let root = wire::Analysis::create(
    &mut builder,
    &wire::AnalysisArgs {
        len_utf16: analysis.len_utf16,
        blocks: Some(blocks),
        fix_text: Some(fix_text),
        ..Default::default()
    },
);
builder.finish(root, Some(b"ONAN"));
let bytes = builder.finished_data().to_vec();
```

There are three implementation postures:

1. **Keep core unchanged; adapt at the boundary.** This is the correct first
   spike. It isolates the dependency and measures the real encode cost.
2. **Give core typed native rows.** `Analysis` could own `Vec<Block>` instead
   of flat `Vec<u32>`, then copy those rows into the final buffer. This may be
   a type-safety improvement on its own merits, but it is not required by
   FlatBuffers and still constructs output bytes.
3. **Emit directly into the FlatBuffer builder.** Do not start here.
   FlatBuffers construction is substantially back-to-front while analysis
   naturally discovers rows forward. Letting transport construction dictate
   the analysis passes would couple the engine to its boundary and likely
   retain intermediates anyway.

Do not make Onion core's semantic types aliases for generated FlatBuffers
types. The engine should remain usable without a particular transport.

FlatBuffers' "zero-copy" advantage is primarily on the **read** side: generated
accessors inspect the received bytes without first unpacking the whole message
into an object graph. Producing those bytes still requires a builder unless the
wire buffer itself becomes the producer's internal representation.

## Tauri and wasm transport

### Tauri

Normal command return values implementing `Serialize` use Tauri's regular
Serde/JSON response path. The optimized binary response is
`tauri::ipc::Response`:

```rust
#[tauri::command]
fn analyze_book(text: String) -> Result<tauri::ipc::Response, WireError> {
    let bytes = analyze_flatbuffer(&text)?;
    Ok(tauri::ipc::Response::new(bytes))
}
```

`WireError` still needs a Tauri-compatible serialized error representation.
A bare `Result<Vec<u8>, _>` expresses the Rust data but should not be assumed
to take the optimized ArrayBuffer response path.

Native Rust consumers should not serialize merely to call other Rust. Keep the
native `Analysis`/Sous types inside Rust; construct bytes only for the webview,
cache, or another process.

### WebAssembly

The wasm boundary returns the same FlatBuffer bytes as a `Uint8Array` or
`ArrayBuffer`. Generated TypeScript accessors retain that byte buffer and jump
to requested fields:

```ts
const bytes = new Uint8Array(await invoke<ArrayBuffer>("analyze_book", { text }));
const bb = new flatbuffers.ByteBuffer(bytes);
const analysis = Analysis.getRootAsAnalysis(bb);

for (let i = 0; i < analysis.diagnosticsLength(); i++) {
  const finding = analysis.diagnostics(i)!;
  render(finding.id(), finding.from(), finding.to());
}
```

No blocks are decoded or materialized by that diagnostic loop. A caller can
still choose to create JS objects for reconciled UI findings; the binary
reader does not create them implicitly.

The byte-level output must be identical between native and wasm for the same
semantic input and schema version. That is the mirror-drift gate.

## Sous input and finding shape

The promising input direction is an ordered source list whose keys are opaque:

```text
Source { key: string, text: string }
```

Sous must not parse file paths or scripture IDs from `key`. Onion is the USFM
ingest route: it supplies the monotone content mask and the mapping between raw
file bytes, analyzed content, verse anchors, and editor coordinates. A vref
loader is another route to the same source contract.

Do not casually call ordering irrelevant. It can remain semantic for:

- deterministic output and source indices;
- book-level discourse state;
- breadth/dispersion accounting;
- a duplicate-key refusal or duplicate-preserving policy.

Before implementation, reconcile this with Sous Chef 2's current canonical-map
contract (canon order, increasing chapters, no duplicates). The FlatBuffers
schema must encode the decided source semantics; it must not decide them.

### Sites and statistical evidence are different grains

Do not force source span, rule identity, numerator, denominator, severity, and
every future variant into one u64/u128 solely because some current fields fit.
They have different lifetimes and consumers.

Sous Chef 2's pattern-grain output suggests two planes:

```text
pattern:
  rule_id
  argument/string index
  numerator
  denominator
  badges/flags
  first_site
  site_count

site:
  source_index
  byte_from
  byte_length
  utf16_from
  utf16_length
```

This stores statistical evidence once per pattern while allowing one pattern
row to expand to rare sites. Preserve raw numerator and denominator somewhere;
a rounded percentage alone cannot support truthful explanations or cheap
re-judgment under changed thresholds.

### Start plus length

`from: u32 + length` is a sensible canonical span shape, but do not make an
internal semantic length `u8` before measuring:

- UTF-8 byte length and grapheme count are different units;
- a grapheme count cannot slice source bytes directly;
- damage and hygiene runs can exceed 255 bytes;
- some rules require a second span;
- web presentation may need both byte and UTF-16 coordinates.

Keep lengths as u32 internally. If real histograms justify it, a future wire
schema may add a one-byte fast lane plus an overflow plane. That optimization
must not leak into the domain contract.

Reserve u128-scale data for facts that need it, such as content/suppression
checksums or Sous's short-word packing tier, rather than making every site a
u128 record by default.

## CST is not the public wire candidate

`onion/src/cst.rs` is already compact:

- nodes contain an opening token index, one child-arena range, close reason,
  and stamped context;
- `child_ids` mixes token IDs and high-bit-tagged node IDs;
- `Cst::in_order()` reconstructs document order independently of node close
  order.

That does not make the CST a good IPC or persistence contract. It is derived
from, and tied to, the exact source snapshot, lexer token sequence, marker
tables, structural rules, and internal node-ID tagging. It is also cheap to
rebuild (currently documented at roughly 0.6 ms for the heaviest aligned
book).

The intended boundary remains:

```text
source
  |-- native: Rust builds and consumes CST, then emits public reads
  `-- web:    wasm builds and consumes CST, then emits public reads

CST does not cross either boundary.
```

If a later cache needs CST-derived data, persist the public monotone facts Sous
needs--chapter rows, mask spans, anchors, checksums, or validated findings--not
the internal CST. Resident engine caches and the CST remain rebuildable.

## Alternatives

### Tiny custom envelope

The current u32 planes need only a small header and section directory to become
one binary payload. This is probably the smallest and fastest encoding and can
retain typed-array views exactly. Its cost is owning framing, versioning,
validation, Rust/TypeScript codecs, optional fields, strings, and future schema
evolution ourselves.

This alternative must remain in the spike. FlatBuffers should win on total
boundary simplicity, not merely because it is a named dependency.

### Protocol Buffers

Strong when broad language interoperability and long-lived service messages
dominate. Varints and ordinary decoded representations are less aligned with
dense typed-array reads and "malloc once in JS." It solves an ecosystem problem
not currently demonstrated here.

### Cap'n Proto

Strong for mmap-oriented rich object trees and capability RPC. Its 64-bit-word
and pointer-oriented representation, plus the Rust/TypeScript pairing, is less
natural for Onion's numeric planes. Cap'n Proto "packing" compresses zero bytes;
it is not field-level bitpacking.

### FlatBuffers

The most credible standard-format candidate here because it has Rust and
TypeScript code generation, direct reads over the received buffer, dense
vectors of fixed-width structs, variable strings/tables when Sous needs them,
and explicit schema evolution rules. The costs are a compiler/runtime
dependency, generated source, another output buffer construction, accessors
instead of raw arrays, verifier/version policy, and possible wasm-size growth.

## Recommendation now

Do not adopt a format from discussion alone. If the project wants generated
Rust/TypeScript bindings and does not want to own a custom codec, FlatBuffers
is the one standard format worth a bounded spike.

The spike must be a boundary adapter only. Do not refactor Onion core types,
persist the CST, or redesign Sous around generated types.

Encode four representative sections:

- blocks;
- token spans;
- diagnostics;
- fixes including `fixText`.

Compare three boundaries:

1. the current wasm plain object with multiple typed arrays;
2. a tiny custom section directory over the existing u32 payloads;
3. a FlatBuffer with vectors of structs and one string.

Measure:

- encoded byte size on small, median, and largest books;
- Rust construction time and peak/extra allocation;
- wasm binary-size increase;
- boundary transfer time in browser and Tauri;
- TypeScript full scan and random-row access;
- generated-code and build-tool burden;
- ease of adding one optional string-bearing Sous variant.

## Correctness gates for any spike

- Every decoded Onion read equals the current `Analysis` values exactly.
- `NONE` survives in every optional u32 field.
- requested-empty versus unrequested follows the chosen `presentReads` rule.
- UTF-16 offsets and LF input semantics are unchanged.
- A u64 value above `2^53` round-trips Rust -> wasm/Tauri -> TypeScript without
  precision loss.
- Empty, ASCII, non-ASCII, and repeated strings round-trip exactly.
- Native and wasm produce semantically identical buffers/decoded values.
- Untrusted or corrupted persisted bytes are verified and refused, not read
  unchecked.
- Older/newer schema compatibility is demonstrated for the exact evolution
  rules the project intends to support.

## Decision gates still open

1. Is one shared persisted/IPC/wasm byte contract actually required, or is the
   current package-versioned wasm schema sufficient?
2. Must unrequested and requested-empty reads be distinguishable?
3. Is a diagnostic ID a compact catalog code, a stable rule ID, or a stable
   finding/suppression identity?
4. Does the catalog ship with the package, once per session, or inside every
   analysis?
5. Are Sous sources whole books, chapters, or both, and what ordering/duplicate
   contract is normative?
6. Which coordinates are canonical in Sous findings: byte only, or byte plus
   UTF-16 at emission?
7. Is persisted data only validated UI findings, or is any derived Onion TOC
   data also worth caching? Resident CST/substrate state is explicitly out.
8. Do measured boundary cost and schema evolution justify FlatBuffers over the
   much smaller custom-directory alternative?

## References

- FlatBuffers Rust: <https://flatbuffers.dev/languages/rust/>
- FlatBuffers TypeScript: <https://flatbuffers.dev/languages/typescript/>
- FlatBuffers schema evolution: <https://flatbuffers.dev/evolution/>
- Tauri binary responses: <https://v2.tauri.app/develop/calling-rust/#returning-array-buffers>
- Current Onion analysis wire: `onion/src/analyze.rs`
- Current wasm projection: `onion-wasm/src/lib.rs` and
  `onion-wasm/onion-wasm.ts`
- Current CST representation: `onion/src/cst.rs`
