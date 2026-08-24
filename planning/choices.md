# Choices ledger

## format design (2026-08-24, pre-implementation — ruled with Will directly)

Givens Will ruled in-session, not audit items (full rationale in
planning/sketches/format.md):

- The Form channel: severity grows an explicit 5th variant (Form) —
  never `severity: None` overloaded; `lint()` skips Form rows entirely,
  `format_edits()` evaluates them. Dual citizens (missing-paragraph,
  empty-paragraph) keep real severity + a `formatter` bit.
- format_edits() = call lint() internally (harvest formatter-bit +
  repairs-allowlist fixes) + a Form-row pass; merge, one transaction.
  No "lint first" caller protocol.
- Format is opt-in and MAY mutate/delete/invent bytes — the mask's
  no-normalization law does not bind it. remove-marker (`\s5`) is a
  parameterized row, not a third lane.
- Options over profiles: axes verse_breaks Keep|Remove (default
  Remove), char_marker_breaks Keep|Join, newline Lf|CrLf (default Lf,
  NORMALIZES existing endings too); params remove_markers, repairs.
- Poetry NOT special-cased — `\q#` is block-like, derived from the
  tables codegen category ONLY (no authored list, no onion-set
  comparison: evolution, not port).
- Interior verse-text ws never touched; edges trim. Renumbering never
  formats. `\p\p`→`\p` yes, `\m\p` no (ambiguous). missing-paragraph
  fix = `\p` on its own line before the first `\v`. dedupe-verse-number
  + bridge-empty-verses exist as default-FALSE opt-ins. No
  remove-marker safety rail (lint on output is the rail).
- Not ported: RemoveOrphanEmptyVerse, RemoveBridgeVerseEnumerators,
  MoveChapterLabel (positional `\cl` semantics), RecoverMalformedMarkers
  (moot — re-lexing makes the state unrepresentable).

Every decision made on Will's behalf where the spec was silent — surfaced,
judged, banked. Banked is settled: entries here are givens for later passes.
Entries keep their walked scenario forever (a headline-only entry has
failed). Verdicts: sound / unsound / needs-user, each with confidence that
Will would have made the same call.

## masks-toc design (2026-08-21, pre-implementation — presented to Will)

Givens Will ruled directly, not audit items: two artifacts on two walks;
config-only Filter, constructor recipes, marker>kind, VerseExtent; marker
names at the boundary; fixed-width #[repr(C)] as internal hygiene, wire
deferred; book code copied as-is; ebible `<range>` bridges; trim as a
vref-renderer option only; Toc subsumes ParseHeader; utf16 index promoted
from experiments and built in this window.

1. **from_source(dropped byte) = None, never clamp.** RULED SOUND
   (Will, 2026-08-21). A diagnostic on a `\f` marker byte asked "where in
   the proofread text?" — the footnote isn't in that text, so the honest
   answer is nowhere (None). Clamping always answers but lies. Companion
   CONVENTION ruled in the same exchange: **public offsets are always
   SOURCE bytes** — a mask's consumer (sous) converts via to_source
   BEFORE emitting any diagnostic, so mask space never escapes its maker
   and downstream tools speak one space. from_source only appears in the
   reverse "show a source diagnostic inside the masked view" direction,
   where None honestly means "not in this view."
2. **Front matter is "chapter 0".** RULED SOUND (Will): onion returns 0 /
   the book slug; a frontend may localize it as "front matter /
   introduction" — that naming is the client's, never the engine's.
   Chapter rows tile the file so locate() is total; locate() in row 0
   renders book-only ("GEN"), never a fake verse.
3. **Widths: chapter/verse u16, offsets u32.** Real maxima ~150/~176, so
   absurd headroom — but if the layout ever crosses a wire these freeze.
   Verdict: sound. Confidence: high.
