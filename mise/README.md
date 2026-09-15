# mise

*Mise en place*: the prep both stations share, laid out before either starts.

`mise` is the workspace's leaf crate. It has zero dependencies and depends on
nothing in the workspace, so `usfm_onion` and `sous-core` can both use it
without either depending on the other — which is the boundary
`sous-chef/charter.md` requires and `CLAUDE.md` restates.

## The scope rule

Two things belong here, and only if MORE THAN ONE crate needs them:

1. **Standard-derived data tables.** Facts a published specification states,
   authored or generated once — `BOOK_CODES` and its order from USFM, the
   per-scalar classification bits from the UCD.
2. **Borrow-free data structures.** Types whose whole job is to be passed
   between crates or retained after their source is gone — `BookKey`,
   `Utf16Table`.

Nothing else. In particular, no I/O, no policy, no checksums, no engine logic,
no dependencies. A candidate that would make `mise` need a crate does not
belong in `mise`.

Owning the table is not owning the semantics: Onion still owns canonical book
order as a rule, Sous still owns what a book key means to a corpus, and Sous
still owns its neighbour pools and atom rule over the classification bits.
`mise` only owns the bytes they agree on.

A generated table may live here while its generator does not. `unicode/table.rs`
is written by `cargo run -p sous-core --bin gen-unicode`, which needs a hash map
and the pinned UCD extracts; both stay in `sous-core`, and the regeneration gate
(`sous-chef/core/tests/unicode_generator.rs`) compares the bytes it writes here.

## Modules

| Module | What it holds |
| --- | --- |
| `books` | `BOOK_CODES` in spec order, its membership predicates, `BookKey`, `canonical_rank` |
| `unicode` | `Class`, its bits, `class_of`/`is_glue`, and the two index paths in `lookup` |
| `utf16` | `Utf16Index` (borrows its source, both directions) and `Utf16Table` (detached, byte → UTF-16) |
