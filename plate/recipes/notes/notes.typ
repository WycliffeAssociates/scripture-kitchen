// Recipe: notes. A study edition — notes after the paragraph that holds
// their verse, introductions in place. Single column reads best.
#import "/usfm.typ": *
#import "/usfm.typ" as base

#let tint = rgb("#f3efe6")
#let rule-color = rgb("#b9ad94")

#let md-h(level, body) = block(above: 0.8em, below: 0.3em, sticky: true,
  text(weight: "bold", size: if level == 1 { 1.15em } else if level == 2 { 1.05em } else { 1em }, body))
#let md-p(body) = par(justify: false, first-line-indent: 0pt, body)
#let md-li(body) = pad(left: 1em, [• ] + body)
#let md-quote(body) = pad(left: 1.2em, text(style: "italic", body))

// One note: its translation-issue tag, then the note.
#let note(support, body) = block(above: 0.35em, below: 0.35em, {
  if support != "" {
    text(size: 0.85em, fill: rgb("#6b5d3f"), smallcaps(support.replace("figs-", "").replace("translate-", "").replace("-", " ")))
    h(0.5em)
  }
  body
})

// The notes for one verse, in a tinted box.
#let notes(verse, body) = block(width: 100%, fill: tint, inset: (x: 0.8em, y: 0.5em), radius: 2pt,
  stroke: (left: 1.5pt + rule-color), above: 0.5em, below: 0.7em, breakable: true, {
    set text(size: s.note_size * 1pt + 0.5pt)
    set par(justify: false, first-line-indent: 0pt, leading: 0.45em)
    text(weight: "bold", fill: rgb("#6b5d3f"), verse)
    body
  })

// A book or chapter introduction: ruled above and below, full width.
#let intro(body) = block(width: 100%, above: 1em, below: 1em, inset: (y: 0.6em),
  stroke: (top: 0.5pt + rule-color, bottom: 0.5pt + rule-color), breakable: true, {
    set text(size: s.note_size * 1pt + 1pt)
    set par(justify: false, first-line-indent: 0pt)
    body
  })
