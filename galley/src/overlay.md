# Match formatting

Bibles that are nothing but verse markers, typeset to match a source's
paragraphing and poetry. Onion extracts the skeletons; galley pairs them,
diffs them, and hands back one transaction.

```text
source                                   target
\p                                       \v 21 Kwa hiyo Bwana Mungu …
\v 21 So the \nd Lord\nd* God …\f + …\f* \v 22 Kisha Bwana Mungu …
\v 22 And from the rib …                 \v 23 Ndipo huyo mtu akasema: “Huyu … mwanamume.”
\v 23 And the man said:                  \v 24 Kwa sababu hii …
\q1 “This is now bone of my bones
\q2 and flesh of my flesh;
\q1 she shall be called ‘woman,’
\q2 for out of man she was taken.”
\p
\v 24 For this reason …\f + … \f*

skeleton(source)
  GEN 2:21 leading 1 p       GEN 2:23 inside 1 q1     GEN 2:23 inside 2 q2
  GEN 2:23 inside 3 q1       GEN 2:23 inside 4 q2     GEN 2:24 leading 1 p

overlay(target, source) → 2 edits, ascending, non-overlapping
  insert "\p\n"   at the target's \v 21
  insert "\p\n"   at the target's \v 24
  (the four poetry lines split verse 23's text: the translator places them)

overlayText(target, source)
  \p
  \v 21 Kwa hiyo Bwana Mungu …
  …
  \v 23 Ndipo huyo mtu akasema: “Huyu … mwanamume.”
  \p
  \v 24 Kwa sababu hii …
```

## The address

**(verse sid, leading | inside, ordinal)** — plus the MARKER the position held
when the address was taken. The triple is what names equivalent nodes on both
sides; the marker is a check, never a key.

- **Leading** — the block is in the run immediately before its verse's `\v`,
  with no verse text between them: `\b \q1 \v 5` gives verse 5 two leading
  blocks. It lands exactly, because "immediately before `\v 5`" means the same
  thing in every language. A `\c`, or a paragraph-kind marker outside the set
  (a `\s1` by default), cuts the run: what waits before it stays inside the
  verse behind, so nothing is carried past a chapter number or a heading.
- **Inside** — the verse's own text is above it. Where a verse's text SPLITS
  is unknowable across languages, so the overlay NEVER opens an inside block.
  The translator places each one (a key binding, not cut and paste), and
  `targetNodeFor` answers where the next one would go. Inside blocks the
  target already has are respelled to the source's, ordinal by ordinal, and
  any past the source's count are removed.
- **The book code is compared in capitals.** `\id mat` and `\id MAT` are one
  book: the Toc keeps the code verbatim, the overlay folds it, so a lowercase
  tS/BTT export still pairs and its sids read `MAT 1:1` like any other.

Ordinals count from one per address, in document order.

`targetNodeFor` and `sourceNodeFor` REQUIRE the marker and check it against
the side the address came from — `targetNodeFor` is handed a source address,
`sourceNodeFor` a target one. If that position still exists but now spells
something else, the call refuses by name (`GEN 2:23 inside 2 names q1 but the
node there is q2 — the address is stale`) rather than answering about a
different node. A position that is simply gone is not stale; it is absent, and
answers as such.

## A row's spans

```text
\p                          block p    from..to  = "\p\n"            the marker
\v 1 In the beginning …                from..end = "\p\n\v 1 … \n"    the paragraph
\s1 The creation           (no row, and in no block's from..end)
\p                          block p    …
```

- **`from..to` is the marker**, delimiter included — what the overlay inserts,
  removes and respells. A verse's `from..to` is its `\v` in the same way;
  `textFrom..textTo` is the verse's retained text.
- **`from..end` is the block**: the paragraph node as onion's tree closes it —
  at the next paragraph-kind marker of ANY category, listed or not, a `\c`, an
  unknown marker, or the end of the book. So the next row's `from` is not the
  block's end: a `\s1`, `\d` or `\c` between them belongs to no block, and a
  caret there is in no block. A host that wants "the nearest block" takes the
  last row whose `from` is at or before the caret, and says so.
- **`empty` is about the block**, not the marker: onion's
  `Code::EmptyParagraph`, nothing in `from..end` but the marker and line
  endings.
- **Rows are in document order**, ascending and disjoint, for `verses` and
  `blocks` alike.

## The rules

- **Markers, not classes.** The default set is onion's paragraph and poetry
  rows — `\p \m \q1 \b …` — with identification, introductions, section
  titles, lists, tables and peripherals left out. A host that wants titles to
  cross lists them (`markers: ["p", "q", "s"]`). A name is resolved by onion's
  own table, so an unknown one is a refusal rather than a filter that quietly
  matches nothing, and a numbered spelling names its ROW: `"q1"` and `"q"`
  both admit every `\q` level.
- **Footnotes and cross-references never cross.** They are not blocks, so they
  are not in a skeleton; a target's notes stay exactly where the target put
  them.
- **Empty blocks do not propagate.** A block marker with nothing under it but
  line endings is what onion lints as `Code::EmptyParagraph`, and that lint —
  not a second copy of the rule in galley — is what folds `\m \p` and `\p \p`
  in the SOURCE to one block. `\b` is empty by design, abstains from the lint,
  and crosses. The fold is source-side only: a TARGET keeps every block it
  has, because the overlay has to account for each one.
- **The target's skeleton becomes the source's, less inside blocks it never
  had.** A target block at an address the source lacks is REMOVED — only its marker token, so its text
  joins the block before it. A target block the source spells differently is
  respelled in place, keeping whatever delimiter it ended in.
- **Nothing is placed by proportion or length ratio.** Leading before `\v`,
  and that is all the overlay places.
- **Pairing is S1's**, through `sous_core::align` over both sides'
  `OnionBook` projections. A verse pairs when one range meets one range, which
  is bridge ↔ the same bridge. A bridge met by a constituent run, a duplicate
  sid, a partial overlap, a verse the other side does not have: no edits, and
  one `unpaired` line per side.

## The invariant

> Apply the overlay's edits to the target, re-extract its skeleton, and it
> equals the source's skeleton with the source's empty blocks folded away and
> each verse's inside blocks cut to the number the target already had.

`galley/tests/overlay.rs` asserts it on every fixture, stating the comparison
itself rather than calling the library's, so the two cannot drift into
agreement. It holds strictly, with nothing pasted first: the overlay opens no
empty block, so every marker it writes reads back as its own address.

## What it costs, and what it is not

An overlay reads the Pantry's retained products: one `parsed` off the warm
chunk cache per side for tokens and the lint report, the retained mask for
"is there verse text here", the retained `Toc` for the verses. A source must
therefore be registered with its text — `updateReference(id, text, { keepText: true })` —
because a lengths-only reference has no skeleton to copy.

The transaction crosses as `onion-wasm`'s `Edits`, built through
`Edits::from_parts` — the same class `formatEdits` answers with, so a host
applies an overlay exactly as it applies a fix. Its spans are bytes unless the
caller asked for `utf16`. Inserts that share a position — `\b` and `\q1`
before one verse — cross as ONE edit, so an applier that splices
last-first still lands them in source order.

An overlay is a SUGGESTION applied on request, never a finding. It is not on
the publication path and nothing about it reaches a snapshot.