4. RULED SOUND (Will). **Malformed `\v` designator still gets a row** — anchored at its token,
   first/last = 0, sid renders ugly; skipping the row would let the next
   verse's extent silently swallow this one's text. Degrade, never repair
   (USX's ruled behavior, copied here without a ruling). Verdict: sound.
   Confidence: medium.
5. **locate() in a bridge reports the whole bridge** ("MRK 6:1-3"). RULED
   (Will): keep the full range. The Sid carries first..last; ebible's
   first-verse keying stays a renderer concern.
6. **structure() keeps newline tokens verbatim** — dropped text leaves its
   line breaks, so the scaffold has blank lines where prose was.
   PENDING WILL'S EYEBALL of the pass-3 debug/ dumps; provisional stands
   (verbatim — collapsing blank runs is trimming, ruled invention).
7. **verses() allocates one String per verse** — a renderer at the blessed
   serialization boundary; zero-alloc consumers use Mask::iter underneath.
   Verdict: sound. Confidence: high.
8. **VerseAnchor spends 4 bytes on a token index** so raw designator
   spelling ("6a") stays reachable with no stored strings (~5 KB/book).
   Verdict: sound. Confidence: high.

## pass 1 — Toc (2026-08-21, audited from the implementer's self-report)

1. **lint::header_scan NOT migrated — the one overridden instruction, judged
   SOUND — Will confirmed.** The delegation listed it as a ParseHeader consumer; it never was —
   it's an independent scan for (BookCode, declared \usfm version) bounded at
   the first \c. Re-homing it would put a version field on Toc (not its job)
   or cost lint a whole-stream pass for a header fact. Confidence: medium
   (flagged to Will because it declined an instruction).
2. **Chapter raw labels — REVISED BY WILL, applied.** ChapterRow grew
   12→16 B with a `token: u32` (u32::MAX for the front-matter row) plus a
   `designator_span` helper matching VerseAnchor's, because a chapter
   NAVIGATION GRID (1 2 … 12b) needs the raw label first-class. The grid
   itself, and plural `chapter_spans(n)` knowing "this is the second 4"
   for duplicated numbers, are NOT built — investigate-later material.
3. **Missing book code renders "###"** — RULED (Will chose ### over the
   implementer's ???); applied, one const.
4. RULED SOUND (Will — "yes if that means the \id", it does: the BookCode
   token of \id MAT, which is what makes "MAT 6:4" renderable).
   **Toc carries book_token: Option<u32>** beyond the sketch's fields —
   distinguishes "no \id" from NUL-byte codes, keeps the full spelling and
   lint's anchor reachable; without it the subsumption regressed capability.
   Sound, high.
5. **Pre-first-verse / malformed-designator bytes render "GEN 3", not
   "GEN 3:0"** — first==0 means "no verse known"; :0 would name a verse that
   doesn't exist. Data still distinguishes the two cases. RULED SOUND.
6. **chapter_span(n) answers the FIRST run of a duplicated chapter number**
   (bdf_reg really has one); a chapter_spans(n) iterator is the escape hatch.
   RULED SANE (Will): lint flags duplicates at full-document runs, which also
   rebuild the Toc.
7. **Lint reconciliation by pinned numbers + naming comment**, not re-running
   lint inside the toc oracle (which would triple its cost). Sound, medium.
8. One-line sound discretion: chapter_span returns Option (0..0 would be
   ambiguous); row 0 always emitted even zero-width; byte-only tiling (old
   row-ranges adapter had no consumer); u16 saturation continues designator's
   "ordered junk" rule; VerseAnchor.token = the \v MARKER's index (always
   exists, at==tokens[token].start checkable) + designator_span helper; pub
   fields; --toc-trace writes stdout, caller redirects; toc() free fn
   re-exported at crate root matching lint/usj/usx shape.

## pass 2 — Utf16Index (2026-08-21, audited from the implementer's self-report)

1. **Interior offsets snap DOWN, both directions** — a byte inside a
   multi-byte char answers that char's unit offset; a utf16 offset inside a
   surrogate pair answers the char's first byte. The SWAR formula's free
   behavior was snap-UP; a ≤3-step back-up loop buys symmetry so a garbage
   offset stays inside the character it touches. Total, no asserts
   (Toc::locate's spirit). Sound, medium-high.
2. **Both `utf16_index()` and `Utf16Index::new` exist** (one delegates) —
   crate free-fn convention vs natural borrowing-type reading. Mild
   duplication. RULED SOUND (Will), keep both.
3. One-line sound discretion: &[u8] in, UTF-8 by contract (matches toc);
   u32 offsets (banked widths); top-level src/utf16.rs (depends on
   nothing); to_utf16/to_byte/len_utf16 names; utf16_len + STRIDE public
   (playground prices them); index_bytes() kept for the size invariant;
   index length len/256+1 (byte==len needs an entry); private strides, no
   repr(C) (only scalars cross); tests/utf16_oracle.rs exhaustive
   (107.5M boundaries, 227 files incl. Hindi) replacing the experiment's
   every-1000th sampling; experiments/utf16.rs = record only, one
   implementation, playground --utf16 exercises the promoted module.

## pass 3 — Mask + Filter (2026-08-21, audited from the implementer's self-report)

1. **NEEDS-USER: aligned-corpus readability.** Word-aligned USFM (en_ult)
   puts every \w on its own line, so verse_text with newlines-verbatim is
   one-word-per-line and newlines:false smushes (the newline IS the word
   separator there). A pretty view needs newline→space substitution, which
   breaks text()[i]==source[to_source(i)] — normalization, ruled invention.
   PROVISIONAL: accept as-is — the mask is an analysis input (sous treats
   \n as whitespace; detection unaffected); a human-readable aligned VIEW
   is a renderer concern if ever wanted (newline-as-space is same-width, so
   the map survives — investigate-later). en_ulb/bsb read clean both
   recipes. RULED: leave as-is (Will).
2. **NEEDS-USER: the Keep gap.** Keep = "marker survives, children still
   filtered" (the compositional meaning structure() requires). Consequence:
   markers:[("f",Keep)] under text:None yields an empty \f +\f* shell —
   "structure + real footnote text" is INEXPRESSIBLE with the pinned
   fields. RULED INTENDED (Will): footnote text needs translation anyway,
   and character markers wrap words that aren't portable across languages —
   chars default opted-out, keep:("f"/chars) is the aligner's escape hatch.
   No KeepAll.
3. Sound: a dropped payload's DELIMITER whitespace drops with it (one
   horizontal run, never a newline) — delimiter accounting, not trimming;
   it is what makes "\v 1 Jesus" mask to "Jesus". High.
4. Sound: unknown marker PANICS in mask(), Filter::resolve() -> Result is
   the boundary pre-flight (two doors, one implementation). High.
5. Sound one-liners: ranges merge to maximal runs (asserted corpus-wide);
   ~ survives verbatim (a Text byte, no normalization); // takes the
   Paragraph action (verse_text unwraps, structure keeps); Unwrap drops
   the node's closer + attr list (recognized as last child, no side
   table); \b kept by structure (stanza breaks survive, visible in dumps);
   VerseExtent mirrors usx::decorate exactly (\v opens even malformed, \c/
   EOF close, sidebars looked-away-from, \v inside a Removed subtree never
   opens); unnamed kinds get Figure/Meta=Remove, cells Unwrap/Keep,
   Periph Unwrap/Keep, Header Remove/Keep; kinds[Unknown] never read
   (unknowns field is row 0's authority); U25003 containers are
   transparent and un-Removable (share tokens with their points, detected
   structurally); marker names resolve in the spelling written (-s/-e →
   MilestoneOnly); empty mask total; pub fields per Toc precedent;
   MarkerKind::COUNT=14 pinned by const assert.

## interposed pass — designator delimiter fold (2026-08-21, audited)

1. **Net ADDITION (+157/−50), metric honestly missed.** Only mask's
   designator-seam trim deleted (verse_text byte-identical = the proof);
   five consumers gained trim-at-read. Cause: the NEWLINE delimiter keeps
   every export seam rule alive — the fold moves horizontal runs only. The
   buy is grammar-consistency + better structure() bytes, not code.
2. **Nine mask assertions re-pinned by one space** — kept designators now
   carry their delimiter (`\v 1 \f` not glued `\v 1\f`, itself a lint
   shape); matches structure()'s documented promise. One-line revert
   exists. Sound, medium-high.
3. **NoteCaller/BookCode NOT folded** — ruling named the designator;
   folding them is a second behavior change. The `\f + ` grammar argument
   is nearly as strong — follow-up candidate, not smuggled. Sound, high.
   OVERTURNED by the completion pass below.
4. Sound: designator::label() added, verse/chapter step the run (else
   every corpus verse is Malformed — tested); lint renumber splices the
   label not the span (would have written `\v 2a`; check_fixes proves);
   toc designator_span gained a source param (delimiter length isn't in
   the Token; the method exists to hand back a raw LABEL); usj/usx/html
   at_boundary kept by PROBED deletion (guards the newline-delimiter
   case); \ca/\cp/\va/\vp fold by shared payload path; `\v 1 //x` emits
   one fewer token (no consumer noticed, 629 fixtures agree).
5. Perf: en_ulb +2.2%, en_ult −1.5% (consistent, unexplained by the work,
   reads as layout noise; under the 2% stop threshold).

## completion pass — caller + book-code delimiter fold

1. **The fold is now unconditional in `text_arm`**: the railroad makes the
   horizontal delimiter after a caller (`/[^\\\s]+/` then `' '|HS`) and
   after a book code (code then `' '|TAGEND`) required grammar, the same
   status the designator's has. One `ws_run_end` for all three kinds; the
   `match kind` the designator pass introduced is gone. Sound, high.
2. **mask's `delimits_payload` DELETED** (−18 lines in mask.rs, a real net
   deletion): it existed only to strip the caller/book-code delimiter off
   the head of the following Text, which the scanner now never leaves
   there. verse_text's mask-oracle bytes are byte-identical — the proof
   the fold replaces the mechanism exactly. Sound, high.
