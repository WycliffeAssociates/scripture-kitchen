// Recipe: diff. A printable comparison of two versions of a book.
#import "/usfm.typ": *
#import "/usfm.typ" as base

#let red = rgb("#b3261e")
#let green = rgb("#1e6b34")

#let del(body) = text(fill: red, strike(stroke: 0.6pt + red, body)) + h(0.25em, weak: true)
#let ins(body) = text(fill: green, underline(stroke: 0.6pt + green, offset: 1.5pt, body))

#let summary(old, new, changed, same, removed, added, layout) = {
  align(center, text(size: 16pt, weight: "bold", old + " → " + new))
  std.v(0.3em)
  align(center, text(size: 9pt, fill: luma(35%),
    [#changed verses changed · #same unchanged · #removed words removed · #added added]))
  std.v(0.8em)
  if layout == "side" {
    grid(columns: (2.6em, 1fr, 1fr), column-gutter: 4mm, [],
      text(weight: "bold", old), text(weight: "bold", new))
    line(length: 100%, stroke: 0.4pt)
  }
}

#let ref(cv) = text(size: 0.85em, weight: "bold", fill: rgb(s.verse_color), cv)

// One changed verse, old beside new. Running heads read the metadata.
#let row(cv, l, r) = {
  let (c, n) = cv.split(":")
  [#metadata((c: c, v: n))<plate-v>]
  grid(columns: (2.6em, 1fr, 1fr), column-gutter: 4mm, row-gutter: 0pt,
    inset: (y: 0.25em), ref(cv), par(justify: false, l), par(justify: false, r))
}

// One changed verse, read straight through.
#let redline(cv, body) = {
  let (c, n) = cv.split(":")
  [#metadata((c: c, v: n))<plate-v>]
  block(above: 0.45em, below: 0.45em, par(justify: false, hanging-indent: 2.6em, box(width: 2.6em, ref(cv)) + body))
}

// A run of verses with no change.
#let same(range, count) = block(above: 0.3em, below: 0.3em, align(center,
  text(size: 0.8em, style: "italic", fill: luma(50%),
    if count == 1 { range.split("–").first() + " unchanged" } else { range + " unchanged (" + str(count) + " verses)" })))
