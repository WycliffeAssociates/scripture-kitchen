# Architecture

    mise ◄── onion ◄── onion-wasm            mise ◄── sous-core
       ▲        ▲                                        ▲
       ╰────────┴──────────── galley ◄──────────────────╯

- `onion` / `onion-wasm` / `galley` are the engine. `sous-core` / `sous-cli`
  (under `sous-chef/`) are the other product. `planning/` and `GLOSSARY.md`
  are onion's only; everything about tests and conventions applies to both.
- `mise/` is the zero-dependency leaf both share: spec-derived tables and
  borrow-free data structures only (`BookKey`, book order, UTF-16 offset
  types). Scope rule and exclusions: `mise/README.md`.
- `sous-core` never depends on onion. It takes a neutral borrowed view of the
  analysis. Do not add the dependency; see the charter's ownership boundaries.
- `galley` is the one adapter allowed to depend on both. `galley::sous` owns
  the Onion→Sous seam entire: `OnionBook` (the `ProjectedBook` over Onion's
  mask and TOC), coordinate rebasing, and findings publication, cold
  (`publish_onion_findings`) or resident (`Expediter`).
- `galley` has ONE caching layer, `galley::pantry`. Everything kept between
  calls is there or on a store it defines: chunk products (`pantry/chunks.rs`,
  the one place a byte ceiling is enforced), per-book products and retained
  text, and the content-addressed `derived::Store`s the Expediter's caches are
  instances of. `pantry/budget.rs` attributes every resident byte to pinned,
  hot, or rebuildable with no residual. `galley/src/pantry.md` is the one
  caching doc; `galley/src/sous/expediter.md` is publication order and
  counters.
- `sous-cli` depends on both only to call them.
- `sous-chef/donor/` (gitignored) is the v1 spike's source, kept to read and
  port from by hand. Not a crate; nothing builds it. `donor/deps.toml.txt`
  records what it compiled against.
