# mise

*Mise en place*: the prep both stations share, laid out before either starts.

`mise` is the workspace's leaf crate. It has zero dependencies and depends on
nothing in the workspace, so `usfm_onion` and `sous-core` can both use it
without either depending on the other — which is the boundary
`sous-chef/charter.md` requires and `CLAUDE.md` restates.

## The scope rule

Two things belong here, and only if MORE THAN ONE crate needs them:

1. **Spec-derived data tables.** Facts the USFM specification states, authored
   or generated once — `BOOK_CODES` and its order.
2. **Borrow-free data structures.** Types whose whole job is to be passed
   between crates or retained after their source is gone — `BookKey`,
   `Utf16Table`.

Nothing else. In particular, no I/O, no policy, no checksums, no engine logic,
no dependencies. A candidate that would make `mise` need a crate does not
belong in `mise`.

Owning the table is not owning the semantics: Onion still owns canonical book
order as a rule, and Sous still owns what a book key means to a corpus. `mise`
only owns the bytes they agree on.

## Modules

| Module | What it holds |
| --- | --- |
| `books` | `BOOK_CODES` in spec order, its membership predicates, `BookKey`, `canonical_rank` |
| `utf16` | `Utf16Index` (borrows its source, both directions) and `Utf16Table` (detached, byte → UTF-16) |
