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

overlay(target, source) → 6 edits, ascending, non-overlapping
  insert "\p\n"   at the target's \v 21
  insert "\n\q1" "\n\q2" "\n\q1" "\n\q2"   after verse 23's text
  insert "\p\n"   at the target's \v 24

overlayText(target, source)
  \v 21 Kwa hiyo Bwana Mungu …
  …
  \v 23 Ndipo huyo mtu akasema: “Huyu … mwanamume.”
  \q1
  \q2
  \q1
  \q2
  \p
  \v 24 Kwa sababu hii …
```

## The address

**(verse sid, leading | inside, ordinal)** — plus the MARKER the position held
when the address was taken. The triple is what names equivalent nodes on both
sides; the marker is a check, never a key.

- **Leading** — the block sits immediately before its verse's `\v`, with no
  verse text between them. It lands exactly, because "immediately before
  `\v 24`" means the same thing in every language. A verse has AT MOST ONE
  leading block: the markers ahead of it in the same run closed before the
  `\v` and belong to the verse behind them.
- **Inside** — the verse's own text is above it. Where a verse's text SPLITS
  is unknowable across languages, so an inside block is inserted EMPTY after
  the target's verse text, in source order, and the translator pastes each
  line into place.

Ordinals count from one per address, in document order.

`targetNodeFor` and `sourceNodeFor` REQUIRE the marker and check it against
the side the address came from — `targetNodeFor` is handed a source address,
`sourceNodeFor` a target one. If that position still exists but now spells
something else, the call refuses by name (`GEN 2:23 inside 2 names q1 but the
node there is q2 — the address is stale`) rather than answering about a
different node. A position that is simply gone is not stale; it is absent, and
answers as such.

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
- **The target's skeleton becomes the source's, exactly.** A target block at
  an address the source lacks is REMOVED — only its marker token, so its text
  joins the block before it. A target block the source spells differently is
  respelled in place, keeping whatever delimiter it ended in.
- **Nothing is placed by proportion or length ratio.** Leading before `\v`,
  inside after the verse text. That is all.
- **Pairing is S1's**, through `sous_core::align` over both sides'
  `OnionBook` projections. A verse pairs when one range meets one range, which
  is bridge ↔ the same bridge. A bridge met by a constituent run, a duplicate
  sid, a partial overlap, a verse the other side does not have: no edits, and
  one `unpaired` line per side.

## The invariant

> Apply the overlay's edits to the target, re-extract its skeleton, and it
> equals the source's skeleton with the source's empty blocks folded away.

`galley/tests/overlay.rs` asserts it on every fixture, stating the comparison
itself rather than calling the library's, so the two cannot drift into
agreement.

**One shape needs its text first.** A block marker with nothing under it but
the next `\v` IS, in USFM, the block that verse sits in — no byte in the file
says otherwise. So an inside block inserted at the end of a verse, with no
further block before the next `\v`, reads back as that verse's leading block
until the translator types into it. The test pastes a word into every block
the overlay opened empty, which is exactly what the scaffold is for; every
other claim — which addresses, in what order, spelled how — is asserted
unchanged. Where the source's own blocks close the run, as in the example
above, the empty scaffold already reads back correctly and the test says so
without pasting.

## What it costs, and what it is not

An overlay reads the Pantry's retained products: one `parsed` off the warm
chunk cache per side for tokens and the lint report, the retained mask for
"is there verse text here", the retained `Toc` for the verses. A source must
therefore be registered with its text — `updateReference(id, text, true)` —
because a lengths-only reference has no skeleton to copy.

The transaction crosses as `onion-wasm`'s `Edits`, built through
`Edits::from_parts` — the same class `formatEdits` answers with, so a host
applies an overlay exactly as it applies a fix. Its spans are bytes unless the
caller asked for `utf16`.

An overlay is a SUGGESTION applied on request, never a finding. It is not on
the publication path and nothing about it reaches a snapshot.
