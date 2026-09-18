# Choices

Per-pass decision ledger. Newest section first.

## 0.1.3 — runs that say where and what, three named cuts, dish range queries

**The markup pass takes what the `text` cut DROPS, not "every non-Text
token".** The plan listed `Newline` and the pad kind as markup, but both
survive `Filter::text()` and so already go through the word differ; claiming
them twice would double-cover the span. The split is the mask's own: the text
pass covers the kept bytes, the markup pass the gaps. Tiling then holds because
the two passes partition the tokens, which is why `settle` only debug-asserts
it.

**A `Pad` run is `Markup`, not `Whitespace`.** §2.3's own recipe — drop
`what == "markup"`, concatenate the rest, get the reading — only works if
`what != Markup` is EXACTLY the text cut. `Whitespace` therefore means spacing
inside the reading (a newline, a blank text token), never a delimiter's
surplus.

**L4 compares concatenations, not run lists.** Coalescing is per side, so one
side can carry as a single run what the other splits across a dropped token;
the unchanged TEXT bytes and the unchanged MARKUP bytes each match across
sides, and that is the law. Pairing run-for-run is not true and never was.

**L3 is stated in the flags' own terms.** Both unit flags are byte claims made
after ASCII whitespace is stripped, and the markup pass is token-grain — `\p`
reads as changed when only the space after it moved. So the law compares what
one side LOST against what the other GAINED, whitespace-stripped, rather than
asserting every text run is unchanged.

**The reader's range helpers live on `Tree`, not on a dish class.** `Dish` is
an interface, and `Tree` already holds the token rows, the arena, `extent`,
`owners` and `parents` — the four helpers are that walk, so they belong beside
it.

**The dish-query goldens are Rust-written and JS-asserted.** `Cst::extent` and
`Cst::owners` are the oracle; `onion/tests/dish_queries.rs` writes 544 entries
over one synthetic shapes fixture, three usfmtc cases and one real book, and
`conformance.mjs` reproduces every one through the reader. Neither half can
drift without the other failing. `UPDATE_GOLDENS=1` rewrites.

**A run carries no bytes.** It is a span; `source[from..to]` on its own side is
its text. Shipping both would put the same bytes on the wire twice for
everything a run touches, so the `text` field the old positionless run
carried goes with the positions' arrival — a consumer slices the source it
already holds.
