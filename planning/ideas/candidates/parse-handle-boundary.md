# The parse boundary: one tree, one crossing, no mirrors

**State:** candidate; measured 2026-08-29 against `en_ulb` (66 books, 4,506,101
source bytes). Not scheduled.

Supersedes `flatbuffers-analysis-wire.md`, which asked how to serialize
`analyze`'s output. This asks what should cross at all.

---

## 1. Goals

1. **One definition.** Every fact about USFM lives in Rust once. No constant,
   bit layout, stride or rule is transcribed into a second language.
2. **Ergonomics.** A consumer asks the question it has and gets an answer. It
   does not decode rows.
3. **Speed.** Per keystroke, on the largest books in the wild.

Two standing constraints:

- **No memory footguns.** wasm linear memory grows and never shrinks.
- **Wasm ships in the editor on every platform, Tauri included.** The editing
  path is always a same-address-space call: nothing on it is async, no function
  is colored.

And the governing principle, which decides most of what follows:

> **A consumer must be able to switch exhaustively into its own vocabulary,
> without being forced to because onion under-delivered.**

Onion's job is the typical tree plus the facts from its tables and
canonicalization. A consumer has to know what question to ask; it should never
have to know USFM to ask it, and it should never have to re-derive a fact
onion already holds.

**Onion serves no particular renderer.** A CodeMirror WYSIWYG, a Lexical
element/text-node tree, a plain-text dump and a batch linter are all equally
valid consumers with incompatible shapes. Any read designed around one of them
bakes that one's workarounds into all the others.

---

## 2. The recommendation

```js
const dish = onion.parse(text, { diagnostics: true, toc: true, utf16: true });
const { tree, tokens, diagnostics, toc } = deserialize(dish);

for (const node of tree.walk()) {
  node.marker();        // the resolved table row — name, kind, category
  node.hasChildren();
  node.span();
}
```

- **Stateless.** The document crosses once per call; products come back in one
  buffer. No handle, no `free()`, nothing to leak.
- **Three types — `Token`, `Node`, `Diagnostic` — written field by field**, no
  `unsafe`, no dependency, generated from the same declaration as the JS reader.
  Serde is not in the wire and not in onion core. (onion-wasm already carries
  it for the diff skeleton, `lib.rs:504` — a cold modal path returning JSON.)
- **One wire, both doors.** The same bytes serve the wasm call and the native
  IPC batch. One deserializer, conformance-tested against Rust.
- **Offsets are UTF-8 by default; UTF-16 is opt-in per call.**
- **The tree is the product.** `blocks`, `lines`, `noteParts`, `class_word` and
  `block_level` go away — they are projections a consumer makes for itself.

---

## 3. What today costs

### Payload

```text
                                  bytes        % of source
analyze planes (13 reads)     6,870,097            152%
tokens + cst + diagnostics    4,412,405             98%     0.64x
```

```text
  tokenSpans   3,055,908   44.5%        diagnostics    555,772    8.1%
       lines   1,252,976   18.2%             blocks    516,976    7.5%
    textRuns     641,752    9.3%    fixes/edits/lens   109,240    1.6%
verseAnchors     622,040    9.1%          noteParts     59,392    0.9%
                                          chapters     35,140    0.5%
                                       noteExtents      4,692    0.1%
```

### Time

```text
analyze(ALL)                       24.20 ms
lex + cst + lint  (irreducible)     9.65 ms
core + tokens->utf16 only          15.12 ms
------------------------------------------
planes + 10 UTF-16 conversions     14.55 ms   = 60% of analyze
```

**Three fifths of `analyze` is building and converting planes.**

### The mirror

`onion-wasm/onion-wasm.ts` is 807 lines: 312 doc, 64 blank, **431 code** — of
which **109 are literal transcriptions of Rust**: `analyze.rs:94 mod wants`,
`:122 mod stride`, `:138 NONE`, `:154 mod class`, `:174` the flags,
`:203 mod anchor`, `:217 mod note_part`, `:234` note family, `token.rs:85
NESTED_BIT` / `:91 to_bits`. Hand-kept in sync, with no test that they agree.

That is goal 1's entire failure surface.

---

## 4. Statelessness stays, and that is why one call carries several products

Crossing a JS string into wasm, measured:

```text
book (266 KB, PSA)   pass whole book, return u32     314.4 us
                     source already held wasm-side     0.1 us
five products, stateless (re-pass each time)         1608.4 us
five products, source held                             45.9 us
```

