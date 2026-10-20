# TOC and find

## Sub-features

- `galley.toc(id)` / `tocAll()`: chapter and verse rows, read by `galley/toc-reader.ts`.
- `galley.find(id, needle)` / `findAll`: hits in the verse-text projection, read by `galley/find-reader.ts`.
- `galley.resolve(id, address, { utf16? })`: an address to found, missing or
  ambiguous, as JSON. Rules: `onion/src/resolve.md`.

## How to get to it (user POV)

Sefer's go-to box and cards (TOC units), and "find more" on a finding.

## Driving it with node

No helper for toc and find. `galley/tests/sous_conformance.mjs` sections "the
find buffer" and the census read both doors.

`resolve` runs every case of `testData/resolve/cases.json` in the conformance
section "resolve", in bytes and in UTF-16. To ask one question by hand, after
the pkg-node build:

```js
const { Galley } = (await import("./galley/pkg-node/usfm_galley.js")).default;
const g = new Galley();
g.update("books/LUK.usfm", "\\id LUK\n\\c 1\n\\s1 Ἀρχή\n\\p\n\\v 1-2 a b\n\\v 3 c\n");
const p = (chapter, verse) => ({ chapter, verse });
JSON.parse(g.resolve("books/LUK.usfm", { kind: "verses", from: p(1, 2), to: p(1, 2) }, { utf16: true }));
// { found: { from: 25, to: 36, opening: 13, covers: 1:1-1:2, coarser: false } }
```

A book needs an `\id` line to register; a reference needs `keepText`.

## Gotchas

- A verse's TOC extent starts at `\v`: the `\p` or `\s` before it belongs to
  the previous verse (Sefer ask 12; `resolve`'s `opening` answers it).
- `resolve`'s answer is a JSON string: `JSON.parse` it.
