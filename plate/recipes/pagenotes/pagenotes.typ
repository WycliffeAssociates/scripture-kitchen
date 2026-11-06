// Recipe: pagenotes. One notes band per page, full width, in columns. The
// Rust driver decides which notes each page gets; this only draws them.
#import "/usfm.typ": *
#import "/usfm.typ" as base

#let study-size = s.at("study_size", default: 7.3)

// Every verse leaves a mark the driver reads back.
#let v(b, c, n) = [#metadata(c + ":" + n)<pn-verse>] + base.v(b, c, n)

// Every paragraph start leaves a mark too; bands anchor there.
#let pmark(i) = [#metadata(str(i))<pn-para>]

// One note inside a band.
#let pn(ref, body) = block(above: 0pt, below: 0.3em, breakable: true, {
  set par(justify: true, first-line-indent: 0pt, hanging-indent: 0pt, leading: 0.38em)
  text(weight: "bold", fill: rgb(s.verse_color), ref)
  h(0.4em)
  body
})

// A page's band: a page-wide float at the bottom; the text flows above it.
// The driver splits the notes into two balanced halves.
#let band(page, left, right) = place(bottom, scope: "parent", float: true, clearance: 0.9em,
  block(width: 100%, {
    [#metadata(str(page))<pn-band>]
    line(length: 100%, stroke: 0.5pt + luma(40%))
    std.v(0.15em)
    set text(size: study-size * 1pt)
    grid(columns: (1fr, 1fr), column-gutter: 4mm, left, right)
    [#metadata(str(page))<pn-band-end>]
  }))

#let md-h(level, body) = strong(body) + [ ]
#let md-p(body) = body + [ ]
#let md-li(body) = [• ] + body + [ ]
#let md-quote(body) = emph(body) + [ ]
