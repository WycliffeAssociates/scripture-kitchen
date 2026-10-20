# python3 testData/resolve/gen_cases.py > testData/resolve/cases.json
import json, sys

def at(s, needle, n=0):
    b = s.encode()
    nb = needle.encode()
    i = -1
    for _ in range(n + 1):
        i = b.index(nb, i + 1)
    return i

def L(s): return len(s.encode())
def P(c, v, seg=None):
    p = {"chapter": c, "verse": v}
    if seg: p["segment"] = seg
    return p
def V(a, b=None): return {"kind": "verses", "from": a, "to": b or a}
def C(a, b=None): return {"kind": "chapters", "from": a, "to": a if b is None else b}
def found(f, t, covers, opening=None, coarser=False):
    return {"found": {"from": f, "to": t, "opening": f if opening is None else opening, "covers": covers, "coarser": coarser}}
def missing(what, within=None):
    m = {"missing": what}
    if within is not None: m["within"] = {"from": within[0], "to": within[1]}
    return m

ID = "\\id GEN\n"
cases = []
def case(name, usfm, address, expect): cases.append({"name": name, "usfm": usfm, "address": address, "expect": expect})

A = "\\id GEN\n\\c 1\n\\p\n\\v 1 In the beginning.\n\\v 2 The earth.\n\\c 2\n\\p\n\\v 1 Thus.\n"
case("the book is the whole text", A, {"kind": "book"}, found(0, L(A), {"kind": "book"}))

s = "\\id GEN\n\\mt1 Genesis\n\\c 1\n\\p\n\\v 1 a\n"
case("the intro is the front matter", s, {"kind": "intro"}, found(0, at(s, "\\c 1"), {"kind": "intro"}))

s = "\\c 1\n\\p\n\\v 1 a\n"
case("an empty intro is missing", s, {"kind": "intro"}, missing("intro"))

case("one chapter runs to the next", A, C(2), found(at(A, "\\c 2"), L(A), C(2)))

s = "\\id GEN\n\\c 1\n\\p a\n\\c 2\n\\p b\n\\c 3\n\\p c\n"
case("a chapter range spans its rows", s, C(1, 2), found(at(s, "\\c 1"), at(s, "\\c 3"), C(1, 2)))

s = ID + "\\c 3\n\\p a\n\\c 3\n\\p b\n"
f, g = at(s, "\\c 3"), at(s, "\\c 3", 1)
case("two chapter 3s are ambiguous", s, C(3),
     {"ambiguous": [{"from": f, "to": g, "opening": f}, {"from": g, "to": L(s), "opening": g}]})

case("a chapter the text lacks is missing", A, C(30), missing("chapter"))

case("a verse runs to the next verse or chapter", A, V(P(1, 2)),
     found(at(A, "\\v 2"), at(A, "\\c 2"), V(P(1, 2))))

s = "\\id GEN\n\\c 1\n\\p\n\\v 1 a\n\\v 2 b\n\\v 3 c\n"
case("a verse range spans both ends", s, V(P(1, 1), P(1, 2)),
     found(at(s, "\\v 1"), at(s, "\\v 3"), V(P(1, 1), P(1, 2)), opening=at(s, "\\p")))

case("a range may cross a chapter", A, V(P(1, 2), P(2, 1)),
     found(at(A, "\\v 2"), L(A), V(P(1, 2), P(2, 1))))

s = ID + "\\c 1\n\\p\n\\v 1-2 a b\n\\v 3 c\n"
case("a verse in a bridge covers the bridge", s, V(P(1, 2)),
     found(at(s, "\\v 1-2"), at(s, "\\v 3"), V(P(1, 1), P(1, 2)), opening=at(s, "\\p")))

s = ID + "\\c 1\n\\p\n\\v 1,3 a\n\\v 4 b\n"
case("a verse a list skips is missing", s, V(P(1, 2)), missing("verse", (at(s, "\\c 1"), L(s))))

case("a verse a list names covers its hull", s, V(P(1, 3)),
     found(at(s, "\\v 1,3"), at(s, "\\v 4"), V(P(1, 1), P(1, 3)), opening=at(s, "\\p")))

s = ID + "\\c 1\n\\p\n\\v 3 a\n"
case("a segment the text does not divide is found coarser", s, V(P(1, 3, "a")),
     found(at(s, "\\v 3"), L(s), V(P(1, 3)), opening=at(s, "\\p"), coarser=True))

s = ID + "\\c 1\n\\p\n\\v 3a a\n\\v 3b b\n\\v 4 c\n"
case("a whole verse the text divides spans its segments", s, V(P(1, 3)),
     found(at(s, "\\v 3a"), at(s, "\\v 4"), V(P(1, 3, "a"), P(1, 3, "b")), opening=at(s, "\\p")))
