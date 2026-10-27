# ticket

The order slip both ends of a wire read from: the vocabulary a format's
declaration is built from, and the emitters that write one record's Rust
writer and one record's TypeScript accessor class.

```text
mise ◄── onion ◄── onion-wasm            mise ◄── sous-core
   ▲        ▲                                        ▲
   ╰────────┴──────────── galley ◄──────────────────╯
ticket ◄── onion, galley
```

## The scope rule

Two things, and only two:

1. **The vocabulary** — `Width`, `Space`, `Field`, `Record`, and `Repeat` for
   the one shape a fixed stride cannot express: a run of sub-rows closing a
   row, whose length the row itself carries (find's hit and its source
   pieces). A declaration in any crate is built from these and nothing else.
   `Space::Offset` marks a position in text; whether and when it converts to
   another unit is each format's own rule.
2. **The emitters** — everything INSIDE a row block: the Rust writer's text,
   the TypeScript accessor class's text, the template fill, and the staleness
   reporter every codegen bin and gate shares.

The vocabulary also has the words a format describes its whole buffer in for
the docs — `Layout`, `Region`, `Shape`, `Slot`, `Sample` — and `layout_json`
renders one to `docs/layouts/<format>.json`. That is a description, not a
grammar: no writer or reader is generated from it, and each format still
declares its own envelope from its own constants.

**Not here:** any format's declaration, template, magic, version, envelope,
codegen bin, or generated file. Those live with the format, because an
envelope is what a format IS — the dish frames sections, a census frames
books, find counts hits — and declaring the three of them would be writing a
grammar to save fifty lines.

No dependencies. No I/O: every function returns a `String`.

## Why not in `mise`

`mise` is the leaf both engines share, and its README excludes generators on
purpose: spec data tables and borrow-free structures only. The rule stays
sharper if it keeps to that, so the generator vocabulary sits beside it rather
than inside it.
