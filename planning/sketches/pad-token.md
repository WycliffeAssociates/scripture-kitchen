# Pad: the one-delimiter rule moves into the lexer

*2026-08-27 · from RFC-Lexer-change-8-27.md (onion-2-spike) + Will's rulings.
Finishes what pass 18 did at the emit layer.*

## The rule

A token that folds a trailing horizontal-whitespace run keeps AT MOST ONE
code unit of it. The remainder is a new token kind:

    Pad — the reducible surplus of a structural delimiter run.
          Visible and editable bytes; never content, never chrome.

Newlines never fold (unchanged). The Token vec still partitions the source
byte-for-byte; the `token_spans` wire read goes back to tiling because it no
longer needs pass 18's clip — extents ARE the ≤1 spans now.

```text
\v       1     Text
today:    Marker("\v·······")        Designator("1·····")      Text("Text")
after:    Marker("\v·") Pad("······") Designator("1·") Pad("····") Text("Text")
```

## Rulings banked (Will, 2026-08-27)

- **G1 role change accepted**: verse/chapter anchor lead becomes genuinely
  ≤ label+1; surplus between `\v` and the designator is visible. The only
  role change in the RFC's gap × material table.
- **Pad is NOT text-view content.** Mask text / vref / USJ / USX / HTML drop
  it BY KIND — but the mask's filtered↔source offset mapping must account
  for Pad ranges exactly like other removed spans (law pinned in the mask
  oracle). Proofread addresses all real content by construction: Pad can
  only exist at delimiter positions, never inside a text run; a doubled
  space between words stays Text.
- **AttrList node-initial clips too** (closes pass 18's open ruling): the
  absorbed post-pipe run keeps one byte, surplus is Pad. The trailing form's
  pre-closer whitespace stays inside the list — genuine list bytes.
- **`\f` recovery newline** (CST addendum from the spike): an unclosed
  note's reported extent must not include the recovery `\n` (it is
  structure; a hidden newline breaks H1). Fix at the `note_extents`
  emission — clip a trailing Newline token — NOT at the CST pop, so USJ
  pins and CST shape stay put.

## Where it lands in the scanner

Consumption and mode machinery unchanged; only EMISSION splits. Pad pushes
never touch `pending_payload` / `after_marker` / `attr_frames`, which is
what keeps the designator carve and front-position pipes working across
surplus.

1. `whitespace_arm` — extend the last token by ONE byte, push the remainder
   as Pad. Covers the general marker path, milestones, and node-initial
   AttrList (`absorbs_trailing_ws`) in one place.
2. payload carve (text_arm head) — Designator/NoteCaller/BookCode keep
   label+1, remainder Pad.
3. `fused_plain` — marker keeps `name_end+1`, remainder Pad.
4. hot `\v` arm — designator half, same split (its marker half is exactly
   one space by shape-check).

Fast paths stay token-identical to the general path
(`fast_path_identity`, `fused_identity` re-pin).

## Consumers

- **token.rs** — `Pad` = kind bits 11; crosses the wire as-is.
- **toc** — `designator_row` skips Pad (byteless; addressing only).
- **analyze** — `content_after` collapses to `token.end()`; pass 18's
  `token_spans` clip comes OUT; anchor leads become genuinely all-chrome;
  corpus law un-weakens back to strict tiling; `note_extents` clips a
  trailing Newline.
- **cst** — Pad is a leaf child like Text (frames/extents indifferent).
- **usj / usx / html / vref / mask** — drop Pad by kind; USJ exact pins and
  codegen round-trip are the gate.
- **format** — delimiter-run collapse becomes "delete Pad"; the in-token
  tail-trim rules retire.
- **lint** — surplus-delimiter lane keeps firing (now over Pad tokens) so
  the ragged render is explained and format owns the labeled cleanup.

## Properties this buys (RFC H/T/R, now statable)

    H1  no hidden byte is a newline.
    H2  a maximal hidden span contains ≤1 whitespace code unit, its LAST byte.
    H3  every hidden span is owned by an anchor that paints.
    H4  deleting an anchor deletes exactly its hidden bytes.
    T1  emitted token spans tile the document.
    R1  a byte's role (chrome | pad | slot | content | structure) is a
        function of the token stream alone — the role IS the kind.

## Out of scope / carried open

- `\v1` name-glue: editor policy (show-or-hide unknowns), not a lexer change.
- `\v 1This` greedy designator: intentional per the verse-designator grammar.
- Line-leading whitespace before a marker stays Text (format's
  MarkerWsAtLineStart lane) — a candidate for Pad later, not this pass.
