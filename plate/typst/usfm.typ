// plate's template: every visual decision for USFM printed through Typst.
//
// The markup onion writes calls five functions — book, c, para, v, ch, n —
// with marker names, never styles. Settings come from /settings.json and are
// laid over `defaults` (right wins). Lengths in settings are plain numbers:
// mm for page geometry, pt for type sizes, em for indents.

#let defaults = (
  paper: "a5",              // a5 | a4 | letter | trade | 6x9 | pocket
  margin_top: 14,           // mm
  margin_bottom: 14,
  margin_inside: 16,
  margin_outside: 12,
  columns: 2,
  gutter: 5,                // mm
  column_rule: true,
  font: "Libertinus Serif",
  size: 9.5,                // pt
  leading: 0.5,             // em between lines
  justify: true,
  hyphenate: true,
  lang: "en",
  dir: "auto",              // auto | ltr | rtl
  widows: true,             // avoid widows and orphans
  indent: 1,                // em, first line of \p
  poetry_indent: 1,         // em per \q level
  poetry_wrap: 3,           // em, where a wrapped poetry line resumes
  verse_numbers: "super",   // super | bold | none
  verse_color: "#7a1f1f",
  chapter: "inline",        // inline | heading | drop (experimental)
  chapter_size: 2.6,        // em, for drop and inline
  headings: true,           // \s, \ms
  parallel_refs: true,      // \r
  title_size: 22,           // pt, \mt1
  red_letter: false,        // \wj
  red: "#b3261e",
  footnotes: true,
  footnote_numbering: "a",  // a | 1 | * | i
  footnote_reset: "page",   // page | chapter | book
  footnote_ref: true,       // print \fr (1:1)
  note_size: 7.5,           // pt
  xrefs: true,
  running_head: "range",    // range | book | none
  page_numbers: true,
  book_break: "page",       // page | odd | column | none
)

#let s = defaults + json("/settings.json")

#let mm(x) = x * 1mm
#let papers = (
  a5: (148, 210), a4: (210, 297), letter: (215.9, 279.4),
  trade: (139.7, 215.9), "6x9": (152.4, 228.6), pocket: (110, 170),
)

// ---------------------------------------------------------------------------
// Running heads: verses and books leave metadata; a header binary-searches the
// verse list for the first and last verse on its page.

#let book-of-page(pg) = {
  let found = none
  for b in query(<plate-book>) {
    if b.location().page() <= pg { found = b.value } else { break }
  }
  found
}

#let lower-bound(vs, pg) = {
  let lo = 0
  let hi = vs.len()
  while lo < hi {
    let mid = calc.quo(lo + hi, 2)
    if vs.at(mid).location().page() < pg { lo = mid + 1 } else { hi = mid }
  }
  lo
}

#let ref-of(m) = m.value.c + ":" + m.value.v

#let head-range(pg) = {
  let vs = query(<plate-v>)
  let i = lower-bound(vs, pg)
  let j = lower-bound(vs, pg + 1) - 1
  if i > j { return none }
  let (a, b) = (vs.at(i).value, vs.at(j).value)
  if a.c == b.c { a.c + ":" + a.v + "–" + b.v } else { ref-of(vs.at(i)) + "–" + ref-of(vs.at(j)) }
}

#let header() = context {
  if s.footnote_reset == "page" { counter(footnote).update(0) }
  if s.running_head == "none" { return }
  let pg = here().page()
  let book = book-of-page(pg)
  if book == none { return }
  // No head on the page a book starts.
  let starts = query(<plate-book>).any(b => b.location().page() == pg)
  if starts { return }
  let label = if s.running_head == "range" {
    let r = head-range(pg)
    if r == none { book.name } else { book.name + " " + r }
  } else { book.name }
  let number = if s.page_numbers { str(counter(page).get().first()) } else { "" }
  set text(size: s.size * 0.9pt)
  if calc.odd(pg) {
    grid(columns: (1fr, auto), smallcaps(label), number)
  } else {
    grid(columns: (auto, 1fr), number, align(right, smallcaps(label)))
  }
  std.v(-0.4em)
  line(length: 100%, stroke: 0.3pt)
}

#let footer() = context {
  if not s.page_numbers { return }
  let pg = here().page()
  // Pages with a running head carry their number there.
  if s.running_head != "none" and not query(<plate-book>).any(b => b.location().page() == pg) { return }
  set text(size: s.size * 0.85pt)
  align(center, str(counter(page).get().first()))
}

