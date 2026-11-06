// Recipe: study. Notes for the verses on a page, set below a rule at the foot
// of the page in small type. Each note is a footnote with no caller.
#import "/usfm.typ": *
#import "/usfm.typ" as base

#let study-size = s.at("study_size", default: 7.3)

// A study note's numbering is a function (it prints nothing); a textual
// footnote's is a pattern string. That is how the entry rule tells them apart.
#let is-study(note) = type(note.numbering) == function

#let setup(body) = {
  show: base.setup
  set footnote.entry(separator: line(length: 100%, stroke: 0.5pt + luma(40%)), clearance: 0.9em, gap: 0.3em, indent: 0pt)
  show footnote.entry: it => if is-study(it.note) {
    set text(size: study-size * 1pt)
    set par(justify: true, first-line-indent: 0pt, hanging-indent: 0pt, leading: 0.38em)
    it.note.body
  } else { it }
  body
}

// One verse's notes. The footnote counter is stepped back afterwards so the
// textual notes keep their letters.
#let sn(ref, body) = {
  footnote(numbering: (..n) => [], [#text(weight: "bold", fill: rgb(s.verse_color), ref)#h(0.4em)#body])
  counter(footnote).update(n => calc.max(n - 1, 0))
}

// Markdown pieces, flattened into running prose.
#let md-h(level, body) = strong(body) + [ ]
#let md-p(body) = body + [ ]
#let md-li(body) = [• ] + body + [ ]
#let md-quote(body) = emph(body) + [ ]
