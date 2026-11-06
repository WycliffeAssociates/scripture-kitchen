# usfm_onion

A USFM engine in Rust: lex, CST, lint, format, and the USJ / USX / HTML
exports, plus a JS doorway over the same code.

| crate | directory | what it is |
|---|---|---|
| `usfm_onion` | `onion/` | the engine — no JS, no wasm, no serde in `src/` |
| `onion-wasm` | `onion-wasm/` | the `#[wasm_bindgen]` doorway and the UTF-16 walls |
| `usfm_galley` | `galley/` | the workflows layer: chunk memoization, checksums |
| `canon` | `canon/` | book identity and numbering: the USFM book table, book names in 32 gateway languages, versification schemes, mapping and sniffing; the table, names and counts also as plain TypeScript at `./canon` |
| `bento` | `bento/` | scripture containers: RC, Scripture Burrito and BTT Writer manifests, read leniently into one shape, checked against WA's Burrito fork, created and edited; its own wasm module at `./bento` |
| `bttwriter` | `bttwriter/` | BTT Writer projects: stitched to USFM, their save histories replayed into one git pack with the manifests in a closing commit; its own wasm module at `./bttwriter`, loaded on the migration screen |
| `reference` | `reference/` | a place in scripture spelled for machines: the Scripture Referencing Standard's basic reference (`MAT 1:20-2:3; MRK 1:1`), read strictly into `mise::Address` and written back; and citations as people write them (`Mateo 5:3-7; 6:1`), read leniently with canon's names |

`onion/src/bin/playground.rs` drives the engine over a corpus for profiling and
for the `debug/` eyeball dumps; `onion/src/bin/codegen.rs` regenerates the
checked-in tables, the wire, `reader.ts` and `diagnostics.json`.

## Releasing

Bump the version in every manifest (as the `0.1.x:` commits do) and push to
`master` with the release notes as the commit message. CI does the rest: it
rebuilds the wasm packages and commits them back if they changed, then, with
every gate green, tags `vX.Y.Z` on that commit and releases and publishes it.
Nobody builds wasm or pushes a tag by hand.

## Install

Every `v*` tag publishes to the org Nexus: the npm package to `wa-npm`, the
ten library crates to `wa-cargo`. Reads need no login. Not on crates.io or
public npm. Install from the registry, or from the git tag; both carry the same
code.

**npm, from Nexus**

```ini
# .npmrc
@wycliffeassociates:registry=https://nexus-registry.walink.org/repository/wa-npm/
```

```sh
pnpm add @wycliffeassociates/scripture-kitchen
```

**Cargo, from Nexus** (read through the `wa-cargo-public` group: the hosted
repo marks its index auth-required, the group does not)

```toml
# .cargo/config.toml
[registries.wa-cargo]
index = "sparse+https://nexus-registry.walink.org/repository/wa-cargo-public/"
```

```toml
# Cargo.toml
usfm_onion  = { version = "0.1", registry = "wa-cargo" }
usfm_galley = { version = "0.1", registry = "wa-cargo" }
```

**Or from the git tag**, no registry configured

```toml
usfm_onion  = { git = "https://github.com/WycliffeAssociates/scripture-kitchen", tag = "v0.1.19" }
usfm_galley = { git = "https://github.com/WycliffeAssociates/scripture-kitchen", tag = "v0.1.19" }
```

```sh
pnpm add github:WycliffeAssociates/scripture-kitchen#v0.1.19
```