3. **One mask unit pin re-pinned by one space** (`\f +\f*` → `\f + \f*`),
   the same class as the designator pass's nine: a KEPT caller now brings
   its delimiter, as a kept marker does. Sound, medium-high.
4. **`scanner::payload_label` is the one implementation** of the fold's
   inverse, and `designator::label` delegates to it — one trim, one doc,
   designator.rs's public surface untouched. `pub(crate)`, because
   `scanner` is a private module; external readers still have
   `designator::label`. Sound, medium.
5. Perf: max-of-8, en_ulb +35.8%, en_ult +2.2% — no regression. The ulb
   figure is a 4.5MB corpus and reads as noise plus one fewer token per
   note; the honest claim is "not slower". Medium.

## fold completion — caller + book code (2026-08-21, audited)

1. Sound: fold unconditional at the ONE carve site (three-way match was
   dead weight); newline-after never folds (rule 1); EOF no-op; runs fold
   whole (tabs included); degraded \id copies min(3) of the LABEL so no
   space enters book[3].
2. Sound-critical catches: lint's is_book_code would have fired
   BookCodeUnknown on EVERY corpus book untrimmed; CallerShape would
   false-fire on 3-byte callers; BookCodeNotUppercase's splice narrowed to
   the label (would have eaten the delimiter — caught by reasoning, corpus
   has no lowercase \id, unit pin covers the shape).
