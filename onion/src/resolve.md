# Resolve: an address to its place in one text

```text
toc.resolve(source, GEN)          → found 0..len
toc.resolve(source, GEN intro)    → found 0..8          the front matter, chapter row 0
toc.resolve(source, LUK 1:2)      → found over `\v 1-2`, covers LUK 1:1-2
toc.resolve(source, LUK 1:2)      → missing verse within 1's span, when the text has `\v 1,3`
toc.resolve(source, LUK 30)       → missing chapter
toc.resolve(source, LUK 3)        → ambiguous [first \c 3, second \c 3]
```

```rust
Toc::resolve(&self, source: &str, address: &Address) -> Resolution

Resolution::Found { span, opening, covers, coarser }
Resolution::Missing { missing: Intro | Chapter | Verse, within: Option<span> }
Resolution::Ambiguous(Vec<Candidate { span, opening }>)
```

Every answer comes from the one text's `Toc`, never a silent nearest. The
address's book is the caller's to match: resolve answers for this text.
Offsets are UTF-8 bytes into `source`, half-open. The cases, as data any
language can run, are `testData/resolve/cases.json`; every rule below is one or
more of them, named in quotes.

Extents are STRUCTURAL, as the `Toc`'s are: a verse runs from its `\v` to the
next `\v` or the end of its chapter, so a heading before verse 4 sits inside
verse 3's span. `opening` is the answer for a caller that wants the heading.

## The rules

**A book** is the whole text. "the book is the whole text"

```text
\id GEN\n\c 1\n…\v 1 Thus.\n     GEN → found 0..74, covers GEN
```

**An intro** is chapter row 0, the bytes before the first `\c`. It is missing
when that row is empty. "the intro is the front matter", "an empty intro is
missing"

```text
\id GEN\n\mt1 Genesis\n\c 1…     GEN intro → found 0..21
\c 1\n\p\n\v 1 a\n               GEN intro → missing intro
```

**Chapters** span their rows, first to last. A number two `\c` rows carry is
ambiguous, with both rows. "one chapter runs to the next", "a chapter range
spans its rows", "two chapter 3s are ambiguous", "a chapter the text lacks is
missing"

```text
\c 1 … \c 2 … \c 3 …             GEN 1-2 → found \c 1 .. \c 3
\c 3\n\p a\n\c 3\n\p b\n         GEN 3   → ambiguous [0..10, 10..20]
```

**A verse** is the one `\v` covering it, to the next `\v` or `\c`. A range
spans its two ends, across a chapter if it says so. "a verse runs to the next
verse or chapter", "a verse range spans both ends", "a range may cross a
chapter"

**A bridge** covers each verse it joins, and `covers` says what was found. "a
verse in a bridge covers the bridge"

```text
\v 1-2 a b\n\v 3 c               LUK 1:2 → found \v 1-2 .. \v 3, covers LUK 1:1-2, not coarser
```

**A list** covers its members and not its holes. Found, it covers its hull,
because an address is one range and cannot say "1 and 3". "a verse a list
skips is missing", "a verse a list names covers its hull"

```text
\v 1,3 a\n\v 4 b                 LUK 1:2 → missing verse within chapter 1
                                 LUK 1:3 → found \v 1,3 .. \v 4, covers LUK 1:1-3
```

**A segment** the text does not divide is found COARSER: the whole verse, said
so. A whole verse the text divides spans its first segment to its last. A
segment the text divides is exact. "a segment the text does not divide is
found coarser", "a whole verse the text divides spans its segments", "a segment
the text divides is exact"

```text
\v 3 a                           LUK 1:3a → found \v 3, covers LUK 1:3, coarser
\v 3a a\n\v 3b b\n\v 4 c         LUK 1:3  → found \v 3a .. \v 4, covers LUK 1:3a-3b
                                 LUK 1:3b → found \v 3b .. \v 4, covers LUK 1:3b
```

**Two units covering one place** is malformed text, and ambiguous with every
candidate, in source order. Two segments naming the same part are too. "two
anchors for one verse are ambiguous"

**A missing verse** names the chapter it would be in (`within`), so a caller
can choose to stop there. A range with either end missing is missing that end.
A verse in a chapter the text lacks is a missing chapter. "a missing verse
names its chapter", "a range with a missing end is missing", "a verse in a
missing chapter is a missing chapter"

## Opening

`opening..to` is what a multibuffer clips; `from..to` is what a check reads.
`opening` is where the run of OPENERS directly before the first `\v` starts. It
equals `from` mid-paragraph, and for every answer that is not a verse.

```text
\v 1 a\n\p\n\v 2 b               opening at \p
\v 1 a\n\s1 A heading\n\p\n\v 2  opening at \s1
\q1 \v 1 one\n\q2 two\n\q1 \v 2  opening at the second \q1; `\q2 two` is verse 1's
\q1 \v 1 a\n\b\n\q1 \v 2 b       opening at \b
\p \v 1 a \v 2 b                 opening == from
\c 1\n\p\n\v 1 a                 opening at \p, never \c
\c 1\n\cl Psalm 1\n\p\n\v 1 a    opening at \p
\c 3\n\d A psalm of David.\n\q1 \v 1   opening at \d
\s1 Title\n\p\n\v 1 a\n\p\n\v 2  GEN 1:1-2 opens at \s1, where its first verse does
```

Walking back from the `\v`, never past the previous `\v` or the chapter's `\c`:

- A LINE OPENER, a paragraph marker of the table's `ParaBody`, `ParaPoetry` or
  `ParaLists` category (`\p`, `\m`, `\pi#`, `\nb`, `\b`, `\q#`, `\qc`, `\li#`,
  …), joins the run only when nothing but whitespace lies between it and the
  run. Text after one means the verse starts mid-paragraph.
- A HEADING, a paragraph marker of `ParaTitlesSections` (`\s#`, `\ms#`, `\mr`,
  `\r`, `\d`, `\sp`, `\sr`, `\sd#`, and `\z` markers registered as titles or
  sections), joins the run WITH its text, notes and character markup. So does
  `\qa`, the acrostic heading the table files under poetry.
- Not openers: `\mt#` and `\mte#` (a book's titles, not a verse's), `\cl` and
  `\cd` (they label the chapter), and everything else. The walk stops at the
  first one, and at a milestone.

A verse range opens where its first verse does. A whole verse the text divides
opens where its first segment does. An ambiguous candidate carries its own.

## Where this departs from Sefer's `locate.ts`

- `opening` is new; `locate.ts` has no such field. Its rule above is a choice
  this module makes, from onion's marker table.
- Segments order by their bytes (Rust `str` order). `locate.ts` orders them by
  `localeCompare(…, "en")`. The two agree on lower-case `a`-`z`, which is every
  segment the cases and the corpora hold; they differ on mixed case.
- `Ambiguous` carries `{ span, opening }` per candidate, where `locate.ts`
  carries bare spans.
- Offsets are UTF-8 bytes here, UTF-16 in `locate.ts`. The galley door converts
  when asked (`utf16: true`).
- `locate.ts` reads a `TocView` adapted from the engine's rows; this reads the
  `Toc` and the source directly. The source is needed for segment letters,
  which the `Toc` holds as spans, and for the opening walk.
