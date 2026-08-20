# Format / prettify (roadmap 3.5)

Will's brief: "similar to usfm onion's format — remove extra line
breaks, move paras to their own linebreaks but remove them between
verses etc."

## Architecture (RULED, lint-sketch 2026-08-18): NO subsystem

A formatter is a RULE BUNDLE: each rule is an ordinary lint code whose
findings are all Hint + auto-fixable. "Format document" = collect every
formatter-tagged fix, sort, apply as ONE transaction (the "fix all X"
machinery that already exists). Onion's separate `FormatRule` enum,
per-rule toggles, and token-mutation engine (`format/mod.rs`, 2,989
lines of OwnedToken rewriting) are the MACHINERY we do not port — we
port its RULE LIST onto `LintRow` + `Fix`/`Edit`.

One addition to the row table (PROPOSED): a `formatter: bool` column —
the bundle membership flag "Format document" filters on. (Alternative:
`Category::Form + Hint + has-fix` as the implicit definition — rejected
because empty-paragraph is Info and marker-not-ws-preceded's category
could drift; an explicit bit is one bool per row and no inference.)

## Onion's fifteen rules, dispositioned

Read from ../usfm_onion/src/format/mod.rs (FormatRule::ALL + the
per-rule fns + its three authored marker lists).

**Port as new Hint codes (the whitespace canon — the heart of Will's
brief):**

| new code | onion rule | edit shape |
|---|---|---|
| `para-needs-own-line` | InsertStructuralLinebreaks | insert `\n` before (and after, for onion's BEFORE_AND_AFTER list: p/m/pi#/ms#/li#/b) a paragraph marker not at line start. SUBSUMES marker-not-ws-preceded's fix? No — see "collision" below. |
| `no-break-inside-verse-text` | RemoveUnwantedLinebreaks | delete a Newline token (span delete) between verse text and its continuation inside one paragraph — poetry rows (onion's POETRY list) exempt; their breaks are content. |
| `collapse-blank-lines` | CollapseConsecutiveLinebreaks | N consecutive Newlines → keep one: delete spans 2..N. |
| `collapse-inner-whitespace` | CollapseWhitespaceInText | runs of spaces/tabs inside a Text span → one space (edit rewrites the run). NOT the delimiter (that's the marker's span). |
| `space-after-designator` | NormalizeSpacingAfterParagraphMarkers + EnsureInlineSeparators | ensure exactly one space between a designator/marker and following text where the spec's printing wants one. |
| `marker-ws-at-line-start` | NormalizeMarkerWhitespaceAtLineStart | delete indenting ws before a line-leading marker. |

**Already ours (no new code needed):**

- `marker-not-ws-preceded` — already Hint + `\n` fix (demoted 2026-08-20
  after Will's railroad read). Gets the formatter bit.
- `empty-paragraph` (onion RemoveEmptyParagraphs) — code exists (Info);
  formatter membership means ADDING its fix: delete the node's extent
  (Cst::extent, the "replace whole note" derivation with empty insert).
  Info + fix is fine; the bundle filters on the bit, not the severity.
- missing-paragraph (onion InsertDefaultParagraphAfterChapterIntro) —
  already exists WITH the `\p\n` fix. Formatter bit: yes.

**Dropped, with reasons:**

- RecoverMalformedMarkers — repairs tokens; never-synthesize. Lint
  already flags (`unknown-marker` et al.).
- RemoveDuplicateVerseNumbers — `verse-duplicate` exists; its ruled fix
  is RENUMBER (with the non-improving guard), and DELETE-the-duplicate
  is a content decision (which copy?), not formatting.
- MoveChapterLabelAfterChapterMarker — a MOVE across content, same
  refusal class as attr-trailing-form relocation.
- BridgeConsecutiveVerseMarkers + RemoveBridgeVerseEnumerators +
  RemoveOrphanEmptyVerseBeforeContentfulVerse — uW-chunk-era cleanups
  that REWRITE versification (`\v 1 \v 2` → `\v 1-2`). Semantic
  interpretation, not form. FLAG: if sous/translation flows still need
  these, they're a consumer-side batch tool over the same Edit
  vocabulary, not engine lint. (Open question 3.)

## The idempotence oracle (the new clause)

Every formatter fix already passes `check_fixes` (site repaired,
nothing new, partition holds). The bundle adds one property:

    format(format(x)) == format(x)
    — concretely: apply the whole bundle once, re-lint, assert ZERO
    formatter-tagged findings remain (stronger than fixpoint-in-2).

Corpus test: run the full bundle over all 226 books, assert
convergence-in-one plus byte-diff sanity on a couple of books read by
eye. en_ulb is the stress case (its `\s5` idiom means thousands of
empty-paragraph deletions + blank-line collapses interacting) —
overlapping-edit REJECTION in check_fixes already guards the composed
dispatch; rules whose edits collide (delete a Newline that another rule
also touches) must yield ONE owner: rule precedence = row order,
first-writer wins, second rule's finding simply isn't emitted on a span
another formatter rule claimed (dedup at emit, cheap since formatter
findings are per-span).

**The collision to design around** (why para-needs-own-line and
marker-not-ws-preceded coexist): `text\p more` draws
marker-not-ws-preceded (insert `\n`); `text \p more` (space, not
newline) draws para-needs-own-line. Same family, adjacent byte
conditions — implement as ONE emit site with two codes, or merge into
one code with aux distinguishing? PROPOSED: merge — retire neither
name, but para-needs-own-line becomes the general rule and
marker-not-ws-preceded stays as-is for the zero-ws case only (its name
is now honest: NOT ws-preceded).

## Tests (plain English)

- Each rule: minimal snippet → exactly its finding + fix; apply →
  relint → zero formatter findings (idempotence per-rule).
- Poetry exemption: `\q1` lines keep their breaks; `\m(` prose reflow
  removes an intra-verse break.
- The en_ulb `\s5` block: `\m\n\p\n\v 1` → empty `\m` deleted AND blank
  lines collapsed in one transaction; converges in one pass.
- Bundle over 226 books: convergence-in-one, partition holds, no
  non-formatter finding's count changes (formatting must never alter
  meaning-bearing findings — ordering counts identical before/after).

## Open

1. The `formatter: bool` column vs implicit membership — pick at build.
2. Merge para-needs-own-line / marker-not-ws-preceded or keep two codes?
3. Do the uW bridge/dedup verse rules need a home (consumer-side batch
   tool) or are they dead with the chunk era?
4. Onion's `with_original_spacing` preserved some author spacing
   through normalization — read it before coding collapse-inner-
   whitespace so we don't flatten intentional NBSP typography (NBSP is
   content, never collapsed — needs a ruling line in the rule doc).
