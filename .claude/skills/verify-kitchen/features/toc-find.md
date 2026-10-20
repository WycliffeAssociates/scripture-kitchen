# TOC and find

## Sub-features

- `galley.toc(id)` / `tocAll()`: chapter and verse rows, read by `galley/toc-reader.ts`.
- `galley.find(id, needle)` / `findAll`: hits in the verse-text projection, read by `galley/find-reader.ts`.
- Planned: `galley.resolve(id, address)` (planning item "resolve").

## How to get to it (user POV)

Sefer's go-to box and cards (TOC units), and "find more" on a finding.

## Driving it with node

No helper yet. `galley/tests/sous_conformance.mjs` sections "the find buffer"
and the census read both doors. `resolve` gets a helper when it lands, driven
by `testData/resolve/cases.json`.

## Gotchas

- A verse's TOC extent starts at `\v`: the `\p` or `\s` before it belongs to
  the previous verse (Sefer ask 12; `resolve`'s `opening` answers it).