3. **NEEDS-USER lite: no public label helper for caller/book-code bytes**
   — scanner::payload_label is pub(crate) (the scanner module is private);
   designator::label stays the only public one. A client reading raw
   caller/code token bytes must trim itself. Export a public helper if
   that client ever exists. Confidence: medium.
4. Sound: exports needed NOTHING (payload_child already trims — why
   187/195/434 held); header_scan reads indices not bytes; one mask pin
   moved (\f + \f* — a kept caller brings its delimiter like a kept
   marker); whole pass ≈ +48 lines with mask.rs -15 — one rule now, two
   mechanisms before.

## pass 4 — vref + chain test (2026-08-21, audited; tree left uncommitted for Monday)

1. **NEEDS-USER: verses() substitutes \n→space and \t→space in verse text,
   one byte for one byte** — the FORMAT forces it (a line-per-verse file
   cannot hold a line break; the tab-joined column cannot hold its own
   separator — en_ult PSA 55:8 really contains a tab, corpus-test-caught).
   The one substitution beyond trim; same-width, renderer-level (the ruled
   home for normalization), incidentally fixes aligned-corpus one-word-
   per-line. Audit: sound; awaiting Will's bless. Medium-high.
2. Malformed verse (first==0) KEEPS its line with a chapter-only key
   ("ZEC 1\ttext", no colon) — dropping loses text, :0 names a nonexistent
   verse; an ebible aligner must notice colon-less keys (2 in the corpus).
   Sound, medium.
