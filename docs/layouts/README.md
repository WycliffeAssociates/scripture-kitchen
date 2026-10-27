# Wire layouts

One JSON file per hand-packed buffer in the workspace, and a real buffer of
each beside it, for a docs page to draw as byte strips. All of it is
GENERATED and checked by a staleness test; edit the declaration, not this
folder.

| file | magic | declared in | reader | generated | regenerate with |
| --- | --- | --- | --- | --- | --- |
| `dish.json` | `ONWR` | `onion/src/wire/schema.rs` | `onion-wasm/reader.ts` | true | `cargo run -p usfm_onion --bin codegen` |
| `census.json` | `TOCS` | `galley/src/toc/schema.rs` | `galley/toc-reader.ts` | true | `cargo run -p usfm_galley --bin codegen` |
| `mask.json` | `MASK` | `galley/src/mask/schema.rs` | `galley/mask-reader.ts` | true | same |
| `find.json` | `FIND` | `galley/src/find/wire/schema.rs` | `galley/find-reader.ts` | true | same |
| `corpus.json` | `ONCP` | `galley/src/corpus.rs` | `onion-wasm/reader.ts` | false | same |
| `sous.json` | `SOUS` | `sous-chef/core/src/corpus/layout.rs` | `galley/sous-reader.ts` | false | same |

`generated: false` means one end is packed or decoded by hand; the layout
still reads every offset from the format's own constants.

Every sample is the engine's output over `samples/sample.usfm`, with the
options its file's `sample.options` names.

## The shape

```json
{
  "format": "mask", "magic": "MASK", "magic_hex": "0x4b53414d", "version": 1,
  "declared_in": "galley/src/mask/schema.rs", "reader": "galley/mask-reader.ts",
  "generated": true,
  "doc": "One book's projection as the source spans it keeps.",
  "envelope": [
    {"region": "header", "at": "0", "kind": "fixed", "bytes": 28, "doc": "…", "fields": [
      {"name": "magic", "offset": 0, "width": 4, "doc": "`MASK`, little-endian."}
    ]},
    {"region": "ranges", "at": "28", "kind": "rows", "of": "Range", "count": "header.rangeCount", "doc": "…"}
  ],
  "records": [
    {"name": "Range", "stride": 8, "doc": "…", "fields": [
      {"name": "sourceFrom", "offset": 0, "width": 4, "space": "offset", "doc": "First kept unit."}
    ]}
  ],
  "sample": {"usfm": "samples/sample.usfm", "bin": "samples/mask.bin", "options": {"recipe": "verseText", "utf16": false}}
}
```

Every integer is little-endian; `width` and `bytes` are in bytes.

An envelope region is one stretch of the buffer, in wire order. `at` and
`count` are expressions a reader evaluates against what it has already read:
a number, `header.<field>`, `directory[<i>].<field>`, or
`directory[book].<field>` for a region repeated once per directory entry.

| `kind` | extra keys | means |
| --- | --- | --- |
| `fixed` | `bytes`, `fields` | a run whose fields tile it exactly — a header |
| `table` | `entry_bytes`, `count`, `fields` | `count` entries, each tiled by `fields` |
| `rows` | `of`, `count` | `count` rows of the record named `of` |
| `words` | `width`, `count` | `count` bare integers |
| `blob` | `bytes`, optional `holds` | opaque bytes; `holds` names the format inside |

A record is a row's declaration: `stride` bytes per row head, fields packed in
order with no padding. `space` is `offset` for a position in text — which unit
it is in is the header's flags — and `plain` for an index, count, code or flag.
A record with a `tail` is followed by `count` (a field of the row) rows of the
record named `of`.

`doc` on every region, field and record is the declaration's own doc string.
