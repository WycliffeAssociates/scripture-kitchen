// Recipe: links. Contents page, PDF bookmarks, clickable cross references.
#import "/usfm.typ": *
#import "/usfm.typ" as base

#let link-color = rgb("#1d4f91")

// Every verse leaves an anchor named BOOK-C-V.
#let v(b, c, n) = [#metadata(none)#label(b + "-" + c + "-" + n)] + base.v(b, c, n)

// A book is a level-1 heading (contents and bookmarks) that takes no space;
// the visible title is still \mt's.
#let book(code, name) = {
  base.book(code, name)
  place(hide(heading(level: 1, outlined: true, bookmarked: true, numbering: none, name)))
}

// Section headings become level-2 bookmarks, not contents entries.
#let para(m, lvl, c: none, body) = {
  if m == "s" and calc.max(lvl, 1) == 1 and s.headings {
    place(hide(heading(level: 2, outlined: false, bookmarked: true, numbering: none, body)))
  }
  base.para(m, lvl, c: c, body)
}

// A cross reference links to its verse if the verse is in this document.
#let xt(target, body) = context {
  if query(label(target)).len() > 0 {
    link(label(target), text(fill: link-color, body))
  } else { body }
}

#let contents() = {
  page(columns: 1, header: none, footer: none, background: none, {
    align(center, text(size: 18pt, weight: "bold", [Contents]))
    std.v(1em)
    columns(2, outline(title: none, depth: 1))
  })
}