case("a segment the text divides is exact", s, V(P(1, 3, "b")),
     found(at(s, "\\v 3b"), at(s, "\\v 4"), V(P(1, 3, "b"))))

s = ID + "\\c 1\n\\p\n\\v 3 a\n\\v 3 b\n"
f, g = at(s, "\\v 3"), at(s, "\\v 3", 1)
case("two anchors for one verse are ambiguous", s, V(P(1, 3)),
     {"ambiguous": [{"from": f, "to": g, "opening": at(s, "\\p")}, {"from": g, "to": L(s), "opening": g}]})

case("a missing verse names its chapter", A, V(P(1, 9)), missing("verse", (at(A, "\\c 1"), at(A, "\\c 2"))))

case("a range with a missing end is missing", A, V(P(1, 1), P(1, 9)), missing("verse", (at(A, "\\c 1"), at(A, "\\c 2"))))

case("a verse in a missing chapter is a missing chapter", A, V(P(7, 1)), missing("chapter"))

s = ID + "\\c 1\n\\p\n\\v 1 a\n\\p\n\\v 2 b\n"
case("opening: a verse after a paragraph opens at the paragraph", s, V(P(1, 2)),
     found(at(s, "\\v 2"), L(s), V(P(1, 2)), opening=at(s, "\\p", 1)))

s = ID + "\\c 1\n\\p\n\\v 1 a\n\\s1 A heading\n\\p\n\\v 2 b\n"
case("opening: a heading and its text open the verse", s, V(P(1, 2)),
     found(at(s, "\\v 2"), L(s), V(P(1, 2)), opening=at(s, "\\s1")))

s = ID + "\\c 1\n\\q1 \\v 1 line one\n\\q2 line two\n\\q1 \\v 2 line three\n"
case("opening: a poetry line opens the verse it starts", s, V(P(1, 2)),
     found(at(s, "\\v 2"), L(s), V(P(1, 2)), opening=at(s, "\\q1 \\v 2")))

s = ID + "\\c 1\n\\q1 \\v 1 a\n\\b\n\\q1 \\v 2 b\n"
case("opening: a blank line joins the run", s, V(P(1, 2)),
     found(at(s, "\\v 2"), L(s), V(P(1, 2)), opening=at(s, "\\b")))

s = ID + "\\c 1\n\\p \\v 1 a \\v 2 b\n"
case("opening: mid-paragraph the opening is the verse", s, V(P(1, 2)),
     found(at(s, "\\v 2"), L(s), V(P(1, 2))))

s = "\\id GEN\n\\c 1\n\\p\n\\v 1 a\n"
case("opening: the first verse opens after its chapter marker", s, V(P(1, 1)),
     found(at(s, "\\v 1"), L(s), V(P(1, 1)), opening=at(s, "\\p")))

s = ID + "\\c 1\n\\cl Psalm 1\n\\p\n\\v 1 a\n"
case("opening: a chapter label is not an opener", s, V(P(1, 1)),
     found(at(s, "\\v 1"), L(s), V(P(1, 1)), opening=at(s, "\\p")))

s = ID + "\\c 3\n\\d A psalm of David.\n\\q1 \\v 1 a\n"
case("opening: a psalm title is an opener", s, V(P(3, 1)),
     found(at(s, "\\v 1"), L(s), V(P(3, 1)), opening=at(s, "\\d")))

s = ID + "\\c 1\n\\s1 Title\n\\p\n\\v 1 a\n\\p\n\\v 2 b\n"
case("opening: a range opens where its first verse does", s, V(P(1, 1), P(1, 2)),
     found(at(s, "\\v 1"), L(s), V(P(1, 1), P(1, 2)), opening=at(s, "\\s1")))

s = ID + "\\c 1\n\\s1 Ἐν ἀρχῇ\n\\p\n\\v 1 ἦν ὁ λόγος\n\\v 2 οὗτος\n"
case("non-ASCII text: offsets are bytes", s, V(P(1, 2)),
     found(at(s, "\\v 2"), L(s), V(P(1, 2))))
case("non-ASCII text: a heading opening", s, V(P(1, 1)),
     found(at(s, "\\v 1"), at(s, "\\v 2"), V(P(1, 1)), opening=at(s, "\\s1")))

s = "\\id PSA\n\\c 3\n\\cl Psalm\n\\d \\zaln-s |x-strong=\"H4210\"\\*\\w A|x-occurrence=\"1\"\\w*\\zaln-e\\* psalm.\n\\q1 \\v 1 a\n"
case("opening: a milestone inside a heading is transparent", s, V(P(3, 1)),
     found(at(s, "\\v 1"), L(s), V(P(3, 1)), opening=at(s, "\\d")))

s = ID + "\\c 1\n\\p\n\\v 1 a\n\\s1 Head\n\\ts\\*\n\\p\n\\v 2 b\n"
case("opening: a milestone between heading and paragraph is transparent", s, V(P(1, 2)),
     found(at(s, "\\v 2"), L(s), V(P(1, 2)), opening=at(s, "\\s1")))

