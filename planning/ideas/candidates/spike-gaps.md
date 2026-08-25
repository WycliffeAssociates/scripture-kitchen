# spike-gaps — engine-facing asks from the CodeMirror integration (2026-08-24)

Source: `onion-2-spike/GAPS.md` (the full report, with workarounds, evidence and
what worked better). This file is the asks alone, condensed, for triage here.

The spike replaced the CM probe's hand-rolled USFM logic with `galley::analyze`
end to end — block classes, chapter nav, chrome hiding, verse/chapter widgets,
note extents, diagnostics with eager fixes, a chapter clamp, a note-apparatus
satellite and a second editable surface over one canonical document. It works,
11/11 driver checks pass, and **no engine change was needed** — every ask below
has a JS workaround in the spike today.

## Ranked

1. **Widen `verse_anchors` and `chapters` to carry the marker and content
   offsets.** An editor hides `[marker_from, number_from)` and `[number_to,
   content_from)` and puts the caret at `content_from`; both reads give only the
   designator span. Worse, an ABSENT designator reports its empty span at the
   MARKER START (`\v ` → `[48,48)` where the marker is `[48,51)`), so the
   propped-open empty slot cannot be placed from the read. Proposed:
   `verse_anchors → [chapter, marker_from, number_from, number_to, content_from]`,
   `chapters → [number, marker_from, label_from, label_to, content_from, from, to]`.
   The emitter is holding the token when it writes the row. **Add a flag for
   "number-shaped" while widening**: a designator is positional, so after a
   number is deleted the next word becomes the designator (`\v  Then He
   declared` → designator "Then") and an editor that trusts the read styles
   scripture as a verse number. Lint already computes this (malformed-designator,
   aux malformedShape); the read does not carry it, so the editor re-decides from
   the bytes.

2. **Add a line-level read.** Per marked line: `[class_byte, from, content_from,
   to]`. Today the editor materialises every token to find line-initial markers,
   and that JS pass costs MORE than the wasm call (John 113KB: analyze 0.74 ms,
   projection 1.06 ms). It would also remove the editor's need for whole-book
   `TOKEN_SPANS`, leaving that read genuinely viewport-clippable. Highest-value
   single addition.

3. **Split FRONT — the pass-7 ledger item, CONFIRMED.** `\id \ide \usfm \rem
   \sts \h \toc1-3` (machine metadata) and `\ip \iot \io1 \is \imt` (introduction
   prose the reader sees) are all `PARA|FRONT`. The editor lays out the second
   group and hides the first, so the one marker-name regex the class byte was
   meant to delete is still in the probe. Proposed: a `META` flag bit, or split
   into FRONT_IDENTIFICATION / FRONT_INTRODUCTION. The categories that decide it
   are already in the table.

4. **A `note_parts` read** (gated behind the existing `NOTE_EXTENTS` bit):
   `[note_index, part_kind, from, to]` for caller / origin (`\fr`,`\xo`) / body /
   nested. The apparatus satellite needs the note's internals to render a row and
   freeze structure; `note_extents` stops at the outer span and `text_runs` has
   markup removed, so the spike still regexes `\fr`/`\ft` out of the note's bytes.

5. **Report the declared `\usfm` version in the Analysis** (or resolve
   severities engine-side). `diagnostics.json` gates codes on it — `severity:
   null` until the document declares a version — so the editor currently regexes
   the first 512 bytes to know whether a finding is showable at all.

6. **Two one-line emit fixes.** Diagnostic anchor spans are TOKEN spans and carry
   the trailing delimiter, so `{anchor}` renders `"DEMO  is not a book
   identifier"` (double space); and `unterminated-milestone`'s template prepends
   a `\` the anchor already contains, giving `"\\ts-s  milestone is missing its
   \*"`. Trim the anchor at emit and drop the template's backslash. (Same family:
   milestone token spans include the trailing space, so a pip widget replacing
   the token eats the space between words.)

7. **`locate` / `book` are on galley's method list but not exported.** The sid
   readout is a JS walk over `chapters` + `verse_anchors` in every consumer that
   wants one. Export them, or remove them from the list and say the reads are
   sufficient.

8. **Wrapper hygiene in `galley.ts`.** `view()` never frees the wasm `Analysis`
   and nothing documents who must — a per-keystroke leak if a consumer misses it.
   Also: `blockAt()` re-walks the generator per call, and every consumer will
   hand-assemble the same commit-set mask (a `WANTS.COMMIT` constant is one line).

9. **`toByte`/`toUtf16` re-encode the whole document per call** — 0.26 ms on a
   113 KB book for one number. Fine at a handful of calls per interaction (the
   documented design), not fine for the per-token conversion §10.3 imagines. No
   action now; the opaque-handle fallback stays the documented answer.

10. **No way to browse the side-table.** `playground --lint --codes` (name,
    severity ladder, template, fix label, example rendering) would make the
    `{anchor}` conventions discoverable without rendering one by hand.

## Non-asks worth knowing

- `analyze` cannot describe ONE extent (clip bounds token reads only; the
  pipeline still lexes the book). At 0.7 ms/book that is the right trade — the
  spike does not want an incremental API.
- The engine's classification beat the probe's regexes wherever they disagreed
  (`\ip`/`\iot` are FRONT, not HEADING; `\s5` is not block-level so a `\p` extent
  spans it correctly). The spike follows the engine everywhere.
- Eager fixes apply as ONE CodeMirror transaction and ONE undo step with no
  offset round-trip — the wire shape needs nothing.
