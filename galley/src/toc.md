# The census

What a project CONTAINS, off the pinned tier alone — the answer to "how many
chapters and verses has each book" that does not cost one parse per book.

```js
import { Census } from "usfm-galley/toc-reader";

for (const id of project) galley.update(id, await read(id));   // once, at open
const census = Census.open(galley.tocAll());                   // once, for the sidebar

for (const book of census) {
  sidebar.add(book.code, book.chapters, book.verseCount);      // GEN 50 1533
}
```

```rust
galley::toc::encode(&pantry, &ids, utf16)   -> Vec<u8>   // the buffer
galley::toc::rows_of(&toc)                  -> (Vec<Chapter>, Vec<Verse>)   // members cross as toc.members
```

`galley/src/pantry.md` says what the Pantry retains and why the `Toc` is
pinned; this note says what a consumer may ask of it, and what it may not.

## Why it is nearly free

`Pantry::update` builds a book's `Toc` on the way past and pins it until the id
is removed — both roles, target and reference. This door reads that and encodes
it. **No chunk is resolved, no text is read, no wire is plated**, and `encode`
takes `&Pantry` rather than the `&mut` an `Entry` needs, because nothing here
can run the cache.

The alternative it replaces is `parse(id)` per book, and the measured gap is
not the parse:

| per book, native, warm cache, 83 KB (MRK) | |
| --- | --- |
| `parse` — every chunk a cache HIT | 311 µs |
| the same work without the wire plate | 45 µs |
| one census row set | the `Toc` is already there |

A warm parse is mostly the buffer it plates, which is why a cache hit does not
make one cheap, and why a census is a different door rather than a flag on
that one.

## What the rows carry, and what they refuse to

| | chapter row | verse row | member row |
| --- | --- | --- | --- |
| where | `start`, `end` — the rows TILE the book | `at` | — |
| which | `number` | `chapter`, `first`, `last` (the hull) | `from`, `to` |
| as written | `labelStart`, `labelEnd` | `labelStart`, `labelEnd` | each end's segment span |
| how many | `anchors`, `lastVerse` | `membersFrom`, `membersLen` | — |

```text
\c 12b         chapter  number 0 (malformed as a NUMBER), label "12b"
\v 1,3,5 …     verse    first 1, last 5, label "1,3,5", members [1] [3] [5]
\v 12a …       verse    first 12, last 12, label "12a", members [12a]
```

**The label and the members are spans, not tokens.** The dish's own rows also
carry a `token` and a `designator` index; those index the TOKEN STREAM, which
is the rebuildable tier and not resident, so the census does not. What the
census carries instead is where the designator's LABEL sits in the book's
text — the spelling `number` cannot carry, `\c 12b` or `\v 6a` — and what a
verse designator COVERS, as a run of member rows. The retained `Toc` keeps
both as positions when it is built, so no parse and no token is needed to
read them, and the host already holds the text they slice.

**First and last are the hull; the members are the truth.** `\v 1,3,5` spans
1–5 and covers 1, 3 and 5 — not 2 or 4. A segment is a place INSIDE its
number: `\v 12a` covers `12a`, not `12b`. `membersOf(n)` on the reader
decodes one verse's run. A malformed designator has a label and no members.

**Row 0 is always the front matter.** It carries the bytes before the first
`\c` and its `number` is 0, which is what makes the rows tile. The reader
exposes both readings: `chapterCount` is rows, `chapters` is `\c` markers.

**A verse COUNT is the host's arithmetic, not the engine's claim.** A bridge
`\v 5-7` is ONE anchor naming three verses, so "how many verses in this
chapter" has two honest answers. Both are on the row — `anchors` counts
markers, `lastVerse` is the highest number any of them names — and the engine
picks neither.

**A verse belongs to the chapter row whose span contains it.** Position, never
the designator: a book with two `\c 3`s gives each row its own anchors, and
neither swallows the other's.

## Bytes, or UTF-16

Offsets are raw bytes unless `utf16` asks otherwise, like every other door on
the handle. Under `utf16` every offset is rebased through **that book's own**
retained table — per book, because each has its own.

A reference registered without `keepText` kept no text, so it kept no table:
it answers `PantryError::NoProjection`, which crosses the wall naming the
argument that fixes it (`… retains no UTF-16 table; register it with keepText,
or ask for byte offsets`). It is never quietly answered in bytes.

## Scope, and who is listed

`tocAll`'s scope reaches further than `findAll`'s, and the difference is the
point: **a reference that kept no text still kept its `Toc`**, so it is
listed. Find excludes it because it retains nothing to search; the census
includes it because it retains exactly what a census asks for. The one thing
such a book cannot answer is `utf16`.

An id that is not registered contributes nothing and is not listed — the
directory names what was FOUND, and a caller comparing its own list against
`bookCount` sees the difference. The wasm door refuses an unknown id by name
before the encoder is reached, as `parse(id)` does.

## The wire

Declared once in `galley/src/toc/schema.rs`; the Rust writer
(`toc/generated.rs`) and the TypeScript reader (`galley/toc-reader.ts`) are
both generated from it, and `galley/tests/codegen_output_matches_input.rs`
fails the build if either is stale. The rows come from `ticket`, the
vocabulary and emitters every declaration in the workspace shares; the
ENVELOPE — header, directory, ids — is this format's own, because a census
frames books where a dish frames sections.

```text
header      magic "TOCS", version, flags, bookCount, the three strides, directoryAt
directory   per book: code, chaptersAt, chapterRows, versesAt, verseRows, idAt, idLen,
            membersAt, memberRows
rows        per book: chapter rows, verse anchors, then members, each block 4-aligned
ids         every id's UTF-8, in directory order
```

**Read it with the generated reader, not by hand.** `Census.open` validates
magic, version and all three strides and throws naming the mismatch, so a consumer
that is a version behind fails at its first call rather than misreading a
field. Nothing in a consumer should know a byte offset in this table; it is
written here because the format has to be reviewable, not because anyone is
meant to implement it twice.

The reader is LAZY by design: opening validates the envelope, and a row is
decoded only when something asks for it. A sidebar over 66 books touches 66
directory entries and no verse row at all.
