1. Author the table — new marker_rows data file in this repo, named struct fields (ws enums can live in their own module like onion's whitespace.rs, which ported cleanly). Pragmatic move: a throwaway script mechanically translates onion's marker_defs_data.rs into the new schema first, then you audit category-by-category (paragraphs one sitting, char markers another) rather than row-by-row from scratch — audit effort goes into judgment, not typing.
2. Codegen script (a small src/bin/ binary): reads the rows, emits the packed u128 table + the strip-digits-then-match name→idx function into a generated file. JS registry projection comes later, same source.
3. Load into the lexer: marker_idx assignment, the conditional ws fold (per-class, killing the TODO), and the payload column → NumberRange token kind — yes, that's the 9th shape, so NESTED_BIT slides to bit 4 (the comment on it already documents this exact move).
4. Then attributes, on top of the stack that the kind column enables.
e stores facts, codegen turns facts into instructions — same division as everywhere else.
- Delimiter-ws in one instruction: same answer. The general path reads the ws-after-name column; the prelude bakes the delimiter into the pattern itself ("v " as a u16 compare is the delimiter check). Hot markers get it fused for free.
- Multi-token templates ([\c][ ][#][nl][\p][nl] — six tokens, one load): possible, with a warning label. Two real frictions: CR-vs-LF doubles every pattern containing a newline (or needs a normalize-mask trick), and variable digit-count shifts every byte after it — so the tail compare needs a shift-by-digit-run first. Each template is a new bail-laden code path needing oracle coverage. The discipline: build the prelude one pattern at a time, measured — \v +digits first (biggest population), then \w , \zaln-s|, \c +digits — and stop the moment a pattern's delta drops below run noise. My bet is the single-marker patterns capture most of the win and the multi-token templates never earn their bail complexity — but the harness will just tell us.

And yes — everything past "happy shape" falls back: 4+ digits, bridges (1-2), \va, CRLF surprises. Fast path recognizes; the general path defines.




# Document Structure
A USFM or USX document consists of valid elements for Scripture or Peripheral content organized within a sequence of divisions.

Scripture
[Scripture]
[BookIdentification] — Book Identification
[BookHeaders] — Book Headers
[BookTitles] — Book Titles
[BookIntroduction] — Book Introduction
[BookIntroductionEndTitles] — Book Introduction End Titles
[BookChapterLabel] — Book Chapter Label
[ChapterContent] — Chapter Content

Book Identification
[BookIdentification]

USFM: Document Structure > id, usfm
USX: Document Structure > book

Rail Guide "r" = required
{} a stop on the rail
TAGEND
Pattern: /(?:${ws}+|(?=[\\|]|$))/ Delimits a marker

Where rail reads as: 
{slash(r) or /?{anyws}*\\}{id}{space or TAGEND}{bookCodeTable or [0-9A-Z]{3}}{optional hs*}{\n or NL (r)}{O usfm tag and \d+\.\d+(\.\d+)?}{BookHeaders}{BookTitles(R)}{BookIntroduction(R)}{BookIntroductionEndTitles(R)}{BookChapterLabel(O)}{ChapterContent(R)}{Para | Section | Chapter | Milestone} | Footnotes | CrossReference | List | Table | Sidebar}


Book Identification
[BookIdentification]

USFM: Document Structure > id, usfm
USX: Document Structure > book

An optional collection of one or more paragraph elements for book name and abbreviation texts.

Paragraphs > Identification > ide, h, toc#, toca#, rem, sts

Where rail is: 
{\n\\ or /${Ws}\\/}{ide/h1/h2/h3/h/toc1/toc2/toc3/toca1/toca2/toca3/remo/sts}{" " or TAGEND}{O TEXT}{O TEXTEND}

Book Titles
[BookTitles]

A collection of one or more paragraph elements for book main titles.

Paragraphs > Titles and Sections > mt#

Paragraphs > Identification > rem

An optional collection of one or more embedded elements.

[Footnote] — Footnotes

[CrossReference] — Cross References

[Char] — Characters

[Break] — Optional line break



Book Introduction
[BookIntroduction]

USFM

USX

bkintro rail
An optional collection of paragraph and table elements for book introductions.

Paragraphs > Introductions > imt#, imte#, ib, ie, ili#, imi, imq, im, io#, iot, ipi, ipq, ipr, ip, iq#, is#, iex, rem

[Table] — Paragraphs > Tables

An optional collection of one or more embedded elements.

[Footnote] — Footnotes

[CrossReference] — Cross References

[Char] — Characters

[IntroChar] Introduction Characters

[Milestone] — Milestones

Book Introduction End Titles
[BookIntroductionEndTitles]

USFM

USX

bkintroend rail
An optional collection of one or more paragraph elements for book titles occurring at the end of the book introduction.

Paragraphs > Titles and Sections > mt#

An optional collection of one or more embedded elements.

[Footnote] — Footnotes

[CrossReference] — Cross References

[Char] — Characters

[Milestone] — Milestones

[Break] — Optional line break

Book Chapter Label
[BookChapterLabel]

An optional paragraph element used for providing a chapter heading text which may be applied when formatting all chapters as headings.

Paragraphs > Identification > cl

Chapter Content
[ChapterContent]

USFM

USX

chaptercontent rail
An optional collection of chapter, section, paragraph/poetry, list, table, or sidebar elements for the main content of a scripture book.

[Chapter] — Chapters and Verses > c

[Section] — Paragraphs > Titles and Sections > cd, cl, mr, ms#, mte#, r, s#, sp, sd#, sr

Paragraphs > Introductions > iex, ip (study Bibles)

[Para] — Paragraphs > Body Paragraphs > b, cls, m, mi#, nb, p, pc, ph, pi#, pm, pmc, pmo, pmr, po, pr

Paragraphs > Poetry > b, q#, qa, qc, qd, qm#, qr

[List] — Paragraphs > Lists > lf, lh, li#, lim#

[Table] — Paragraphs > Tables

[Sidebar] — Sidebars

An optional collection of one or more embedded elements.

[Verse] — v

[Footnote] — Footnotes

[CrossReference] — Cross References

[Char] — Characters

[Milestone] — Milestones

[Break] — Optional line break

Peripheral
[Peripheral]

See the documentation section on peripherals for more detail on the strategy for marking project peripheral contents.

[PeripheralBook] — Peripheral Book - Standalone peripheral book.

[PeripheralDividedBook] — Peripheral Divided Book - Peripheral book with optional divisions.

Peripheral Book (Standalone)
[PeripheralBook]

[BookHeaders] — Book Headers

[BookTitles] — Book Titles

[BookIntroduction] — Book Introduction

[BookIntroductionEndTitles] — Book Introduction End Titles

[PeripheralContent] — Peripheral Content

Peripheral Divided Book
[PeripheralDividedBook]

[PeripheralDivision] — Peripheral Division

Peripheral Division
[PeripheralDivision]

USFM

USX

periph rail
Peripherals > periph - Peripheral division identifier

[BookHeaders] — Book Headers

[BookTitles] — Book Titles

[BookIntroduction] — Book Introduction

[BookIntroductionEndTitles] — Book Introduction End Titles

[PeripheralContent] — Peripheral Content

Peripheral Content
[PeripheralContent]

USFM

USX

chaptercontent rail
An optional collection of chapter, section, paragraph/poetry, list, table, or sidebar elements for the main content of a scripture book.

[Chapter] — Chapters and Verses > c

[Section] — Paragraphs > Titles and Sections > cd, cl, mr, ms#, mte#, r, s#, sp, sd#, sr

Paragraphs > Introductions > iex, ip

[Para] — Paragraphs > Body Paragraphs > b, cls, m, mi#, nb, p, pc, ph, pi#, pm, pmc, pmo, pmr, po, pr

Paragraphs > Poetry > b, q#, qa, qc, qd, qm#, qr

[List] — Paragraphs > Lists > lf, lh, li#, lim#

[Table] — Paragraphs > Tables

[Sidebar] — Sidebars

An optional collection of one or more embedded elements.

[Verse] — v

[Footnote] — Footnotes

[CrossReference] — Cross References

[Char] — Characters

[Milestone] — Milestones

[Break] — Optional line break
