# usfm-galley

The workflows crate over both engines — `onion` (USFM) and `sous-core`
(proofreading) — and **the JS package a host installs**: every onion door plus
the resident `Galley` handle, in ONE module.

`onion-wasm` remains for a consumer that wants the engine alone. This package
is the superset: its module exports `onion-wasm`'s own shims under their own
names, because galley's cdylib links the object they sit in.

## What's in the package

| file | what it is | who writes it |
|---|---|---|
| `src/wasm.rs` | the tagged exports — the `Galley` handle and `Fingerprint` | by hand |
| `src/wasm/onion.rs` | the linker root that pulls `onion-wasm`'s shims in | by hand |
| `sous-reader.ts` | the typed door over the findings wire | `cargo run -p sous-core --bin codegen` |
| `package.json` | the subpath map over the two builds | by hand |
| `pkg-bundler/` | `--target bundler` output — **committed** | `wasm-pack build` |
| `pkg-web/` | `--target web` output (explicit `init()`) — **committed** | `wasm-pack build` |
| `tests/wasm_wall.rs` | the boundary test: the goldens, through the handle | by hand |
| `tests/sous_conformance.mjs` | the JS half, and the pinned export list | by hand |

`sous-reader.ts` is a SECOND output of the one generator; `sous-chef/reader.ts`
is the first. A package cannot export a path above its own directory, so the
reader the package ships lives here. Both are checked in and
`checked_in_reader_is_fresh` pins them to the schema.

## Build

```sh
./build.sh                                          # BOTH committed packages

cd galley && wasm-pack test --node --features wasm --test wasm_wall
cargo check -p usfm_galley --features wasm --target wasm32-unknown-unknown
```

`build.sh` is the only supported way to produce `pkg-web`/`pkg-bundler`: the
committed `.wasm` has to be BYTE-IDENTICAL on any machine, which is what lets
CI gate the binary and not merely its interface. It exports
`--remap-path-prefix` for `$CARGO_HOME` and the rustup sysroot, and
`Cargo.toml`'s wasm-opt args strip the `producers` section. Same treatment as
`onion-wasm/build.sh`, same reasons, stated there in full.

`--weak-refs` is not optional: `FormatOpts`, `Edits`, `Splices` and
`Fingerprint` hand out HANDLES, and it is the backstop for a missed `.free()`.

## Distribution

Not on npm. Consumers install from a GitHub tag, which is why the two `pkg-*`
directories are committed. This `package.json` maps the subpaths: `.` →
`pkg-bundler`, `./web` → `pkg-web`, `./web/wasm` → the binary, `./sous-reader`
→ the findings reader.

npm cannot install a subdirectory of a git repo, so the REPO-ROOT
`package.json` is the installable identity
(`@wycliffeassociates/scripture-kitchen`). It is DERIVED from this file and
`onion-wasm/package.json` together — `node package-root.mjs > package.json` —
with this package at `.` / `./web` and onion-wasm's under `./onion`. Edit a map
here and regenerate; never both by hand. CI diffs the derived file.

## The doors

`galley/src/wasm.md` is the door list, the find buffer's layout, and the
equivalence claim. In short: `update` is the only door that takes a book's
text, and the rest run off the retained copy by id.

Match formatting — a target's paragraph and poetry structure made equal to a
source's, as one edit transaction — is `overlay` and its five companions on
the handle. `galley/src/overlay.md` is the contract (the skeleton, the block
address, what crosses and what never does, the round-trip law and its one
USFM caveat); the "overlay doors" section of `wasm.md` is the wire.