~847 MB/s — TextEncoder speed — so it scales: **~80 us for an average 68 KB
book, ~314 us for PSA.**

This is the causal chain behind today's design, and it is sound:

```text
stateless  ->  re-passing costs 80-314 us per call
           ->  several products must come back from ONE call
           ->  a section directory
```

Holding the source wasm-side breaks the chain at link one and buys ~80 us on a
typical edit — against owning two copies of the document, where **one dropped or
mis-offset splice desynchronizes them permanently and silently.** Not worth it.

**`analyze`'s shape was right.** What was wrong was thirteen lossy planes, the
unions inside them, and a hand-written reader.

---

## 5. What crosses

### The three serialized types

```rust
#[repr(C)]                          struct Token       //  8 bytes
#[repr(C)]                          struct Node        // 16 bytes
#[repr(C)]                          struct Diagnostic  // 32 bytes
```

### Serialization: no unsafe, no dependency, generated

The three types are `repr(C)` so their layout is fixed, but **nothing casts.**
The writer emits fields explicitly:

```rust
for t in tokens {
    out.extend_from_slice(&t.start.to_le_bytes());
    out.extend_from_slice(&t.len.to_le_bytes());
    out.push(t.kind_bits);
    out.push(t.marker_idx);
}
```

Measured against the pointer cast, whole corpus:

```text
unsafe cast + memcpy              0.068 ms   0.3 ns/token   1.0 us/book
safe explicit to_le_bytes         0.526 ms   2.1 ns/token   8.0 us/book
```

**7 us per book** — 9% of a string crossing that is already unavoidable (§4).

Three things bought for that:

1. **No `unsafe`.** `slice::from_raw_parts` is unsafe for four reasons, of which
   only one is real here: **padding bytes are uninitialised, and reading
   uninitialised memory as `u8` is UB.** (Validity and lifetime are trivial;
   alignment only bites going FROM `u8`.) Writing fields never touches padding,
   so the hazard does not exist rather than being guarded.
2. **No dependency.** `bytemuck`'s `#[derive(Pod)]` would discharge the same
   obligation safely at the cost of one crate — cheap (no transitive runtime
   deps, `no_std`, compiles to nothing) but unnecessary. onion has two
   dependencies and should keep it that way.
3. **Explicit endianness.** A cast produces NATIVE-endian bytes. Every target
   here is little-endian, so it works — but that is an unstated assumption baked
   into a wire format. `to_le_bytes()` makes it a guarantee.

A guard worth knowing about if the cast is ever revisited: `size_of::<T>() == 8`
does NOT prove there is no padding, only that the size is 8. The real check
writes the sum of the field sizes — `assert!(size_of::<Token>() == 4 + 2 + 1 + 1)`
— so that size equals sum if and only if nothing is padded.

**Both sides should be generated.** The Rust writer is a field list; so is the
JS reader. `codegen.rs` already emits two artifacts from Rust declarations under
a staleness test. Emitting both ends of the wire from ONE declaration means they
cannot disagree and neither is hand-written — which is goal 1 applied to the
serializer itself.

### What each type needs

- **`Token`** is `{u32, u16, u8, u8}` = 8 bytes as it stands. Nothing to change.
- **`Node`** carries `children: Range<u32>`; `Range` is `repr(Rust)` with no
  layout guarantee, so flatten it to `child_from`/`child_to`. With explicit
  writes there is no padding question, so no `_pad` field is needed — the
  written row is 14 bytes of fields, and whether the READER strides by 16 for
  alignment is a reader decision.
- **`Diagnostic`** is a wire type distinct from `Observation`, which could not be
  cast in any case (`Observation.code` is an ENUM, and enums have invalid bit
  patterns). The wire row is wider anyway — it expands `anchor` into `from`/`to`
  — so it is what `diagnostics()` already effectively builds: seven `u32`s,
  written as eight so indexing is a shift rather than a multiply, with a spare
  word for a future `u64` code.

Enums are written as `u8`/`u32` and converted on read, which `Node.reason` and
`Node.ctx` already do.

### The dependency graph pins the unit

```text
lex(source: &str)                                        -> Vec<Token>
cst::build(tokens: &[Token])                             -> Cst
toc(source: &[u8], tokens: &[Token])                     -> Toc
lint(source: &[u8], tokens: &[Token], cst: &Cst)         -> LintReport
mask(source: &[u8], tokens: &[Token], cst: &Cst, filter) -> Mask
```

