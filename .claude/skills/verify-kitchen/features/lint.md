# Lint diagnostics

## Sub-features

- `galley.lint(id)`: the parse buffer's diagnostics section for a registered book.
- Each diagnostic resolves to a `DiagnosticName` through `onion-wasm/diagnostics.json`.

## How to get to it (user POV)

Sefer's editor underlines a structural problem (an unclosed `\add`, an
unknown marker) and lists it beside the Sous findings.

## Driving it with node

No helper yet. `onion-wasm/tests/conformance.mjs` (search "diagnostics resolve
through the catalog") reads `parse(text, { diagnostics: true }).diagnostics`
and `d.code().name`; `galley/tests/sous_conformance.mjs` calls `galley.lint`.
Write the helper beside `findings.mjs` the first time a change needs it.

## Gotchas

- A reference book retains no text, so `lint` on it throws "retains no text".
