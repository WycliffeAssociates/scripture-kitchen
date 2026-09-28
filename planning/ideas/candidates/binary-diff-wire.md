# Binary diff wire (a reader beside the JSON, NOT SCHEDULED)

Deferred 2026-09-28 (Will): 0.1.8's changed-only JSON is fast enough. Pick this
up when a profile shows the word runs matter.

## Why the runs, not the units

en_ulb, one verse in forty edited, 0.1.8, `unchanged: false`:

| | JSON | `JSON.parse` |
|---|---|---|
| Psalms, `"words"` | 389 KB | 1.1 ms |
| Psalms, `"none"` | 18 KB | ~0 |
| 66 books, `"words"` | 6.2 MB | 17 ms |
| 66 books, `"none"` | 221 KB | 0.5 ms |

61 units in Psalms are 18 KB. Their runs are the other 370 KB: runs tile both
sides of every changed verse, markup and whitespace included, at ~65 bytes of
JSON per run. A 30-word verse with one word edited is ~120 runs.

## Shape

Keep the units as JSON (ids and sids are strings, and there are few of them).
Put the runs in one `Uint32Array` beside them, the way `Edits` and `Splices`
already cross.

```text
header   u32 format version · u32 mode (none | words | chars)
per run  u32 from · u16 len · u8 flags            (7 bytes; 8 if aligned)
flags    bit 0  changed      (the side says which: removed on baseline, added on current)
         bit 1-2 what        (markup | text | whitespace)
         bit 3  note
unit     u32 first run · u32 run count, per side, in the unit JSON
```

- No direction bit: a side's array is either baseline (unchanged | removed) or
  current (unchanged | added).
- `u16` len: a verse never comes close, but a long introduction in front matter
  can. The writer splits any run longer than `u16::MAX`, so the reader never
  has to handle the long case.
- Mode is per call, so it goes in the header, not on each run.
- The reader is generated from the schema, like `find-reader.ts` and
  `mask-reader.ts`.

## V8 constraints

- Offsets stay far below 2³⁰, so every value read out is an unboxed small
  integer (a Smi), not a HeapNumber. UTF-16 offsets into one book are a few
  million at most.
- Typed arrays never change element kind, so a hot loop over them doesn't
  deoptimize the way a mixed JS array does.

## A cheaper first step

Merge adjacent unchanged runs that aren't markup, whitespace included, so
`the`, ` ` and `fox` become one text run. Sefer's reading drops only markup
(`readingRuns`), so nothing it draws changes. Coverage is still whole and the
wire stays JSON. It may be enough on its own.