#let rule() = context {
  if not s.column_rule or s.columns < 2 { return }
  let (w, h) = papers.at(s.paper, default: papers.a5)
  let pg = here().page()
  let ml = if calc.odd(pg) { s.margin_inside } else { s.margin_outside }
  let mr = if calc.odd(pg) { s.margin_outside } else { s.margin_inside }
  let x = ml + (w - ml - mr) / 2
  place(top + left, dx: mm(x), dy: mm(s.margin_top + 2),
    line(angle: 90deg, length: mm(h - s.margin_top - s.margin_bottom - 4), stroke: 0.3pt + luma(60%)))
}

// ---------------------------------------------------------------------------

#let setup(body) = {
  let (w, h) = papers.at(s.paper, default: papers.a5)
  set page(
    width: mm(w), height: mm(h),
    margin: (top: mm(s.margin_top), bottom: mm(s.margin_bottom),
             inside: mm(s.margin_inside), outside: mm(s.margin_outside)),
    columns: s.columns,
    header: header(),
    footer: footer(),
    background: rule(),
  )
  set columns(gutter: mm(s.gutter))
  set text(
    font: (s.font, "Libertinus Serif"),
    size: s.size * 1pt,
    lang: s.lang,
    dir: if s.dir == "auto" { auto } else if s.dir == "rtl" { rtl } else { ltr },
    hyphenate: s.hyphenate,
    costs: if s.widows { (widow: 100%, orphan: 100%) } else { (widow: 0%, orphan: 0%) },
  )
  set par(justify: s.justify, leading: s.leading * 1em, spacing: s.leading * 1em,
          first-line-indent: 0pt)
  set block(spacing: s.leading * 1em)
  set footnote(numbering: s.footnote_numbering)
  set footnote.entry(separator: line(length: 25%, stroke: 0.3pt), gap: 0.25em, clearance: 0.6em)
  show footnote.entry: set text(size: s.note_size * 1pt)
  show footnote.entry: set par(justify: false, first-line-indent: 0pt, hanging-indent: 0pt)
  body
}

// ---------------------------------------------------------------------------
// Book, chapter, verse

#let book(code, name) = {
  if s.book_break == "page" { pagebreak(weak: true) }
  else if s.book_break == "odd" { pagebreak(weak: true, to: "odd") }
  else if s.book_break == "column" { colbreak(weak: true) }
  if s.footnote_reset == "book" { counter(footnote).update(0) }
  [#metadata((code: code, name: name))<plate-book>]
}

#let c(n) = {
  if s.footnote_reset == "chapter" { counter(footnote).update(0) }
  if s.chapter == "heading" {
    block(sticky: true, above: 1.2em, below: 0.6em, width: 100%,
      align(center, text(size: s.size * 1.6pt, weight: "bold", n)))
  }
}

#let chapter-number(n) = text(size: s.chapter_size * 1em, weight: "bold", fill: rgb(s.verse_color), n)

// A drop number: a box as tall as one line, the number hung down into the
// lines below it. Typst cannot wrap text around a float, so the second line
// is indented by a hanging indent on the paragraph instead (see para()).
#let drop-width(n) = measure(chapter-number(n)).width + 0.3em

#let v(c, n) = {
  [#metadata((c: c, v: n))<plate-v>]
  if s.verse_numbers == "none" { return }
  if n == "1" and s.chapter != "heading" { return }
  if s.verse_numbers == "bold" {
    box(text(weight: "bold", fill: rgb(s.verse_color), n)) + h(0.25em)
  } else {
    box(super(typographic: false, text(size: 0.95em, weight: "bold", fill: rgb(s.verse_color), n))) + h(0.12em)
  }
}

// ---------------------------------------------------------------------------
// Paragraphs, by marker name (number stripped; `lvl` is the number, 0 if
// none was written).

#let heading-block(body, size: 1, style: "normal", weight: "bold", above: 0.9em, below: 0.35em) = block(
  sticky: true, above: above, below: below, width: 100%,
  align(center, par(justify: false, first-line-indent: 0pt, hanging-indent: 0pt, text(size: size * 1em, style: style, weight: weight, hyphenate: false, body))))