3. Bridge lines: ONE extent, all text on line 1, literal <range> on
   covered lines; each LINE's Sid names one verse while Toc::locate on a
   byte still reports the full range (banked #5 untouched — keying is the
   renderer's). Sound, high / medium-high.
4. Sound one-liners: verses(toc, mask, source) — the mask stays the
   caller's choice of view (no internal recipe hard-coding); only
   verses/Verses re-exported at root; JOIN = tab; empty extent = empty
   line never a skip (alignment); chapter 0 yields no line; duplicate
   chapters render duplicate keys in source order (dedup = versification
   opinion the engine doesn't own); join separates never terminates;
   forward-only cursor over mask.ranges (one pass per book); --vref /
   --vref-only playground modes. Chain test surfaced + encoded: Newline
   tokens survive verse_text outside extents ("kept ⇒ in a verse" holds
   for TEXT bytes only), and locate's (0,0) degrade matches a direct scan.
5. Inherited by future work: master-vref padding needs a versification
   table nobody owns; per-verse text split inside a bridge is not
   attempted (would need a segmentation opinion).

## pass 5 — format (2026-08-24, implementer's self-report)

Everything the sketch RULED is implemented as ruled. What follows is only
where the sketch was silent and a choice had to be made. Two entries are
NEEDS-USER; the rest are judged sound.

1. **NEEDS-USER: `missing-paragraph`'s run aggregation now ends at a row-0
   marker, and the corpus count goes 2,865 → 5,434.** THE SCENARIO: en_ulb
   JON writes `…least of them.` ␊ `\s5` ␊ `\v 6 Soon the news…`. `\s5` is
   row 0, whose pop-all recovery kills the standing paragraph, so `\v 6`
   really has no paragraph above it — but lint's `run_reported` flag only
   reset at `\c` and at a paragraph OPEN, so the whole post-`\s5` stretch
   was folded into one earlier finding. Format inserts the one `\p` lint
   offered, re-walks, and lint finds a NEW paragraph-less run behind it:
   the invariant "format converges in one pass" fails on 40-odd en_ulb
   books. The one-line fix in `Ancestry::on_leaf` (row 0 resets the run,
   for the same reason `\c` does — the repairing `\p` cannot survive
   either) makes convergence hold. It also removes the confusion the fix
   oracle's own doc comment describes ("PHM: 36 findings before, 36
   after"): the aggregation was hiding repairs that a human clicking
   "fix" would have wanted. This is a LINT behaviour change made for
   format's sake, so it wants Will's eyes. Judged sound; confidence
   medium-high. The alternative was to declare the sketch's headline
   invariant unreachable, which reads worse.
2. **NEEDS-USER: invariant 7 is checked by EXHAUSTION, not proptest.** THE
   SCENARIO: the sketch says "proptest over random FormatOptions". The
   option space is finite and small — 2 verse × 2 char × 2 newline × 2⁹
   switches = 4,096 combinations — so `options_are_total` enumerates ALL
   of them against 12 fixtures (49,152 format calls, ~1 s) and asserts
   each yields a valid, settled transaction. That is a superset of what
   random sampling could prove, and it avoids adding a third-party dev
   dependency to a crate whose only dependency is memchr. If Will wants
   proptest for the INPUT side (random documents rather than random
   options), that is a real gap this does not close. Confidence high on
   the reasoning, medium on the deviation being wanted.
3. **A rewritten line break swallows the horizontal whitespace in front of
   it; a rewritten delimiter at a line end goes away entirely.** THE
   SCENARIO: `\p \v 1 a` under `verse_breaks: Keep` wants a break before
   `\v`. Inserting one at the marker leaves `\p ␊\v 1 a` — a trailing
   space the next pass deletes, so the document needs two passes. Every
   newline-writing rule therefore replaces `[start of the horizontal run,
   here)` rather than inserting at a point, and `delimiter-single` deletes
   its run outright when a Newline follows (`\p   ␊` → `\p␊`, which
   `delimiter-shape` already accepts as a legal spelling). Idempotence
   forced both. Sound, high.
4. **`collapse-blank-lines` keeps the LAST newline of a run, not the
   first.** THE SCENARIO: `…text` ␊␊ `\v 4` with verse breaks removed. If
   the run collapses to its first ending, the surviving byte is one the
   verse-join rule never looked at and the joined break is one this rule
   already deleted — two rules on one span, and a second pass to settle.
   Keeping the last leaves the break that touches what follows, which is
   the one every other rule speaks about, and the two edits are disjoint
   by construction. A run is also read THROUGH whitespace-only text
   (`␊   ␊` is a blank line however it was typed), which is what makes
   `\n   \n` converge in one pass too. Sound, high.
5. **`remove-marker` runs as a PRE-PASS and marks its tokens invisible.**
   THE SCENARIO: en_ulb HEB `…sufferings.` ␊␊ `\s5` ␊ `\v 11 For both…`
   with `remove_markers: ["s5"]`. Judged in document order, the break
   before `\v 11` is followed by an `\s5` — not a verse — so the verse-join
   declines; after the removal it IS followed by the verse, and pass two
   joins it. Removing first and hiding the removed tokens makes every
   later rule see the document the caller asked for. A removal also takes
   the line ending behind it when the marker had the line to itself,
   which is what the sketch's `\s5` ␊ `\p` → `\p` example shows. Sound,
   high.
6. **A harvested lint fix is RE-TARGETED to the options: every `\n` in its
   `FixStr` becomes the configured ending, and `missing-paragraph`'s
   TRAILING break becomes a space under `verse_breaks: Remove`.** THE
   SCENARIO: `\c 1\v 1 Text` with `newline: CrLf`. Lint's fix text is the
   literal `\n\p\n`, written before any formatter existed, so the output
   would carry two LF breaks in a CRLF document — the sketch's "every
   break in the output is `\r\n`, inserted and pre-existing alike" test
   fails. And under `Remove` that fix's trailing break is a VERSE break,
   which the axis owns: leaving it makes `\p` ␊ `\v` survive a Remove pass
   and need a second one. Both rewrites are the OPTIONS speaking about
   bytes lint had no way to know about, not a second opinion on the
   repair. Sound, high.
7. **The Form pass tells `close_run` which verses are about to receive a
   `\p`.** THE SCENARIO: examples.bsb ZEC 12 writes `\s1 heading` ␊ `\d` ␊
   `\v 1 This is the burden…`. `\d` is a Paragraph row, so the sweep's
   cheap "is a paragraph open" state says yes and the break before `\v 1`
   joins — but the `\v` DISPLACED `\d` in the tree, so lint says
   missing-paragraph and format inserts a `\p` into the line it just
   joined, giving `\d \p \v 1`. `harvest` therefore returns the byte
   offsets where a `\p` lands, and a verse in that set keeps its break —
   the `\p` is a block marker and wants the line. The mirror rule: any
   `\v` marker now sets the sweep's paragraph state true, because a verse
   either sits in a paragraph already or gets one in this very
   transaction. Sound, medium-high (it is the one place the Form pass
   depends on lint's answer rather than on the tokens).
8. **`empty-paragraph`'s fix is derived at FINISH time, not at the close
   event.** THE SCENARIO: the repair needs the paragraph that DISPLACED
   the empty one — spelled the same? holding content? — and at
   `on_node_close` that marker has not arrived. Deriving it there also
   diverges the fused experiment, whose oracle demands byte-identical fix
   links (`en_ulb/13-1CH` caught it). `Structure` now records
   `(observation slot, node id)` and resolves them in `finish`, which both
   the staged and the fused paths call with the whole stream in hand —
   the same shape `correct_renumbers` already uses. Sound, high.
9. **The fix is declined for a CHAIN of identical empties.** THE
   SCENARIO: en_ulb ISA writes `\p` ␊ `\p` ␊ `\q1` ␊ `\v 3 …`. Both `\p`s
   are empty and identical, so "delete the first, the second is the same
   marker" holds for the first — but the survivor is empty too, and the
   fix oracle (which judges a fix by its own SITE) sees empty-paragraph
   still firing at that byte. The condition is now "the survivor holds
   content", which is checked over the tokens against the same
   paragraph-displacing kind set the sweep uses. 25 of the corpus's 787
   empty paragraphs qualify; the rest are mixed pairs or chains. Sound,
   medium-high.
10. **Precedence is (position, row order) with ATOMIC claims, plus one
    special case: two insertions at one point are legal, two line BREAKS
    at one point are not.** THE SCENARIO: a truncated `\f` with
    `repairs: [UnclosedNote]` inserts `\f*` exactly where
    `block-marker-own-line` wants the break before the `\c` that
    truncated it. Rejecting the second insert outright leaves `note\f*\c 2`
    for a second pass; allowing both gives `note\f*␊\c 2`, correct in one.
    But `missing-paragraph`'s `\n\p\n` and a block break at the same `\v`
    would give a blank line nobody asked for. So a claim is refused only
    if it would put a second line break at a point that already has one.
    Claims are all-or-nothing because `bridge-empty-verses` is two edits
    and half of it is worse than none. Sound, medium-high.
11. **Sound one-liners.** Block-likeness is an EXHAUSTIVE match on
    `MarkerKind` (Paragraph/Chapter/TableRow/Header/Sidebar/Periph, plus
    Verse under `Keep`), so a new kind cannot arrive unclassified and no
    marker list is authored anywhere; row 0 is NOT block-like (`\s5` is
    `remove-markers`' business, and an unknown marker's row says nothing
    about anything). `designator-ws-single` covers the note caller and the
    book code as well as the designator — the delimiter fold made all
    three the same span shape. `trim-text-edges` yields a line-leading
    indent to `marker-ws-at-line-start` rather than relying on the
    precedence dedup, so both rows have a snippet that fires exactly one
    of them. Trailing whitespace at EOF is treated as trailing whitespace
    at a line end (deleted). A glued `\v` stays glued under `Remove`
    (`a\v 2 b`): the axis DELETES verse breaks, it never invents a
    separator — the char-boundary law's spirit. Non-UTF-8 input yields an
    empty edit list, never a panic and never a guess. `format_edits` takes
    `&[u8]` (lint's contract) and lexes internally, so no caller runs a
    "lex first" protocol. The eleven Form rows carry templates a formatter
    UI could show ("42 line endings normalized"); nothing renders them
    today. `edit::check_edits` was extracted from `check_fixes` so the
    same pre-flight guards both transactions.
12. **Not built, deliberately.** No playground mode for format (the
    sketch names none, and every existing mode exists to price a walk).
    No `Severity::Form` handling in any consumer — there is nothing to
    handle, since no report can carry one, and a test asserts that over
    the corpus. No preset constructors over `FormatOptions`.

## pass 5 review (2026-08-24, Will's rulings on the self-report)

1. Entry 1 (missing-paragraph run aggregation resets at row-0 markers,
   2,865 → 5,434) — RULED CORRECT by Will: "annoying of the en_ulb but
   correct behavior for the lib."
2. Entry 2 (invariant 7 by exhaustion, no proptest dep) — RULED FINE
   for now. hegel-rust noted as a revisit-at-stable candidate for the
   random-documents side (beta today, fails the boring bar).
3. Entry 12's "no playground mode" — OVERTURNED by Will: he wants
   eyeball dumps. `--format-trace <file> [--format-chapter N]
   [--format-variant default|keep-verse-breaks|remove-s5|join-chars]`
   added, mirroring --mask-trace; regenerates debug/formatting/*.txt
   with the options + wall-clock in the header. Whole-book PSA (272KB,
   largest book): 3,971 edits, format_edits ~1.5ms.
4. retarget() overflow no longer silently keeps the un-normalized fix —
   RULED by Will (stack, don't skip): a rewrite that outgrows one
   FixStr now rides adjacent edits at the splice point (first carries
   the replacement span, the rest are pure insertions; apply
   concatenates in order). Unreachable today (max fix is 6 bytes under
   CrLf); unit-pinned via a synthetic 8-break fix.
5. Slow-oracle split (Will's inner-loop concern, 2026-08-24): the 52s
   utf16 exhaustive sweep is now `#[ignore]`d as the pass-end gate
   (`cargo test -- --include-ignored`); an always-on fast slice (all
   Hindi files + PSA, 0.8s) keeps both-script coverage in every run.
   REVISES pass 2's "exhaustive replacing sampling" — the exhaustive
   test still exists and still gates, it just isn't the inner loop.
   Convention: agents in the loop run targeted tests
   (`cargo test --lib format`, `--test format_corpus`); the full suite
   + --include-ignored is the finish-line check.

## pass 6 — diff (2026-08-24, self-report)

The port itself is the sketch's: anchor-cut blocks, Myers over the derived
addresses, two-tier coalescing, the covered_by/dup_context/relabeled
narration, the status-gated word diff, and merge-as-projection. All 23
fixture cases and every narration pin from onion's `skeleton_fixtures.rs`
reproduce byte-for-byte on the FIRST run of the port, so what follows is
only where the spec was silent.

1. **A malformed `\v` designator still cuts a block, and it addresses as
   `BOOK c:0`.** THE SCENARIO: en_ulb ZEC 12:7 writes `\v 2"`, so the
   designator reader refuses it and the Toc keeps the row with number 0
   (dropping it would let verse 1 swallow verse 2's text). Onion has no such
   row — `derive_canonical_sids` leaves the sid unchanged when the number
   token is missing, so that text joins the PREVIOUS verse's block. The
   anchor cut cannot un-see the row, so this port makes a block whose address
   collides with the chapter open's `GEN 1:0`, and the `@N` id tiebreak keeps
   the two decisions distinct (`GEN 1:0`, `GEN 1:0@1`). An assessed
   divergence, kept: both documents derive it identically, so pairing, the
   partition and the merge all stay total, and the degraded file gets a
   MORE precise diff than onion's. Sound, medium-high.
2. **`is_usfm_structure_change` compares TEXT-kind token bytes only, so a
   verse NUMBER is not reader text.** Onion stripped `\marker` runs textually
   and kept the digits, so it would call `\v 1 a` vs `\v 2 a` "not a
   structure change"; the sketch ruled token-kind granularity, which puts
   Designator/NoteCaller/BookCode/AttrList outside the comparison. THE
   SCENARIO: case 15's coalesced `GEN 1:1` -> `GEN 1:1-2` pair, where the
   designator differs and the prose does not — counting digits as reader text
   narrates "the content changed", which is false. The number IS the address,
   and the address is already narrated by the unit's two sids. Sound, high.
3. **Whitespace means ASCII whitespace.** Onion's classifier used
   `char::is_whitespace`, which counts U+00A0. THE SCENARIO: a reformat that
   swaps a space for a no-break space reads `is_whitespace_change` in onion
   and a plain `Modified` here. NBSP is a character the reader sees, not
   layout, and the sketch ruled a zero-alloc byte walk. Sound, medium — flag
   for Will if a corpus ever churns NBSPs at scale.
4. **`Filter` grew an `opt_breaks` field.** THE SCENARIO: the ruled
   reader-text kinds are text + newline + optBreak, but `//` survival was
   hard-wired to `kinds[Paragraph] == Keep` — which also keeps `\p`. Three
   views disagree (structure wants both, `verse_text()` wants neither so
   `gr//ace` reads `grace`, reader text wants the break without the paragraph
   markers), and one `kinds` slot cannot spell three answers. Every existing
   recipe keeps its current behaviour. Sound, high.
5. **`Filter::reader_text()` unwraps EVERYTHING and keeps all text.** THE
   SCENARIO: case 19 diffs `\h Genesis` against `\h The Book of Genesis` —
   front matter is not verse text, but it is a diff UNIT, so a
   `verse_text()`-shaped filter would hand that unit an empty string. Nothing
   is `Remove`d, so note prose rides in undifferentiated (onion's v1 choice,
   kept explicitly). Sound, high.
6. **`to_edits` reads an Unchanged Shared unit out of the BASELINE whatever
   the decision says.** THE SCENARIO: MRK against itself with default side
   Current — every unit is Shared/Unchanged, and honouring "Current"
   literally emits 695 splices that each replace a byte range with identical
   bytes. The two ranges hold the same bytes by definition of Unchanged, so
   the merged document is the same either way; this is what makes "a book vs
   itself costs zero edits" true and turns the MRK-vs-ULT replay from 679
   splices into 17. `merge`'s bytes are unaffected (a test asserts the
   projection equals an independently assembled chosen-side text). Sound,
   high.
7. **Several insertions at one splice point ride as adjacent zero-width
   splices in LIST order.** THE SCENARIO: a reordered pair chosen Current
   puts two non-contiguous current ranges at one baseline gap, and a
   `SpliceEdit` carries exactly one insert range — so the run emits
   `{from,to,insert0}` then `{to,to,insert1}`. `apply_splices` builds left to
   right; the right-to-left splicing an editor session does (`edit::apply`'s
   rule) yields the same bytes, because the later edit lands first and the
   earlier one goes in front of it. Pinned by the round-trip law over 200
   generated pairs. Sound, high.
8. **One id-uniqueness mechanism, not two.** Onion made BLOCK ids unique with
   `#N` (a non-contiguous sid reuse) and UNIT ids unique with `@N`. Here the
   address already carries `_dup_N`/`_cdup_N` from the Toc's own occurrence
   counts, so two blocks render the same string only in entry 1's corner —
   `@N` alone covers it. Sound, medium-high.
9. **`diff` takes `&str`.** `lex` does, and unlike `format_edits` (which
   degrades non-UTF-8 to an empty edit list) a diff that degraded to "no
   differences" would be a lie about two documents. Sound, high.
10. **Anchors are `(unit, side)`, addresses render on demand.** Onion's
    `Anchor` carried a sid `String` and its units carried two more. Here
    `Addr` is a 12-byte `Copy` fact and `Display` is the renderer, so the only
    strings the skeleton owns are the unit ids the consumer contract requires.
    Sound, high.
11. **The generated-pair laws use a hand-rolled xorshift, no proptest dep.**
    As instructed, and it did not fight back: 200 pairs over verse/chapter
    shapes (bridges, duplicate numbers, reopened chapters, CRLF/LF mixes) and
    six edit shapes (delete, insert, reorder, renumber, retext, reformat),
    seeded once so a failure is reproducible. Shrinking would have nothing to
    shrink into — every generated document is already a handful of verses.
    Sound, medium-high.
12. **Not built, deliberately.** No n-ary unit model (2-way surface on
    N-ready primitives — the pairing key is document-independent, so N sides
    is a later widening, not a redesign). No serde/JSON (the session
    serializes; nothing here owns a wire format). No `check_splices` sibling
    to `check_edits` — the round-trip law over the corpus plus
    `apply_splices`' debug-assert are the checker, and a splice list nobody
    hand-authors needs no pre-flight. No by-chapter batching and no rayon
    (ruled: deferred to galley/braid). No interim carried-sid calling
    convention (this repo's Toc-derived addressing IS the always-derive
    convention).
13. **Divergence assessment (the pause-and-present stance).** Three places
    where the anchor cut and onion's token machinery could have fought,
    assessed rather than retreated from: entries 1, 2 and 3. None of them
    blocks the port — every fixture, narration pin and merge byte-exactness
    case passes unmodified — and none of them is a reason to port
    `derive_canonical_sids`. They are listed here as the record of what was
    weighed, and 1 and 3 are the two Will may want to overrule.

## pass 6 review (2026-08-24, Will's rulings)

All three flagged divergences RULED FINE by Will as-is: (1) malformed
`\v` cuts a block and addresses `c:0` with the `@N` tiebreak; (2) verse
numbers are not reader text in the structure classifier; (3) whitespace
= ASCII whitespace (consistent with format's NBSP-is-content ruling).
The `Filter.opt_breaks` field also stands.
