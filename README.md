# usfm_onion

A USFM engine in Rust: lex, CST, lint, format, and the USJ / USX / HTML
exports, plus a JS doorway over the same code.

| crate | directory | what it is |
|---|---|---|
| `usfm_onion` | `onion/` | the engine — no JS, no wasm, no serde in `src/` |
| `onion-wasm` | `onion-wasm/` | the `#[wasm_bindgen]` doorway and the UTF-16 walls |
| `usfm_galley` | `galley/` | the workflows layer: chunk memoization, checksums |

`onion/src/bin/playground.rs` drives the engine over a corpus for profiling and
for the `debug/` eyeball dumps; `onion/src/bin/codegen.rs` regenerates the
checked-in tables, the wire, `reader.ts` and `diagnostics.json`.

## Install

Nothing is published to crates.io or npm. Both consumers install from a tag.

```toml
# Cargo.toml — one repo, both crates
usfm_onion  = { git = "https://github.com/WycliffeAssociates/scripture-kitchen", tag = "v0.1.0" }
usfm_galley = { git = "https://github.com/WycliffeAssociates/scripture-kitchen", tag = "v0.1.0" }
```

```sh
npm  i   github:WycliffeAssociates/scripture-kitchen#v0.1.0
pnpm add github:WycliffeAssociates/scripture-kitchen#v0.1.0
```