#let para(m, lvl, c: none, body) = {
  let level = calc.max(lvl, 1)
  let pi = s.poetry_indent * 1em
  // A chapter number opens the first prose paragraph of its chapter.
  let lead = if c != none and s.chapter == "inline" {
    box(chapter-number(c)) + h(0.2em)
  } else { none }
  let drop = c != none and s.chapter == "drop"
  let prose(indent: 0em, first: 0em, hang: 0em, justify: s.justify, al: start) = {
    if drop {
      // Drop number: two lines tall, the paragraph indented beside it by a
      // hanging indent — every line after the first, not just the second.
      context {
        let size = 2 * s.leading * 1em + 2 * 1em
        let num = text(size: size * 0.82, weight: "bold", fill: rgb(s.verse_color), top-edge: "cap-height", bottom-edge: "baseline", c)
        let w = measure(num).width + 0.25em
        block(inset: (left: indent), {
          set par(first-line-indent: 0pt, hanging-indent: w, justify: justify)
          box(width: w, height: 0.7em, place(top + left, dy: -0.05em, num))
          body
        })
      }
    } else {
      block(inset: (left: indent), {
        set par(first-line-indent: (amount: first, all: true), hanging-indent: hang, justify: justify)
        align(al, lead + body)
      })
    }
  }
  if m == "p" or m == "po" { prose(first: s.indent * 1em) }
  else if m == "m" or m == "nb" or m == "lh" or m == "lf" or m == "pmo" or m == "pm" or m == "pmc" { prose() }
  else if m == "pi" or m == "mi" or m == "pr" { prose(indent: level * s.indent * 1em, first: if m == "pi" { s.indent * 1em } else { 0em }) }
  else if m == "pc" or m == "qc" { prose(al: center, justify: false) }
  else if m == "qr" { prose(al: end, justify: false) }
  else if m == "q" or m == "qm" {
    let start = level * pi
    prose(indent: start, hang: calc.max(s.poetry_wrap * 1em - start, 0.5em), justify: false)
  }
  else if m == "li" or m == "lim" {
    prose(indent: level * s.indent * 1em, hang: s.indent * 1em, justify: false)
  }
  else if m == "b" { std.v(s.leading * 1em) }
  else if m == "mt" or m == "imt" {
    let size = if level == 1 { s.title_size } else { s.title_size * 0.6 }
    place(top + center, scope: "parent", float: true, clearance: if level == 1 { 1.4em } else { 0.4em },
      block(width: 100%, align(center, text(size: size * 1pt, weight: if level == 1 { "bold" } else { "regular" }, hyphenate: false, body))))
  }
  else if m == "ms" or m == "mr" or m == "imte" or m == "mte" {
    if s.headings { heading-block(body, size: if m == "mr" { 0.9 } else { 1.25 }, style: if m == "mr" { "italic" } else { "normal" }, above: 1.4em) }
  }
  else if m == "s" or m == "is" or m == "sd" {
    if s.headings { heading-block(body, size: if level == 1 { 1.0 } else { 0.92 }, style: if level == 1 { "normal" } else { "italic" }) }
  }
  else if m == "r" or m == "sr" {
    if s.parallel_refs { heading-block(body, size: 0.85, style: "italic", weight: "regular", above: 0.1em, below: 0.4em) }
  }
  else if m == "d" or m == "qa" or m == "sp" or m == "cd" or m == "cl" or m == "qd" {
    heading-block(body, size: 0.92, style: "italic", weight: "regular", above: 0.4em, below: 0.4em)
  }
  else if m == "ip" or m == "ipi" or m == "im" or m == "imi" or m == "ipq" or m == "ipr" or m == "iex" { prose(first: s.indent * 1em) }
  else if m == "io" or m == "iot" or m == "ili" or m == "iq" { prose(indent: level * s.indent * 1em, justify: false) }
  else if m == "tr" { prose(justify: false) }
  else { prose(first: s.indent * 1em) }
}

// ---------------------------------------------------------------------------
// Character styles and notes

#let chars = (
  wj: b => if s.red_letter { text(fill: rgb(s.red), b) } else { b },
  add: emph, bk: emph, it: emph, em: emph, tl: emph, sls: emph, ior: emph, iqt: emph,
  nd: smallcaps, sc: smallcaps,
  bd: strong, bdit: b => strong(emph(b)), no: b => text(style: "normal", weight: "regular", b),
  sup: b => super(b),
  qs: b => [#h(1fr)#emph(b)],
  qac: strong,
  fr: b => if s.footnote_ref { strong(b) } else { none },
  xo: b => if s.footnote_ref { strong(b) } else { none },
  fq: emph, fqa: emph, xq: emph, fk: b => strong(emph(b)), xk: b => strong(emph(b)),
  fv: b => super(b),
  rq: b => text(size: 0.85em, style: "italic", b),
)

#let ch(m, body) = (chars.at(m, default: b => b))(body)

#let n(kind, caller, body) = {
  let xref = kind == "x" or kind == "ex"
  if xref and not s.xrefs { return }
  if not xref and not s.footnotes { return }
  if caller == "-" { return }
  footnote(numbering: if xref { "*" } else { s.footnote_numbering }, body)
}