**The unit is the triple, not the tree.** A CST is indices into tokens; tokens
are offsets into source. A tree handed over alone is inert — which is why the
products travel together in one buffer and never as loose arguments.

`toc` needs no CST: chapters and verses come off source and tokens alone.

### The bit budget

```text
largest book   30,896 tokens (16 bits)   6,781 nodes   37,676 childIds (17 bits)
reason         2 bits  (4 variants, all used)
ctx            5 bits  (18 declared, 5 used)
```

A node carries 57 bits of information in 128 bits of storage. It could pack to
three u32 (-25% corpus-wide) — **take the cast instead.** At 8 KB versus 6 KB
per book the packing is not worth losing the memcpy.

`childTo` is not droppable: child ranges are **neither contiguous nor monotonic
in node order** — nodes close in a different order than they open. Measured.

### Offsets: UTF-8 by default, UTF-16 opt-in

```js
onion.parse(text, { utf16: true })
```

Every offset in every returned product is then a UTF-16 code-unit offset;
without it they are UTF-8 byte offsets, which is what Rust natively holds and
what a native consumer, sous, and a plain-text pipeline all want.

**Not hardcoded.** The conversion happens once, during the copy into the plated
buffer, over sorted offsets — it is not a per-read wall. A Rust caller wanting
the same signature gets the same flag; nothing about UTF-16 is baked into a
type.

Node ids and child ids are **indices**, not offsets, and never convert.

---

## 5b. Layering: where each piece lives

```text
onion  (pure Rust, no deps, no wasm)
  lex / cst::build / lint / toc / mask       the engine
  parse(text, opts) -> Parsed                 native callers stop here: real types
  wire::plate(&Parsed, opts) -> Vec<u8>       the writer + header

onion-wasm  (bindgen)
  #[wasm_bindgen] parse(...) -> Vec<u8>       ONE export: plate(&onion::parse(..))
  #[wasm_bindgen] mask(...)  -> {text, map}   separate; not a dish section

onion-wasm/reader.ts
  deserialize(dish) -> { tree, tokens, diagnostics, toc }
```

**The plating lives in onion, not onion-wasm.** The cold IPC door needs
byte-identical output and never passes through bindgen, so a writer behind the
wasm crate would fork the wire. One writer, both doors.

**A native Rust caller never serializes.** `parse` hands back `Parsed` with real
types and borrowed strings; `plate` exists only to cross something.

`Vec<u8>` marshals to `Uint8Array` unaided, so the whole read boundary is **one
wasm-bindgen export** — against today's ~15 exports plus four handle structs.
bindgen's remaining job is: string in, bytes out.

### Generated, from templates — not string concatenation

The repo already has the pattern: `onion/src/tables/generated.rs.tmpl` is a real
file with `@@SLOT@@` placeholders, `include_str!`'d by `tables::emit`, carrying
an `@generated ... DO NOT EDIT` header and a pointer at its staleness test.

Its useful property is that **the template holds the hand-written code too.** So
there is no generated-file/hand-written-file split: `walk()`, `nodeAt()`,
`children()` and `message()` live in `reader.ts.tmpl` as plain readable
TypeScript, and only the mechanical parts are substituted:

```text
@@ENUMS@@          MarkerKind's 14, Category's 30, TokenKind's 12
@@OFFSETS@@        field offsets and strides for the three types
@@MARKER_TABLE@@   153 rows
@@LINT_CATALOG@@   codes, templates, severity ladders
```

The Rust writer is emitted the same way, from the same field declarations, so
the two ends of the wire cannot disagree and neither is typed by hand.

## 6. Vocabulary: expose the row, never a union

`class_word` has **three callers, all plane emitters** (`analyze.rs:660`, `:683`,
`:906`). `block_level` has **one** (`:678`). Neither is used anywhere else in
onion, onion-wasm or galley — the engine never asks either question for itself.

What `class_word` does to its inputs:

```text
MarkerKind  14 variants  ->  8 coarse classes
Category    30 variants  ->  4 flags (HEADING, FRONT, POETRY, META)
```

The lossiness is the smaller sin. The larger one is that it **unions two
unrelated things**:

```text
a MARKER      has a TABLE ROW    — name, kind, category, closing, shape, numbering
a NON-MARKER  has a TOKEN KIND   — Text, Pad, Newline, OptBreak, ...
```

