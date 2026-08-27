# Positional gaps: duplicate \id and body content before \c

STATUS: BUILT (pass 17, 2026-08-26) — see planning/choices.md for the
self-report; the amendment landed narrower than sketched (Paragraph
rows only — `\ca`-class masks are walker mechanics) and the finding
is judged at a new `Flat::finish` (a book must HAVE a `\c` for
"before the first chapter" to mean anything). Found in the spike:
`\id dem \id MAT \c 1` draws no finding on the second `\id`, and
`\id dem \p a \c 1` draws none on the `\p`.

## Why nothing fires today (traced, both)

- **Duplicate `\id`:** the band judge (flat.rs) is the positional
  machine, and it judges POSITION CLASS, never cardinality. After the
  first `\id` the band sits at BookIdentification, where a second
  `\id` is "legal where we are". (A LATE `\id` after `\c` IS caught —
  every positional context behind → `marker-out-of-band`.) No
  once-per-book rule exists; the only `\id` rules are `missing-id`
  and `book-code-not-uppercase`.
- **`\p` before the first `\c`:** the band judge deliberately
  abstains for any marker whose context mask carries a container bit
  (`mask & !POSITIONAL == 0` is the guard, flat.rs). `\p`'s row is
  `[ChapterContent, PeripheralContent]`, and PeripheralContent is a
  container context — so `\p` is never judged positionally, and the
  walker's container axis judges nesting, not book progression.
  Nobody owns "body paragraph at book level in header/intro
  territory".

## Representation: the rails are ALREADY encoded

The USX "Document Structure" divisions Will pasted
(docs.usfm.bible/usfm/3.2) map 1:1 onto `SpecContext` — the enum IS
that page, variant for variant — and each marker row's
`allowed_contexts` is its rail membership (`\ip:
[BookIntroduction, ChapterContent]` matches the "study Bibles" dual
listing exactly). The division SEQUENCE is the band judge's
POSITIONAL fold plus forward-only advancement. So no new grammar
vocabulary is needed; the gaps are enforcement policy:

## The plan

1. **`duplicate-id` (new lint lane, Flat).** Track the first `\id`
   marker (the `ca`/`cp` resolved-idx pattern); a second sighting
   emits with `anchor` = the second, `second` = the first (the
   `numbering-mix` shape). RULED (Will): never a second `\id`
   anywhere; the FIRST is assumed to be the one in use. Fixless to
   start — deleting the second line is a candidate fix, unruled.
2. **Judge the positional half at positional positions (amendment).**
   The abstention becomes conditional: judge a marker's positional
   bits when the context it OPENS IN is itself positional (top level
   — not inside a peripheral, sidebar, note, or table). The enclosing
   context is walk-level knowledge: the CST stamps `Node.ctx`; the
   walk hands each leaf "is my enclosing context positional" (for a
   node's own opening marker that is the PARENT frame's context).
   Gated on the book being scripture (`BOOK_CODES` index before the
   FRT..NDX peripheral tail, captured at the BookCode leaf) so a
   top-level `\p` in FRT/GLO stays legal PeripheralContent.
3. **`paragraph-before-first-chapter` (new lint lane, Flat).** With
   the amendment, a top-level `\p` in header territory would silently
   ADVANCE the band into ChapterContent (advancing is the judge's
   normal move). Instead: when the advance target is ChapterContent
   and the marker is not `\c`, and the marker is a BODY or POETRY
   paragraph (Category ParaBody|ParaPoetry — NOT ParaSection: `\ms`
   before `\c 1` is live corpus practice and spec-ambiguous), emit,
   then advance anyway (one finding per book, no cascade — the
   ordering machine's resync philosophy).
4. **Context audit (separate, later).** Will's pasted rails give
   per-division marker lists finer than some rows declare
   (BookHeaders = ide, h, toc#, toca#, rem, sts; BookTitles adds
   embedded Char/Footnote). An audit pass over `allowed_contexts`
   against the 3.2 docs, curation citations in comments, in the
   existing style.

## Open rulings (ruled 2026-08-27 except where noted)

- Is `\usfm` also once-per-book? RULED YES — same machinery as
  duplicate-id, and the duplicate offers a delete-the-line fix.
  Rolls into the chunk-fold lint pass (chunk-fold.md "Roll-ins").
- `\h`, `\toc#` cardinality — later, with the audit. (Still open.)
- Fix for `duplicate-id`: RULED — offer the fix, delete the second
  line. Rolls into the chunk-fold lint pass.
- Section paragraphs (`\ms`, `\s`) before the first `\c`: DEFERRED
  (Will, 2026-08-27 — "not sure") — stays silent for now; revisit
  with the audit.
