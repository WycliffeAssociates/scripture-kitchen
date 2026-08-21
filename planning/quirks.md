# Cross-module quirks

Facts that belong to no single module — fixture errata, ambiguities the corpus
cannot settle, and one projection rule all three exports share. A module doc may
point here for these; nothing else in the code should reference planning/.

## Closed span nests inside note text (usj, usx, html)

Inside a note, an explicitly-closed character sibling is an INLINE SPAN: it
re-parents into the open note-text element, and the direct note content after it
resumes that element.

```text
\f + \ft alpha \xt ref\xt* beta\f*   ft: "alpha <xt>ref</xt> beta"
\x - \xo 1.1 \xt Ps 135\x*           xo and xt stay peers
```

Two conditions, both necessary: an explicit closer, AND a marker foreign to this
note family's own peers. The closer alone regresses five fixtures. The CST keeps
the flat peer reading — this is projection-only, in all three exports.

## A lift that would hold markup does not happen

`\ca`/`\va`/`\cp`/`\vp` content lifts to an attribute (`altnumber`,
`pubnumber`, `data-altnumber`, …) — unless it is not plain text, because an
attribute cannot hold markup:

```text
\vp 1b\vp*                  pubnumber="1b"
\vp \+it 21\+it*\vp*        an ordinary vp span, nesting intact
```

Both fixture formats agree on this, so all three exports read it the same way
(biblica/PublishingVersesWithFormatting).

## The delimiter-space ambiguity

A space on either side of a marker seam serializes back to identical USFM, so
USJ/USX each have two truthful spellings of it. We fold it as the delimiter.
Six usfmjsTests fixtures (isa_inline_quotes, isa_verse_span, misc_footnotes,
pro_quotes, tit_1_12_footnote, isa_footnote) put it on the content side instead —
and their own `origin.xml` files agree with us against their `origin.json`. The
same fixture pair disagrees with itself on identical bytes.

## testData fixture errata

- **Phantom spaces.** biblica/CrossRefWithPipe (trailing space where the source
  ends without a newline); special-cases/empty-attributes; paratextTests/
  WordlistMarkerMissingFromGlossaryCitationForms (a space at the
  `definition\v 2` seam, in both JSON and XML); specExamples/footnote (leading
  space at the `\fv*\ft` seam its own XML lacks).
- **Literal newlines.** specExamples/table keeps a raw newline + indent inside a
  cell; specExamples/extended/contentCatogories1 and specExamples/milestone keep
  literal newlines in note text; advanced/footnote-structures keeps one at the
  `\f*` / `\v 2` seam. Everything else folds them to one space, so the corpus has
  no single whitespace convention.
- **Sid-less fixtures.** advanced/complex and advanced/footnote-structures omit
  every chapter/verse `sid` (and `eid`) the rest of the corpus carries.
- **An invented escape dialect.** special-cases/figure_with_quotes_in_desc
  unescapes `alt="He said: \"…\""`. USFM defines NO escapes, so `\"` lexes as a
  marker; `src/attributes.rs` states the law. Excluded from both the USJ and USX
  pins for that reason.
- **Fixtures that contradict their own sibling.**
  biblica/PublishingVersesWithFormatting's JSON says `code="XXA"` where the
  source says `\id MAT` and its XML says `MAT`;
  usfmjsTests/usfmBodyTestD's JSON reads `\fqa … \fv 8\fv* tail` as three
  note-level siblings where its XML nests both inside `\fqa`;
  paratextTests/NoErrorsPartiallyEmptyBook's XML swallows `\h` and `\mt1` into
  the preceding `\rem` where its JSON keeps three paras. So an exclusion list
  entry is often "this fixture disagrees with itself", not "we differ".
- **`\b` drops its `content` key** when empty, where every other empty element
  keeps `content: []`.
- **BOM prefixes** are common; BSB Ecclesiastes ships with NO `\id` line at all,
  so `Toc::book_token == None` is the honest answer and `missing-id` is lint's.
- **Malformed payloads in shipped books.** en_ulb ZEC 12:7 is written `\v 7"`
  (the quote is glued to the number, so the designator span is `7"`); bdf_reg ROM
  3 writes `\v 10` twice then `\v 11`; examples.bsb's only non-`+` note caller is
  a stray `",`.

## Unknown-marker recovery: the corpus cannot settle it

`\s5` (an unfoldingWord chunk marker, not a spec marker) occurs 299 times across
21 validated-pass fixtures. 19 read it our way — pop all frames, start fresh. The
two that don't (usfmjsTests/luk_quotes, usfmjsTests/usfm-body-testF) read it two
DIFFERENT ways from each other: one wants it to swallow the following `\v 17`
text, the other wants it not to swallow but to leave an `\esb` open.
specExamples/milestone is the same shape for a row-0 milestone `\zms\*`.

## Wild-corpus errata

- **`en_ulb` REV writes `\m(for fine linen…`** — a non-whitespace marker
  delimiter in shipped data.
- **`\s5` is everywhere**: 13,636 occurrences in en_ulb alone, so most of that
  book's unknown-marker findings are one non-standard marker, not a lint bug.
- **Three of the four corpora ship no `\usfm` line at all**, so any rule or
  export keyed on a declared version must treat absent as absent, not as 3.0.

## `\usfm` carves no payload

The scanner leaves the declared version as ordinary `Text` isolated by its line
ending, so every reader takes it off the ADJACENT token — the same adjacency
shape `\ca`/`\cp` need.

## usfmtc repairs; we flag

- **Attribute lists.** usfmtc silently rereads a failed pair as the default
  value and drops everything after a comma. We record `Malformed` — see
  src/attributes.rs.
- **Missing paragraphs.** usfmtc fabricates a `\p` in front of a paragraph-less
  verse run. We propose it as a fix instead of writing it.
- **USJ envelope version.** testData and scripture-editors' `USJ_VERSION` say
  "3.1"; usfmtc says "3.0".

## Corpus shape that gates rule design

en_ult declares `\usfm 3.0` while carrying 792,414 TRAILING attribute lists and
4.35M `x-`/`z-` attributes; 461,352 `\zaln-s` lists are over half of every
attribute byte in the corpus. Any rule that fires per attribute list has to
survive that, which is why several are aggregated rather than per-occurrence.

## Rules with no real data behind them

No `sid=` and no `\ta` appear anywhere in example-corpora (226 books), so lint's
`attr-required-if` milestone-pairing rule has never met real input.