Unioning them is not onion's call. The table already names things by their
group — `ParaBody` distinct from `ParaPoetry` — and a `POETRY` flag destroys
exactly that distinction. **Expose the row and the token kind.** A consumer that
wants a coarse class switches exhaustively into its own; it is never forced to
because ours was lossy.

So: no `class_word`, and no replacement coarse class.

`block_level` is a rendering opinion that lived in the engine because `blocks()`
had to be a plane and something had to pick its rows. It may survive as a
documented convenience ON the row, but the tree walk is the primitive.

**Markers resolve, they do not index.** `node.marker()` returns the row with
name, kind and category already resolved — never a `marker_idx` for the caller
to look up in a table it had to be shipped.

---

## 7. The API

Every method below is a read over the deserialized buffer. Nothing allocates
wasm-side; nothing needs freeing.

### Parse

```ts
parse(text: string, opts: ParseOptions = {}): Uint8Array
deserialize(dish: Uint8Array): { tree, tokens, diagnostics, toc }

interface ParseOptions {
  diagnostics?: boolean;   // run the lint walk — the one expensive optional
  toc?: boolean;           // chapters + verses; cheap, needs no tree
  utf16?: boolean;         // every offset in every product becomes UTF-16
}
```

Rust gets the same shape, since a struct with `Default` is idiomatic there:

```rust
onion::parse(text, ParseOptions { diagnostics: true, ..Default::default() })
```

**The object does not cross the wall.** The wasm export underneath is positional
`(text, bool, bool, bool)` and nobody calls it directly — the generated
`reader.ts` wraps it, exactly as `wants()` and `analysis()` wrap the raw exports
today. That wrapper is where the type safety lives, and `onion-wasm.ts` already
documents why:

> "A misspelled key in an object crossing the wall would be silently ignored and
> the read would come back empty — here it is a compile error, because an object
> literal cannot carry a property `Wants` does not declare."

An object passed through bindgen means `Reflect::get` per key with no checking,
so `{ diagnostcs: true }` silently reads as `false`. As a TypeScript object
literal it is a compile error. The wrapper is not a compromise; it is the point.

### Tree

```ts
tree.root()          // the document node; everything hangs off it
tree.walk()          // document order from the root
tree.walk(node)      // the same over ONE subtree — cst::in_order_of already does this
tree.nodeAt(pos)     // innermost node containing pos — the caret question
tree.nodeCount()     // bounds a loop without touching the arena
```

`walk()` yields a uniform ITEM with a discriminator, never a raw tagged id. It
mixes structure and text because a document does: a `\p` node's children in
order are its own `\p ` marker TOKEN, then text TOKENS, then a `\f` note NODE,
then more text.

**The discriminant is inherent and the CST should not change to remove it.** A
class tree is `{marker: 'p', content: [...]}` and ours is a flat arena, but the
flatness is not what forces two kinds — a document tree has two kinds because
text and markup are different things. The DOM has `Element` versus `Text` with
`nodeType`; Lexical has `ElementNode` versus `TextNode`. Erasing it by promoting
every text token to a node costs badly: 254,659 tokens against 33,777 nodes
corpus-wide, and `Node` is 16 bytes to `Token`'s 8 — roughly 4 MB where there is
now 0.5 MB. The flat arena's only contribution is that the discriminant is a TAG
BIT rather than a TYPE, and hiding that is the reader's job.

**Composition with the TOC goes through a position, not a node.** `\c` and `\v`
do NOT open nodes (`cst.rs:841` — chapter and verse markers displace open scopes
but open none), so the TOC is an index over tokens, not a subtree. The composed
form is `tree.walk(tree.nodeAt(chapter.from))`. There is no `chapter.node()` and
should not be.

### Node

```ts
node.span()          // [from, to) — opening marker through last descendant
node.marker()        // the resolved table row; null on the root
node.children()      // child items in document order, structure and text mixed
node.childCount()    // iterate without materialising the child array
node.child(i)        // one RESOLVED child — walking without building the array
node.hasChildren()   // "is this a container?" — the div/element question
node.closeReason()   // Explicit | Implicit | Recovery | Eof — is this well formed?
node.context()       // the stamped SpecContext — where in the document grammar
```

`hasChildren()` plus `marker().kind()` is the whole "is this a paragraph, does it
contain things" question. What a consumer does with the answer — a div, a line
decoration, a Lexical `ElementNode` — is the consumer's business.

