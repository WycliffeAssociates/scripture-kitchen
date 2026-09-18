# The mask map

Which source spans a projection is made of, as a buffer — the answer to "where
in the document is this reading?" that a host can keep and use without asking
again.

```js
import { MaskMap } from "usfm-galley/mask-reader";

const source = await read("books/MRK.usfm");
const map = MaskMap.open(galley.mask("books/MRK.usfm", { utf16: true }));

let reading = "";
for (let n = 0; n < map.rangeCount; n++) {
  const r = map.range(n);
  reading += source.slice(r.sourceFrom, r.sourceTo);   // === galley.verseText(id)
}

map.toSource(hit.projectedFrom);          // the source offset an edit starts at
map.pieces(hit.projectedFrom, hit.projectedTo);   // and the spans it covers
```

```rust
galley::mask::encode(&mask, recipe, table, source_len)  -> Vec<u8>   // the buffer
galley::mask::pieces(&mask, from, to)                   -> Vec<Range<u32>>
```

Two doors write it, the same buffer either way:

```js
galley.mask(id, opts?)      // a registered book — verseText off the retained projection
galley.maskOf(text, opts?)  // text the host holds
// opts = { recipe?: "verseText" | "structure" | "text", utf16?: boolean }
```

## Three cuts, named for what survives

| recipe | word | what survives |
| --- | --- | --- |
| `verseText` | 0 | text inside verse extents only; notes, headings and front matter drop |
| `structure` | 1 | paragraph, chapter, verse, table and periph markers with designators, newlines, attribute lists |
| `text` | 2 | every text byte anywhere, notes included; nothing removed |

The mask's axis is text versus markup, and the names say which side of it a cut
keeps. `structure` is not all markup — character markers drop, which is what
makes overlaying a skeleton onto a target possible at all.

`text` is the cut a diff run is written in: nothing is removed, so no byte is
unreachable, and a run whose `what` is not `"markup"` is bytes from this cut. A
host rendering a diff page cuts its UNCHANGED units through the same recipe, so
a footnote is inline everywhere or nowhere — its choice, made once.

A reader that does not know a recipe number throws at `open` naming it, rather
than labelling the buffer as whichever cut sorts first.

## The layout

Sequential-counted, like the find buffer: one book per buffer, no directory.

| at | word | |
| --- | --- | --- |
| 0 | `magic` | `MASK`, little-endian |
| 4 | `version` | 1 |
| 8 | `flags` | bit 0: every offset is a UTF-16 unit |
| 12 | `recipe` | 0 `verseText`, 1 `structure`, 2 `text` |
| 16 | `rangeCount` | |
| 20 | `sourceLen` | the text's length, in the flags' unit |
| 24 | `projectedLen` | the projection's length, same unit |
| 28 | `rangeCount ×` | `sourceFrom: u32`, `sourceTo: u32` |

`sourceLen` lets a consumer refuse a map cut from a different revision of the
text than the one it holds, BEFORE it joins slices out of the wrong string.
`projectedLen` is its checksum after the join.

`starts` — where each range begins in the projection — is **not** on the wire.
It is the prefix sum of the range lengths, and the reader builds it in one pass
at open: four bytes a range for a value derived in microseconds.

Read it through `usfm-galley/mask-reader`, never by hand. Both ends are
generated from `galley/src/mask/schema.rs`, so they cannot disagree, and
`MaskMap.open` validates the magic, the version, and that the row block fits
the count — a consumer a version behind fails at its first call instead of
misreading a field.

## The laws

1. `ranges` ascending, disjoint, each non-empty.
2. Maximal: `ranges[i].sourceTo < ranges[i+1].sourceFrom`, strictly. Two
   adjacent survivors are ONE range, so a seam is always a real gap.
3. `starts[0] == 0` and `starts[i+1] == starts[i] + len(ranges[i])`.
4. The source slices, concatenated, are the projection.
5. `projectedLen` is that concatenation's length; 0 when there are no ranges.

Law 4 holds in both units, because every range boundary is a token boundary and
so a character boundary — which is why the same map, rebased, slices a
JavaScript string correctly.

## Nothing is inserted

The projection is a pure concatenation. `Mask::text` pushes one source slice
per range and nothing between them; there is no separator, and a dropped span
contributes nothing at all.

```text
\add one\add*two   →   onetwo
```

That is correct, not a bug: `\add*` is a delimiter, and the space between two
words is the author's to write. A host that wants a break there writes one; the
engine will not invent text it cannot point at.

Newlines are the same rule read the other way. They are KEPT source bytes, not
synthesized ones, which is why a projection has blank lines where prose was
dropped — collapsing them would be trimming, and nothing here trims.

## What it costs

Eight bytes per range plus the header, measured over one `lex` + `cst::build` +
`mask` pass per book:

| corpus | recipe | ranges | wire | kept text (UTF-8) |
| --- | --- | --- | --- | --- |
| en_ulb | verseText | ~80 k | ~645 KB | ~4.1 MB |
| en_ult, word-aligned, 67 books, 103 MB | verseText | 1,607,157 | 12.9 MB | 4.2 MB |
| en_ult, word-aligned | structure | 784,778 | 6.3 MB | |

**On word-aligned text the map is larger than the projection it describes.**
Every word is its own range and so is the space after it, because `\zaln-e\*`
and the next `\zaln-s` sit between them. For an aligned reference, hold the
string `verseText` returns and not the map; the map is the cheaper thing to
hold for the unaligned targets an editor edits, which is the case it was asked
for.

There is no run-length form. `gap u16, keep u16` would halve the aligned
figure and is not `ticket`'s fixed-width vocabulary.

## Where a projected offset goes

`toSource(projected)` is total: the projection's own length answers the end of
the last range, and anything past it clamps there.

`pieces(from, to)` is the span version — one source range per contiguous run
the interval touches, in order. It is the same computation find does for every
hit, and the two are asserted equal over the fixtures in both units
(`galley/tests/mask.rs`), so a consumer that highlights from a find buffer and
one that highlights from a map cannot disagree about where the text is.

The bytes BETWEEN two pieces are exactly the markup the projection dropped.
Whether that markup survives a replacement is the caller's decision, never the
engine's — the buffer's job is to have said the gap is there.