s = "\\id PSA\n\\c 5\n\\cl Psalm\n\\d For the director.\n\n\\ts\\*\n\\q1 \\v 1 a\n"
case("opening: a milestone after a psalm title is transparent", s, V(P(5, 1)),
     found(at(s, "\\v 1"), L(s), V(P(5, 1)), opening=at(s, "\\d")))

s = "\\id PSA\n\\c 1\n\\ms Book One\n\\cl Psalm\n\\q1 \\v 1 a\n"
case("opening: a chapter label between openers is transparent", s, V(P(1, 1)),
     found(at(s, "\\v 1"), L(s), V(P(1, 1)), opening=at(s, "\\ms")))

s = ID + "\\c 1\n\\s1 😀 Head\n\\p\n\\v 1 a 𝔄\n\\v 2 b\n"
case("astral text: UTF-16 counts two units per scalar", s, V(P(1, 1), P(1, 2)),
     found(at(s, "\\v 1"), L(s), V(P(1, 1), P(1, 2)), opening=at(s, "\\s1")))

s = "\\id GEN\n\\c 1\n\\p\n\\v 1 a\n\\v 2 b\n\\v 3 c\n"
case("a reversed range spans the same verses", s, V(P(1, 2), P(1, 1)),
     found(at(s, "\\v 1"), at(s, "\\v 3"), V(P(1, 1), P(1, 2)), opening=at(s, "\\p")))

s = ID + "\\c 3\n\\p\n\\v 1 a\n\\c 3\n\\p\n\\v 1 b\n"
f, g = at(s, "\\c 3"), at(s, "\\c 3", 1)
case("a verse in a repeated chapter is an ambiguous chapter", s, V(P(3, 1)),
     {"ambiguous": [{"from": f, "to": g, "opening": f}, {"from": g, "to": L(s), "opening": g}]})

s = ID + "\\c 1\n\\p\n\\v 1,3 a\n\\v 4 b\n"
case("a range ending in a list gap is missing", s, V(P(1, 1), P(1, 2)),
     missing("verse", (at(s, "\\c 1"), L(s))))
case("a range checks only its ends", s, V(P(1, 1), P(1, 4)),
     found(at(s, "\\v 1,3"), L(s), V(P(1, 1), P(1, 4)), opening=at(s, "\\p")))

s = ID + "\\c 1\n\\p\n\\v 3a a\n\\v 3 b\n\\v 4 c\n"
case("a part and a whole for one verse are ambiguous, in source order", s, V(P(1, 3)),
     {"ambiguous": [{"from": at(s, "\\v 3a"), "to": at(s, "\\v 3 "), "opening": at(s, "\\p")},
                    {"from": at(s, "\\v 3 "), "to": at(s, "\\v 4"), "opening": at(s, "\\v 3 ")}]})

s = ID + "\\c 1\n\\p\n\\v 1,3,5 a\n\\v 2,4 b\n"
case("lists that interleave cover their joint hull", s, V(P(1, 1), P(1, 2)),
     found(at(s, "\\v 1,3,5"), L(s), V(P(1, 1), P(1, 5)), opening=at(s, "\\p")))

s = ID + "\\c 1\n\\p\n\\v 3a a\n\\v 3a-3b b\n\\v 4 c\n"
case("overlapping segments for a whole verse are ambiguous", s, V(P(1, 3)),
     {"ambiguous": [{"from": at(s, "\\v 3a "), "to": at(s, "\\v 3a-3b"), "opening": at(s, "\\p")},
                    {"from": at(s, "\\v 3a-3b"), "to": at(s, "\\v 4"), "opening": at(s, "\\v 3a-3b")}]})

s = ID + "\\c 1\n\\p\n\\v 2-3a a\n\\v 3a b\n\\v 3b c\n"
amb = {"ambiguous": [{"from": at(s, "\\v 2-3a"), "to": at(s, "\\v 3a "), "opening": at(s, "\\p")},
                     {"from": at(s, "\\v 3a "), "to": at(s, "\\v 3b"), "opening": at(s, "\\v 3a ")},
                     {"from": at(s, "\\v 3b"), "to": L(s), "opening": at(s, "\\v 3b")}]}
case("a bridge into a segment overlaps that segment", s, V(P(1, 3)), amb)
case("a range ending on an overlapped verse is ambiguous", s, V(P(1, 2), P(1, 3)), amb)

def d(v): return json.dumps(v, ensure_ascii=False, separators=(", ", ": "))
out = []
for c in cases:
    out.append("  {\n" + ",\n".join(f"    {d(k)}: {d(c[k])}" for k in ("name", "usfm", "address", "expect")) + "\n  }")
sys.stdout.write("[\n" + ",\n".join(out) + "\n]\n")