**No `contentFrom()`.** Worth recording WHY it ever existed: the block plane is
`[class, from, contentFrom, to]` — an extent with no route to its tokens. A
consumer holding one knows where `\p` starts but cannot reach the marker token
to find where the chrome ends, because the block plane and the token plane are
unrelated. So the engine pre-computed the answer into the row.

That is the same disease as the 330 ms plane join and the overlapping-extent
repair in §10: **several plane fields exist only to compensate for planes not
being linked to each other.** Give a block its children and the compensation is
unnecessary — the one-delimiter rule makes it `children[0].span().to`, or the
designator's end when `children[1]` is a `Designator`.

### Token

```ts
token.span()         // [from, to)
token.kind()         // TokenKind — Text, Pad, Newline, OptBreak, Marker, Designator, ...
token.marker()       // the resolved row when it IS a marker; null otherwise
token.spelled()      // the per-shape spelling bit: `\+` nesting, a milestone's `-e` half
```

`kind()` and `marker()` are the two halves §6 refuses to union.

### Marker

```ts
marker.name()        // 'p', 'q1' — the table's canonical name; null on row 0
marker.kind()        // MarkerKind, all 14 — no coarsening
marker.category()    // Category, all 30 — ParaBody distinct from ParaPoetry
marker.closing()     // ClosingBehavior — does it require a closer?
marker.numbering()   // Numbering — does it take a level, and up to what?
marker.shape()       // SpellingShape — which spellings the row admits
marker.isUnknown()   // row 0 — an unresolved name, a `\z` extension, an illegal spelling
```

No `accepts(name)` convenience: "is this spelling legal" is a table lookup the
consumer can make from `shape()` and `numbering()`, and lint already judges
spelling. A second spelling judge is a second place to be wrong.

**Row 0 is asymmetric and the API must say so.** `\zaln-s` resolves to the empty
row, so the table has no name for it — `name()` is null, `isUnknown()` is true,
and the actual spelling is only in the document, reachable through the token's
span. Extension markers are a separate scope of work.

### Diagnostic

```ts
d.code()             // the durable kebab identity, resolved from the catalog
d.span()             // [from, to) — the anchor
d.second()           // the other party (opener, owner, first occurrence), or null
d.aux()              // the code's aux integer; its MEANING is on the catalog row
d.fix()              // the offered repair, or null
d.severity(version)  // the ladder, resolved against the declared \usfm version
d.message(slice)     // the template rendered from the document's own bytes
d.locate(toc)        // the verse to go look at — needs a toc; the batch list wants it
```

`locate()` is why a whole-project findings list wants the toc alongside: a list
row reading "GEN 3:15" needs the chapter/verse index, not just an offset.

### TOC

```ts
toc.chapters()       // rows tiling the document; row 0 is front matter
toc.verses()         // one per \v, wherever it sits — `\q1 \v 5` puts one mid-line
toc.at(pos)          // the chapter:verse address containing pos
```

Named `toc`, not `chapters` — it carries both, and `at()` is the reverse index.
No further sugar: a consumer that wants a label slices the document.

It is an index over the TOKEN stream, not over the tree — `toc` needs no CST,
and `\c`/`\v` open no nodes. See the composition note under Tree.

### Mask — a separate call, and it need not be a buffer at all

```ts
onion.mask(text, opts) -> { text, ranges, starts }
```

**Not an option on `parse`,** and not a section in the dish. A mask is a
different question with its own parameter, and storing one would make every
caller pay for a product most never want.

It is also not hot and not large, so it is plain wasm-bindgen taking the string
— no dish, no deserializer, no reader. Nothing about it needs the buffer
machinery.

`ranges` are the kept source spans, sorted, disjoint and maximal; `starts[i]` is
the prefix sum, so `ranges[i]`'s bytes sit at `starts[i]..` in the masked text.
That pair IS the map. It already exists as `Mask` (`mask.rs:237`); what does not
exist yet is an export that returns it — galley today has `verseText` and
`structureText` returning a bare `String` with **no map at all**.

Two real uses:

1. **Inside galley**, feeding sous — an implementation detail of that
   coordination, not something a caller asks for.
2. **"Show me this document as plain text"** — the markup-free read.

---

## 8. Two doors, one wire

```text
COLD  project open   IPC, native, async, batch    every book, once
HOT   editing        wasm, sync                   one book, per edit
```

