# Verse under a heading is a missing paragraph (Will, 2026-08-25)

The case (found driving the editor — the demo doc itself has it):

    \c 1
    \s1 Hidden pieces are not places

    \v 1 Put the caret …

Spec: a line feed closes NOTHING (structure is marker-delimited), so
\s1 stays open across the blank line and the verse nests INSIDE the
heading node — the verse text is structurally section-heading text.
usfm.org requires a paragraph marker after a section heading to
re-establish context for verse text.

Today missing-paragraph is SILENT here: lint/ancestry.rs counts any
MarkerKind::Paragraph node as "a paragraph above", and \s1 is
paragraph-kind (heading-ness is Category::ParaTitlesSections, never
consulted by the counter).

PROPOSED FIX: the counter counts only VERSE-BEARING categories as
satisfying "paragraph above" — body paragraphs, poetry, lists, table
cells; ParaTitlesSections (and ParaIntroductions?) do not count. No
new code: the existing missing-paragraph fires with its existing
\p-before-the-verse fix, which is the exact repair.

NEEDS WILL: the excluded set — titles/sections + introductions, or
titles/sections only? Corner: \d (Psalm descriptive title) is in the
titles category — corpus PSA always has \q between \d and \v 1 so it
likely never fires, but measure; same watch for \sp (drama).

Corpus movement unknown — measure and re-pin as part of the pass.
Queue: after pass 13 (designator gate), alongside/before
format_edits_in.

UPDATE (2026-08-25, from Will's railroad read): the spec's grammar
itself encodes the split — paragraphs are enumerated as
VersePara.para.style.enum (may contain Verse in their content loop)
vs OtherPara.para.style.enum (may not). So the excluded set should be
DERIVED from the spec's OtherPara enum via the spec-diff referee
(tcdocs clone), not authored from Category judgment. A \v inside an
OtherPara style is a grammar violation in the spec's own terms; the
\d question answers itself by which enum \d sits in.

RESOLVED (Will, 2026-08-25, from docs.usfm.bible/usfm/3.1/cv/v.html):
\v is Valid In body paragraphs + poetry + List + Table — "the para is
the more determinative part" (ChapterContent membership does not
weaken the missing-paragraph advisory for bare \v after \c).

AND THE MECHANISM ALREADY EXISTS: schema::V_FORBIDDEN_IN_PARAGRAPHS
(spec-diff-mediated, 9 confirmation rounds, sourced from usx.rng's
OtherPara/SectionPara enums) → generated::v_forbidden(idx), currently
CONSUMERLESS. The pass = lint/ancestry.rs's para predicate becomes
`MarkerKind::Paragraph && !v_forbidden(marker_idx)` (TableCell
unchanged) + corpus measurement + re-pin. Zero authored lists, zero
judgment.

CORNER flagged to Will: his pasted poetry list includes \qa as
verse-valid; the table (usx.rng OtherPara:1151) lists qa v-FORBIDDEN.
Lean: table wins (qa is an acrostic HEADING, same disease as \s1);
the referee law says rng over prose page. Awaiting his nod.

\qa CORNER RULED (Will, 2026-08-25): the table wins — \qa is an
acrostic HEADING, verses do not go in it. V_FORBIDDEN membership
stands as baked. No open questions remain; ready to build.
