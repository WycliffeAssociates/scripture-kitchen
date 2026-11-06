// Recipe: diglot. Two texts side by side, one row per paragraph.
#import "/usfm.typ": *
#import "/usfm.typ" as base

#let gutter = 6mm

#let labels(l, r) = grid(columns: (1fr, 1fr), column-gutter: gutter,
  align(center, text(weight: "bold", l)), align(center, text(weight: "bold", r)))

// A verse number inside a cell; the running head still reads it.
#let vn(c, n) = [#metadata((c: c, v: n))<plate-v>] + box(super(typographic: false,
  text(size: 0.95em, weight: "bold", fill: rgb(s.verse_color), if n == "1" { c + ":" + n } else { n }))) + h(0.12em)

#let chapter-row(c) = block(sticky: true, above: 1em, below: 0.4em, width: 100%,
  align(center, text(size: 1.4em, weight: "bold", fill: rgb(s.verse_color), c)))

// One paragraph pair. The row may break across a page; both halves break with it.
#let pair(l, r) = grid(columns: (1fr, 1fr), column-gutter: gutter,
  stroke: (x, y) => if x == 1 { (left: 0.3pt + luma(70%)) },
  inset: (x, y) => if x == 1 { (left: gutter / 2) } else { (right: 0pt) },
  par(justify: s.justify, first-line-indent: s.indent * 1em, l),
  par(justify: s.justify, first-line-indent: s.indent * 1em, r))