### The cold door is IPC because the filesystem and threads are native

Wasm cannot walk a project directory, and wasm threads need `SharedArrayBuffer`
with COOP/COEP headers. It is async because it is file I/O — fine, because it is
a load, not an edit. Coloring only hurts when it reaches the editing path.

```text
rayon par_iter (fs + lex + cst + lint), 10 threads     28.80 ms
same, single threaded                                 158.38 ms   5.5x
```

A whole Bible's diagnostics known in under 30 ms.

### It ships the same bytes

19,849 findings at 32 bytes is **635 KB** — one `tauri::ipc::Response`, the same
`Diagnostic` cast, the same deserializer the wasm door uses. A project-level
header lists books by key with their section offsets; below that it is the
format already described.

**No JSON.** JSON here would be ~2 MB, would drag serde into core, would need a
second representation to maintain, and would materialize a JS object per finding
on the one path that is genuinely large. The binary wire is smaller, needs no new
machinery, and is already conformance-tested by the hot path's own tests.

A per-book severity tally, if a panel wants one before opening anything, is a
derived summary over those same bytes — not a different format.

### The hot door is wasm everywhere

Tauri runs the same module in its webview. A boundary that is *sometimes* IPC
forces every read `async` for one platform, and that propagates through the
consumer's whole call graph.

---

## 9. Memory

```text
size_of::<Token>() = 8       size_of::<Node>() = 16

resident parse, worst book (PSA)   0.89 MB
resident parse, average book        190 KB
all 66 books at once              12.25 MB
```

The stateless design has **no handle in the read path.** Buffers cross as
`Uint8Array` values and are GC'd normally. The footgun is removed rather than
managed.

The one long-lived object is `Galley`: a bounded LRU with a byte budget and a
`residentBytes()` readout, created once per app. A cache with a declared ceiling
is not a handle to forget.

Why that matters, measured — 200 unfreed 190 KB handles, then a forced GC:

```text
                                    live   dropped   linear memory
200 x { new; free() }                  0       200      1.1 -> 1.3 MB
200 x new, no free                   200       200      1.3 -> 38.2 MB
after gc() + two event-loop turns      0       400            38.2 MB
```

wasm-bindgen 0.2.120 registers a `FinalizationRegistry` by default (the
`--weak-refs` note in `onion-wasm`'s module doc is **stale**), and it fires —
every handle was reclaimed. **And the memory never came back.** Finalization
returns the allocation to the Rust allocator, where it is reusable; the
committed pages are permanent. It protects against unbounded growth, not against
the peak, and the peak is what cannot be undone.

### The one shape to avoid

Any function taking the buffer back must be called **once per product**, never
per element. wasm-bindgen copies the entire slice into linear memory on every
call:

```text
walking 6,781 nodes (PSA)
reading from a buffer the caller owns             0.80 us      1.6 ns/read
a wasm fn taking &[u8] per read                6452.68 us    951.6 ns/read
```

Quadratic in buffer size, and dangerous because a pure stateless function is
exactly what this crate's doc asks for. `maskToText(dish, filter)` is the right
grain; `readNode(dish, i)` is not.

---

## 10. What the field shows

From a real consumer, 2026-08-29. Recorded as evidence of what this boundary
costs — **not as a specification.** Its vocabulary and its line-shaped rendering
model are artifacts of one renderer and must not propagate into onion.

**It already stopped maintaining USFM knowledge**, which is the goal working: a
file that "used to hold three regexes (PARA/HEADING/FRONT), a marker matcher, a
note matcher and a block grouper" now derives all of it from the engine, and
resolves a classification disagreement with "the engine's table is the
authority." Its class registry is deliberately never per-marker.

**Three costs it is still paying, all traceable to flattening:**

1. **Flattened extents lose containment.** A heading and a paragraph sharing a
   line come back as overlapping extents, and the consumer invented a
   sort-and-clamp repair to separate them. A flat plane cannot distinguish
   overlap-because-nested from overlap-because-wrong, so it guesses. **The
   strongest argument for shipping the tree.**
2. **Marker identity is recovered by slicing the document** — `doc.slice(from +
   1, contentFrom).trim()` to get `'p'`, because the row carries a packed class
   and no route to the marker's row. Re-parsing USFM in the consumer is exactly
   what §1 forbids.
3. **Relating two planes is the caller's problem**, and doing it naively was
   measured at 330 ms on Psalms against 11 ms for the engine itself.

All three dissolve when the tree crosses instead of its projections.

---

## 11. Rejected, with reasons

**Accessors on a live handle.** Fast enough (~5 ns a read), but requires a
persistent handle, and §9 shows the peak cannot be undone.

**Separate calls passing buffers back.** `lint(text, staleTokens, cst)` is
silent corruption — nothing checks the triple is coherent. It moves the
two-copies divergence out of a handle and into the call signature, where it is
worse: a handle has one owner; this hands every call site three loose pieces.
`text` also crosses twice (~160 us against ~86 us).

**Serde / JSON on either door.** A second representation, a core dependency, ~3x
the bytes, and a materialized object per finding on the largest path — to
replace a cast that already produces the bytes.

**FlatBuffers / flatc.** No bitfield concept, so it subsumes strides and little
else; and it generates FROM a `.fbs` and cannot read Rust, so the schema becomes
a second source of truth the engine must conform to. It also cannot generate the
153-row marker table. It wins only if consumers outside this repo, in languages
we do not ship, need to read the tree.

**A single buffer for speed at the wasm wall.** 13 typed arrays versus one
buffer is **9 microseconds** on the largest book — 0.2% of a wasm `analyze`. The
buffer is right for coherence and layout, not throughput.

---

## 12. Correctness gates

- Every value equals the corresponding `analyze` read exactly over the whole
  corpus (`tests/analyze_corpus.rs` already re-derives every read natively).
- The JS deserializer's output equals the Rust reader's, over the corpus. This
  is the conformance test that replaces the mirror.
- `repr(C)` structs are padding-free and round-trip through the cast unchanged.
  **Verify before relying on it** — `Node`'s `Range<u32>` must be flattened.
- `NONE` survives wherever an optional u32 does today.
- `utf16: true` and `utf16: false` agree after conversion, for every offset in
  every product, over the corpus.
- Section offsets are 4-aligned; a typed-array view over one never throws.
- `maskToText`'s `ranges`/`starts` map every masked offset back to the same
  source offset the Rust `Mask` gives.
- The generated reader has a staleness test, like `generated.rs` and
  `diagnostics.json`.
- LF-normalization contract unchanged.

---

## 13. Open questions

1. **Does `Diagnostic` want a `u64` code?** Seven `u32`s already cast cleanly, so
   an eighth word is optional. Whether one is wanted for a stable finding
   identity or a suppression checksum is a lint question, not a wire one — but
   the wire should not foreclose it.
2. **Does a project-wide findings list need `locate()` resolved eagerly?** If a
   panel lists "GEN 3:15" before any book opens, the cold load must carry the
   toc as well as the findings.
3. **Does the `diagnostics` boolean earn its keep?** ~70 us saved against an
   ~80 us crossing. Test with `galley/benches/wasm/wants-decomp.mjs`.
4. **Is `\s1`/`\p` extent overlap a `cst.extent()` bug?** Probably a consumer
   rendering problem rather than an engine one, but cheap to settle once the
   tree exposes containment.

**Settled, recorded so they are not reopened:** chapter checksums live in
galley's wrapper, never in onion — there is no hashing in the pure crate. Masks
are a separate wasm-bindgen call, not a parse option and not a dish section.
UTF-16 is opt-in per call, never a default and never baked into a type. Galley's
cache key is out of scope: this is a rewrite of `onion-wasm`'s surface plus a
trim of `analyze.rs`, and it does not touch how galley keys anything.

## 14. References

- Curated reads today: `onion/src/analyze.rs`
- JS schema and decoders today: `onion-wasm/onion-wasm.ts`
- wasm projection today: `onion-wasm/src/lib.rs` (module doc's `--weak-refs`
  note is stale)
- Stateful wrapper: `galley/src/wasm.rs`
- Mask and its map: `onion/src/mask.rs:237`
- Marker table, 153 rows: `onion/src/tables/generated.rs` — `name`, `kind`,
  `category`, `closing`, `shape`, `numbering`
- Codegen precedent + staleness test: `onion/src/bin/codegen.rs`,
  `onion/tests/codegen_output_matches_input.rs`
- The serialization question this replaces: `flatbuffers-analysis-wire.md`



## DECISIIONS MADE THAT WERE UNFORESEEN AHEAD OF TIME.

# BIG/MAJOR. 

## NIT / STYLE
