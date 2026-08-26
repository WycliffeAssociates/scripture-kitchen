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

## pass 7 — wasm/analyze (2026-08-24, self-report)

Four pieces, per the sketch: the emit layer in the ENGINE (`src/analyze.rs`,
seven flat u32 reads, offsets converted in one streaming sweep), the
diagnostics side-table codegen (`lint::catalog` → `galley/diagnostics.json`),
the bindings crate (`galley/`, ten tagged exports), and the hand-written TS
wrapper (`galley/galley.ts`, the one place the stride schema lives on the JS
side). Every RULED item in the sketch is honoured as written. What follows is
where the spec was silent, plus the three places Will's eyes are wanted.

1. **`blocks` stride 4 = `[class, from, content_from, to]`.** The sketch's
   table says four numbers and then spells out three. THE SCENARIO:
   `decorations.ts` hides `[from, contentFrom)` on every para line and
   `structure.ts`'s `setBlockMarker` REPLACES exactly that span — so without a
   content-start the editor has to re-lex the marker name to find where the
   chrome ends, which is the whole thing this wire exists to stop. The fourth
   number is the opening marker token's end (delimiter folded in, so it is the
   real content start). Sound, high.
2. **A block is a CST node whose row kind is Paragraph, Header, Periph,
   TableRow or Sidebar.** A match over `MarkerKind`, no marker list. THE
   SCENARIO: `\s5` resolves to NO row, so it is not block-level and opens no
   block — which is exactly what the CM probe hand-codes ("in uW USFM a chunk
   marker sits INSIDE a paragraph"). It still reaches the editor through
   `token_spans` carrying `OTHER | UNKNOWN`. The same rule makes `\id`/`\h`/
   `\toc1` blocks (kind Header/Paragraph), which is what gives the front
   matter its chrome spans. Sound, high.
3. **The class byte's flags are HEADING / FRONT / POETRY / CLOSER / UNKNOWN,
   all read off `Category`.** THE SCENARIO: the probe's three regexes.
   HEADING = `ParaTitlesSections`, FRONT = `ParaIdentification |
   ParaIntroductions | ParaPeripheral | DocumentStructure`, POETRY =
   `ParaPoetry | ParaLists` (the indented block classes). **DIVERGENCE, for
   Will:** the probe's hand-authored HEADING regex includes `ip` and `iot`,
   which are introduction PARAGRAPHS; the table files them FRONT. The regex is
   the impurity (its own comment says so) and the table is the authority, so
   the byte is right and the probe's rendering of an intro paragraph will
   change when it switches over. Sound, medium-high — worth naming because it
   is a visible behaviour change in the probe.
4. **`\c` and `\v` share one coarse class (CHAPTER_VERSE); the two dedicated
   reads tell them apart.** THE SCENARIO: `decorations.ts` needs `usfm-chap`
   vs `usfm-num-v`, which is a chapter-or-verse question — and `chapters` and
   `verse_anchors` answer it with the designator spans the widgets need
   anyway. Spending a 9th coarse class (or a flag) on a fact two whole reads
   already carry buys nothing. Sound, high.
5. **NOT emitted: the front sub-class (`id ide usfm rem sts` = metadata vs
   `h toc* toca* cl` = displayable).** THE SCENARIO: `decorations.ts` branches
   `usfm-meta` vs `usfm-front` on exactly that split — and NO table column
   makes it: `ide`/`rem`/`sts` share `ParaIdentification` with `h`/`toc`/
   `toca`, while `id`/`usfm` are `DocumentStructure`. Emitting it would mean
   an authored marker list in the class byte, which the sketch forbids. The
   editor slices the marker name (3–8 bytes it already holds) for this one.
   **Needs Will:** if that split has to be free, it belongs as a marker-table
   COLUMN, authored once, not as a list in the emit layer.
6. **`text_runs` is the reader-text MASK, not the Text tokens.** THE
   SCENARIO: the read serves "search/proofing views", and vision §10.1 says
   Sous needs an untrimmed projection that reads across markup boundaries —
   which is `Filter::reader_text()` exactly (unwraps everything, keeps all
   text including note prose, merges adjacent survivors into maximal runs).
   Text tokens would hand back a run per marker boundary, which is the thing
   a search must not see. This is also the only read that costs a mask, so
   the wants bit genuinely gates a whole artifact. Sound, high.
7. **An absent designator is an EMPTY span at the marker, not a sentinel.**
   THE SCENARIO: `\c` with no number, and `\v 2"` (en_ulb ZEC 12:7), where the
   Toc keeps the row with number 0. A consumer slicing `label.from..label.to`
   gets `""` for the first and `2"` for the second — both of which are what it
   would render anyway. `NONE` is reserved for the two places absence is
   structurally different: a diagnostic's second span and its fix index.
   Sound, high.
8. **One sentinel, `u32::MAX`, everywhere.** So a decoder needs one rule.
9. **The UTF-16 wall is a streaming `Cursor` with an ascending fast path and a
   sorted-permutation fallback.** THE SCENARIO: three reads are NOT ascending
   — a `\p` nests inside an `\esb` so blocks overlap, a diagnostic's `second`
   always PRECEDES its anchor (the lint invariant), and a chapter's label sits
   inside its own span. Those three are all small (hundreds to thousands of
   rows), so they sort a position list first; `token_spans`, the read that is
   millions of offsets on a big book, is ascending by construction (tokens
   tile the document) and pays one `is_sorted`-style scan and nothing else.
   The sketch said "one streaming cursor riding the emit in document order",
   and this is that, with the honest admission that document order is per
   READ and not global. Sound, high.
10. **`Cursor` went into `src/utf16.rs`, beside the index it mirrors.** As
    instructed — it reuses `utf16_len` (the SWAR count) and both of the
    index's scan loops, and it is proved against `Utf16Index` at every byte
    offset of the existing zoo, forwards, backwards and interleaved. No
    random-access index is built for bulk out; the only `Utf16Index` uses left
    are the format/diff wires (small, unsorted, random-access) and the two
    `to_byte`/`to_utf16` stragglers.
11. **Fixes cross as FOUR arrays, and a diagnostic's `fix` indexes the
    `fixes` array, not an edit.** THE SCENARIO: a fix can hold several edits
    (`bridge-empty-verses` deletes two `\v`s and rewrites a third), so the
    ruled shape — `[from,to]` pairs + one ASCII blob + a lengths array — needs
    a fourth array grouping edits into fixes. `format_edits`' precedent is the
    inner three verbatim; the grouping is the only addition. Sound, high.
12. **Diagnostics and fixes share ONE wants bit.** A fix is worthless without
    the finding that offers it, and the fix arrays are tiny beside the
    findings. Sound, high.
13. **A clip keeps a span that OVERLAPS it, and needs BOTH ends.** THE
    SCENARIO: a token straddling the viewport edge is half-drawn if the test
    is containment. At the binding, one-sided (`clipFrom` without `clipTo`)
    means NO clip rather than "to the end" — a half-specified viewport is a
    caller bug, and silently analyzing a different range than asked is worse
    than analyzing all of it. Sound, medium-high.
14. **The LF contract is a documented degrade in the library and a
    `debug_assert!` at the galley wall.** Per Will's ruling, and this is where
    it can live: the corpus HAS CRLF books, and `tests/analyze_corpus.rs`
    analyzes them — a library-level assert would fail the suite on real data
    the engine handles correctly (it stays byte-honest and counts the CR as
    its own unit). The offset contract is galley's, so the assert is galley's.
    Sound, high.
15. **The side-table carries `escalation` and the Form rows, neither of which
    was asked for.** THE SCENARIO: `attr-trailing-form-deprecated` has
    `severity: null` plus a ladder — without the ladder JS either shows a
    gated rule as permanently silent or as its post-escalation rung, and both
    are wrong. It is `[[version, severity], …]` and `galley.ts` has the
    six-line `severityAt` that reads it. The Form rows are emitted so the
    array stays DENSE and index-addressed (a finding carries `code` as a
    number); they are marked `"severity": "form"` and a test asserts none of
    them ever reaches a diagnostics read — over three shaped documents in the
    unit test and over every corpus book in the oracle. Sound, high.
16. **`galley/diagnostics.json` is a codegen artifact beside the crate that
    ships it, checked in, with the marker table's staleness test.** Codegen
    now writes two files and reports both. Sound, high.
17. **`FormatOpts` is a tagged struct: `pub` scalar fields, the three enums as
    u32 codes, `setRemoveMarkers` taking a comma-separated string and
    `setRepairs` taking a `Uint32Array` of code indices.** THE SCENARIO:
    twelve positional booleans in a `.d.ts` is a call site nobody can read,
    and adding a switch later would renumber it. The enums are u32 rather than
    tagged enums because a wasm-bindgen enum is a second name for a fact the
    library already names — the codes are documented on the fields and the
    wrapper hides them. `repairs` accepts side-table indices and silently
    drops one that names no row (a stale bundle must not panic the engine).
    Sound, medium-high.
18. **`merge`/`merge_splices` are thin wrappers over `String`-erroring
    bodies.** THE SCENARIO: `JsError` cannot be CONSTRUCTED off wasm, so a
    native test of the loud-rejection contract (unknown unit id, unknown side
    name, bad JSON) panics inside wasm-bindgen instead of seeing the
    rejection. The split costs four lines and makes the rejection contract
    testable in `cargo test`. Sound, high.
19. **The diff wire flattens `dup_context` and renders `covered_by`'s sid.**
    `baselineCount`/`currentCount`/`isDup` instead of a nested object, and
    `coveredBy: {unit, sid, side}` with the address already rendered — the id
    renderer stays Rust, per the ruling, and that applies to every address the
    wire carries, not only the unit ids. Slots carry `afterUnit`/`afterSide`
    for the same reason. Sound, medium-high.
20. **The wasm smoke test INLINES a Hindi excerpt.** THE SCENARIO:
    `testData/` and `example-corpora/` are gitignored, and `include_str!` of a
    missing file is a COMPILE error where a missing corpus should be a skip —
    which would make `wasm-pack test` unrunnable on a fresh clone. The excerpt
    is verbatim `hindi-IRV1` MAT 1:1-2 plus a synthetic chapter 2 with a
    footnote, and the pinned numbers are hand-counted (438 UTF-16 units for
    844 bytes; `\c 1` at byte 196 = unit 156; `\c 2` at byte 700 = unit 364;
    the last verse's designator at unit 405). The whole-corpus Hindi check
    stays native, where it can read files. Sound, high.
21. **No checksum export, on Will's ruling (2026-08-24).** Vision §13.4
    assigns the canonical-source checksum to the composed facade; Will
    deferred it as a higher-level concern — revisit when sous joins and the
    host owns a source version. Recorded in `galley/README.md` and the module
    doc as deliberately absent, not forgotten. No `twox-hash`, no hashing
    dependency at all.
22. **The bindings crate is `galley`, per Will's ruling** — ONE combined
    bindings crate, no standalone `onion-wasm`, no standalone `sous-wasm`. Its
    doc header says it is the vision's composed analysis host (§4.4, §9.4) and
    that sous joins it later in the same binary.
23. **Not built, deliberately.** No `Wants` TYPE — a bare `u32` plus a consts
    module IS the wire, and a newtype would only need unwrapping at the tag.
    No `locate`/`book`/`usj`/`usx`/`html` exports: the sketch's galley method
    list accumulates "as real UI asks arrive", and none of those has arrived.
    No opaque handle, no retained state, no worker, no async — all ruled, all
    parked. No `simd128` in the crate's build config: it is a RUSTFLAG the
    consumer sets, documented in the README, not a fact baked into a
    `.cargo/config.toml` that would also change native builds. No wasm bench
    in-repo (ruled: the probes stay scratch).
24. **The playground grew `--analyze` and `--analyze-wants <mask>`** — the
    perf instrument for §8, and the per-read breakdown in it.
25. **The corpus oracle is NOT `#[ignore]`d.** The whole-corpus reference-
    emitter sweep over all 226 books runs in ~7s (rayon, debug), which is not
    the minutes the ignore convention exists for — so there is no fast slice
    and no pass-end-only gate for this one, just the sweep plus the Hindi
    hand-check. Sound, high.

## pass 8 — spike feedback (2026-08-24, self-report)

Nine asks from `planning/ideas/candidates/spike-gaps.md` — the CodeMirror
spike's report after replacing every hand-rolled USFM fact in a real editor
integration with `analyze`. The full evidence, with the workaround each ask
was living with, is `onion-2-spike/GAPS.md`; each section there now carries a
RESOLVED note with the shape that shipped. Ask 9 (`toByte` re-encode) was
skipped as documented design. What follows is where the spec was silent.

**Two pass-7 items are RESOLVED by this evidence.** Item 5 ("needs Will": the
metadata-vs-displayable front split, which pass 7 declined to emit because no
table column made it) — the spike CONFIRMED the split is load-bearing, listed
the exact two groups, and showed that the one marker-name regex the class byte
was meant to delete was still in the probe because of it. It is now a `META`
flag read off the CATEGORIES, no authored list, no new table column (choice 3
below). Item 3 (the `\ip`/`\iot` DIVERGENCE, where the probe's hand-authored
HEADING regex disagreed with the table's FRONT) — the spike followed the
engine and reports the engine was right; the divergence is settled in the
table's favour and the regex is gone.

1. **The class byte became a class WORD (u16), and `token_spans` moved its
   kind byte from `<< 8` to `<< 16`.** THE SCENARIO: `META` is a ninth fact
   and all eight bits were spent (3 coarse + HEADING/FRONT/POETRY/CLOSER/
   UNKNOWN). The alternatives were splitting FRONT into two mutually exclusive
   flags (which loses "is this front matter at all" for consumers that only
   knew FRONT) or putting META at bit 8 in the reads where class owns a whole
   u32 and nowhere else (dishonest — the same field would mean two things).
   Widening keeps the low byte BIT-IDENTICAL, so every existing constant is
   unchanged and only `token_spans`' companion field shifts. Sound, high.
2. **`META` = `ParaIdentification | DocumentStructure`; plain `FRONT` =
   `ParaIntroductions | ParaPeripheral`.** THE SCENARIO: the spike's own two
   lists — `\id \ide \usfm \rem \sts \h \toc1-3` (machine metadata, dimmed or
   hidden) against `\ip \iot \io1 \is \imt` (introduction prose the reader
   sees) — fall exactly on that category line, which pass 7 had not checked
   against a real list. No marker names anywhere, so a marker nobody
   enumerated classifies itself. FRONT still covers both halves. Sound, high.
3. **`NUMBER_SHAPED` rides the HIGH BIT of the `chapter`/`number` field
   rather than a sixth/eighth slot.** THE SCENARIO: both fields are `u16`
   values inside the engine (chapter and verse numbers saturate at `u16::MAX`
   before they are ever emitted), so bit 31 is unreachable by real data — and
   the ask proposed stride 5/7, which a packed flags slot would have made 6/8
   for one bit. The verdict is the DESIGNATOR INTERPRETER's own
   (`designator::verse`/`::chapter`), the same one `designator-malformed`
   reports, not a second reading of the bytes. CLEAR is the conservative
   answer. Sound, medium-high — a packed flag is the thing a decoder can get
   wrong, which is why `galley.ts` decodes it into a boolean and no consumer
   sees the mask.
4. **An ABSENT designator now reports its empty span at `content_from` (the
   marker's end), not at the marker's START.** THE SCENARIO: `\v \n` reported
   `[48,48)` where the `\v ` token is `[48,51)`, so an editor rendering a
   propped-open empty verse slot could not place it from the read — the spike
   had to pair each anchor with the marker token before it, which is the main
   reason its commit set carried whole-book `TOKEN_SPANS` at all. This is a
   BEHAVIOUR CHANGE to an existing field, recorded as such: the old answer was
   not wrong, it was just not the position anyone wanted. Sound, high.
5. **A MARKED LINE is a line whose first NON-WHITESPACE token is an OPENING
   marker.** THE SCENARIO: three sub-decisions.
   (a) *Non-whitespace, not line-start.* `  \p text` is a marked line here
   where the spike's own line-start test called it plain text. An indented
   marker is a Form finding (`marker-ws-at-line-start`), not a non-marker, and
   the leading whitespace lands inside the chrome run where it belongs.
   (b) *Opening markers only.* A line starting with `\f*` or `\ts-s\*` gets no
   row: a closer opens nothing, and a milestone is a point inside a line.
   (c) *No class filter.* A line opening with `\w` gets a row carrying CHAR,
   and the consumer decides that is content. The engine says what the line
   IS; what to do about it is the editor's. Sound, medium-high — (a) is the
   one place this deliberately diverges from the spike it was built for.
6. **`content_from` on a line runs past the marker AND its designator.** THE
   SCENARIO: `\c 1` and `\v 5 ` — the chrome an editor hides is the whole
   numbered opener, and the designator token already folds its own trailing
   delimiter, so one number covers both. Clamped to the line's end for the
   degenerate `\v` at EOF. Sound, high.
7. **`note_parts` rides the existing `NOTE_EXTENTS` bit rather than taking a
   ninth.** THE SCENARIO: a part indexes an extent — `[note_index, …]` — so
   the read is meaningless without it, which is the same argument diagnostics
   and their fixes already share a bit on. It also keeps the wants mask a
   description of ARTIFACTS rather than of arrays. Sound, high.
8. **The four part kinds PARTITION the extent: CALLER / ORIGIN / BODY /
   MARKUP.** THE SCENARIO: the spike wanted two different things out of a
   note and was solving them separately — the apparatus row regexed `\fr`/`\ft`
   out of the note's bytes, and the freeze filter walked every token in the
   extent looking for markers. A partition serves both: render ORIGIN + BODY,
   freeze CALLER + MARKUP, and every byte is accounted for. Adjacent text of
   one kind merges into a single run, so a body wrapping a line comes back
   whole (the spike's regex could not represent that at all). Sound, high.
9. **`\fr`/`\xo` are resolved THROUGH the marker table by NAME.** THE
   SCENARIO: no `Category` separates an origin reference from a `\ft` — both
   are `CharNotes` — and no other column does either. The alternative was
   positional ("the first note-internal child is the origin"), which
   misreports `\f + \ft body\f*`, a shape the corpus has. This is the same
   mechanism `note_family` already uses for the five note spellings: a
   `marker_idx` comparison, so a renamed row degrades instead of
   mislabelling. **Not a category, so it is the one place in this pass a
   marker NAME appears in the emit layer** — named here rather than buried.
   Sound, medium — if a third consumer wants it, it should become a column.
10. **`usfm_version` is a SCALAR on the analysis, computed unconditionally,
    and it reports the LADDER INDEX rather than resolving severities.** THE
    SCENARIO: `header_scan` is bounded at the first `\c`, so it costs nothing
    even at `wants == 0`; and the ask offered "report the version" or "emit
    the effective severity per finding". The second would delete `severityAt`
    from the wrapper but also delete the LADDER, which is what a settings UI
    shows ("this becomes an error at 4.0"). `u32::MAX` is the undeclared
    sentinel, matching every other absence on this wire — and undeclared is
    NOT 3.0, which is the distinction two gated rules turn on. Sound, high.
11. **Diagnostic spans are trimmed of the folded delimiter at EMIT — anchor
    and second alike — and 27 templates lost a `\` they were double-printing.**
    THE SCENARIO: the ask named ONE template (`unterminated-milestone`),
    because that is what the spike's demo document surfaced. Sweeping all 262
    corpus books with the new `--codes` listing shows every template
    containing `\{anchor}` or `\{second}` has an anchor whose first byte IS
    the backslash — `unclosed-note` rendered `\\f was never closed` too, and
    26 more. Fixing one and leaving 26 would have left the wire inconsistent
    in exactly the way that made the bug invisible. A `rows.rs` test now fails
    the build if a template writes a backslash in front of a placeholder.
    Sound, high — but a wider change than the ask, so: named.
12. **`token_spans` is NOT trimmed.** THE SCENARIO: same family as 11 (the
    spike's milestone pip eats the space after the token), but that read is
    the LOSSLESS PARTITION of the document — a corpus law asserts the spans
    tile it — and trimming would put a hole in the tiling. The pip's trim is
    a rendering decision and stays in the consumer, three lines, where it can
    see what it is drawing. Sound, high.
13. **`locate` takes a UTF-16 offset, not a byte, and is TOTAL.** THE
    SCENARIO: the sketch's method list said `locate(byte)`, but every caller
    holds an editor offset — making them convert first puts a straggler
    conversion at the one call site where getting it wrong is silent (a
    slightly wrong sid still reads like a sid). `book` returns `""` rather
    than `"###"` for a document with no `\id`: the `###` rendering belongs to
    a SID, which has to name something, and an empty book code is a fact the
    caller may want to branch on. Sound, high.
14. **`consume()` is the paved free path; `view()` survives without the
    free.** THE SCENARIO: `view()` never freed and nothing said who must, so
    a consumer that missed it leaked one wasm allocation per keystroke.
    `consume(analyze(...))` copies every read out and frees in a `finally`.
    `view()` is kept for a caller that owns the handle's lifetime itself (a
    `using` binding, or a test holding one handle across two views) — deleting
    it would have made the safe thing the only thing at the cost of making an
    honest use case impossible. Sound, high.
15. **`WANTS_COMMIT` deliberately EXCLUDES `TOKEN_SPANS`.** THE SCENARIO: the
    constant is documentation as much as convenience, and the point of the
    line read is that an editor no longer needs token truth everywhere the
    caret can go. The spike still ORs the bit in for two inline decoration
    families and says so in a comment; the constant says what the set is FOR.
    Sound, medium-high.
16. **`AnalysisView.forEachToken(fn)` — an allocation-free token walk,
    which nobody asked for.** THE SCENARIO: `tokens()` yields an object per
    row, and the wrapper's own doc says the read "must never be materialised";
    but a consumer scanning it for two shapes had no other way to obey that.
    On John (6,146 tokens) the object generator was the whole remaining cost
    of the spike's projection. Callback returns `false` to stop early. Sound,
    medium-high.
17. **`--codes` sweeps the LOADED corpus for examples rather than shipping
    authored fixtures.** THE SCENARIO: the ask wanted "one rendered example"
    per code, and 54 authored trigger documents in a bin is a second corpus to
    maintain and to get wrong. Real bytes from real books are the point —
    that is what made ask 11's true scope visible. Codes the corpus does not
    trigger say so (6 of 54 over `testData/`); Form rows say what they are.
    Sound, high.
18. **The corpus oracle grew a SECOND derivation for each new read, not a
    copy of the emitter.** `ref_note_parts` takes a note's tokens BY SPAN
    (every token inside the extent) where the emitter walks the TREE; `ref_lines`
    re-steps the token stream; `ref_slot` re-derives the designator by stepping
    past an optional front attribute list. Agreement over 226 books is then
    evidence rather than tautology. Sound, high.
19. **Not done, deliberately.** No `words`/`milestones` read, though the
    measurement now says that is what is left of the spike's projection
    (0.70ms of 0.81ms on John) — recorded as a new ask at the bottom of
    `GAPS.md`, because the honest argument for it is "the editor should never
    touch a token read at all", and that claim wants a second consumer before
    it becomes wire. No viewport clip wired into the spike (its `project` runs
    inside a `StateField`, which cannot see a viewport — an editor-architecture
    fix, not an engine one). No new probe checks in the spike: the gate is
    11/11 and adding checks would have moved the number this pass is measured
    against.

## pass 9 — spike iteration (2026-08-25, self-report)

The engine was read-mostly this pass; the work was in the CodeMirror spike. One
additive change landed here.

1. **`editList` / `consumeEdits` in `galley/galley.ts` — the format wire gets a
   decoder, beside the fix decoder it duplicates.** THE SCENARIO: the spike grew
   a Format command (book and chapter scope, preview, apply as one trusted
   transaction) and had to turn `formatEdits`'s `{spans, lens, text}` into
   `{from, to, insert}` records. That decode is the wire schema, and the wire
   schema's one home is `galley.ts` — a copy in the spike is exactly the drift
   the "we decode, and in one place" ruling exists to prevent. Eager rather than
   lazy on purpose: a format transaction is small, a human looks at a preview
   before applying it, and no wasm handle should stay alive across that wait.
   No `.wasm` change, no export change; `Edits` already crossed. Sound, high.
2. **Not done.** Chapter-scoped formatting is the SPIKE filtering a whole-book
   edit list to the chapter's span — the engine has no ranged `format_edits`,
   and whether it should is recorded as an ask in the spike's `GAPS.md` rather
   than answered here.

## pass 8/9 review (2026-08-25, Will's rulings)

- Ask 12 (`\c 12b` NUMBER_SHAPED clear, `\v 2b` set) — RULED CORRECT
  by Will: grammar-honest (chapters are bare integers, verses take
  segments).
- Crate layout CORRECTED: the five pieces are onion / onion-wasm /
  sous / sous-wasm / galley, where GALLEY is the higher-level
  workflows crate (dirty-marking, checksumming, ingest recipes, find,
  onion↔sous coordination) and the crate currently named `galley/` in
  this repo is actually ONION-WASM (bindgen + .d.ts + JS/UTF-16
  utilities). Rename pending Will's go; planning/ideas/committed/
  galley.md (renamed from braidv2) is where the real galley's design
  accumulates.

## pass 10 — onion-wasm rename (2026-08-25, self-report)

Mechanical rename plus three behaviour changes at the JS wall. No engine
(`src/`) logic changed; the corpus oracles and the diff/format/analyze surfaces
are untouched.

**What moved.** `galley/` → `onion-wasm/` (crate `onion-wasm`, lib `onion_wasm`),
`galley/galley.ts` → `onion-wasm/onion-wasm.ts`, `diagnostics.json` in place.
Workspace member, `.gitignore`, `src/bin/codegen.rs`'s output path,
`tests/codegen_output_matches_input.rs`'s `include_str!`, and the doc references
in `src/analyze.rs` / `src/bin/playground.rs` all follow. In the spike:
`src/galley/` → `src/onion-wasm/` (vendored `pkg-web/`), `scripts/sync-galley.sh`
→ `sync-engine.sh`, `scripts/probe-galley.mjs` → `probe-engine.mjs`, npm script
`probe:galley` → `probe:engine`.

1. **The TS file is `onion-wasm.ts`, not `index.ts`.** THE SCENARIO: the file is
   vendored into a consumer's tree beside a `pkg-web/`, and an `index.ts` in a
   stack trace or a grep tells you nothing about which package it belongs to.
   The subpath export `./schema` gives the ergonomic import name without costing
   the identity. Sound, high.
2. **`analyze` returns a `js_sys::Object`, hand-built, rather than a
   serde-wasm-bindgen struct.** THE SCENARIO: the return is fourteen keys of
   already-flat data built once per keystroke. `js-sys` is a
   `Object`/`Reflect`/`Uint32Array` dependency the wasm-bindgen tree already
   carries; serde-wasm-bindgen would add a serializer and its derive machinery
   to the hot read path to emit the same fourteen keys. Sound, high.
3. **The generated `.d.ts` types the return as `object`, and the ONE cast lives
   in `analysis()`.** THE SCENARIO: typing it properly needs an extern
   `typescript_type`, which needs the fourteen-field interface DECLARED in Rust
   — a second copy of a schema whose whole point is having one home
   (`RawAnalysis` in `onion-wasm.ts`). So the binary stays honest about handing
   back an object, and `analysis(analyze(...))` — the wrapper's single
   `as RawAnalysis` — is what every call site writes. Sound, medium-high: a
   consumer who skips the wrapper gets no field types, which is the intended
   pressure toward using it.
4. **`consume()` / `view()` / `OwnedAnalysis` deleted rather than deprecated.**
   THE SCENARIO: nothing wasm-side outlives the call now, so a `free()` on the
   analysis path is not merely unnecessary, it is a lie about the lifetime. One
   spike consumer, renamed in the same pass. Sound, high.
5. **`--weak-refs` is a BUILD flag in the scripts and the README, not a Cargo
   feature.** THE SCENARIO: wasm-bindgen 0.2.127 has no such crate feature — it
   is `wasm-pack build --weak-refs`, so it can only be enforced where builds are
   spelled out. It is the backstop for the handles that remain (`FormatOpts`,
   `Edits`, `Splices`), which are still freed explicitly. `using` /
   `Symbol.dispose` is not used anywhere: it crashes older webviews (ruled).
   Sound, high.
6. **`WANTS_COMMIT` moved into the spike, and the wrapper gained a comment
   saying why rather than a replacement constant.** THE SCENARIO: which reads an
   editor needs per accepted change is a CodeMirror probe's opinion — the spike
   already had to OR `TOKEN_SPANS` onto it, which is the tell. `WANTS` and
   `WANTS_ALL` stay; the app composes `COMMIT_WANTS` from bits. Sound, high.
7. **The lib.rs native tests moved onto a `reads()` helper.** THE SCENARIO:
   `js_sys` values cannot be built off wasm, so the old native tests over the
   `Analysis` handle would panic in the shim instead of reading numbers. Same
   split the file already used for `merged` / `splices`. The BOUNDARY test
   (`tests/node.rs`) now reads the object's keys BY NAME, which is the drift
   check the plain object needs and the handle did not. Sound, high.
8. **Packaging (Will's mid-pass ruling): both distributed builds are
   COMMITTED.** `pkg-web/` (`--target web`, explicit `init()`) and
   `pkg-bundler/` (the default) are in the tree; only the scratch `pkg/` and
   `pkg-node/` stay ignored. A hand-written `onion-wasm/package.json` sits ABOVE
   the two generated ones and maps subpaths (`.` → bundler, `./web` → web,
   `./web/wasm`, `./schema`, `./diagnostics.json`) — chosen over editing either
   generated `package.json`, which the next `wasm-pack build` would overwrite.
   `"private": true` on it so nothing can be `npm publish`ed by accident; a git
   install ignores the field. ONE build per target, full default features — no
   lean prebuilts, no consumer asking. Sound, high.
9. **`wasm-pack` writes a `.gitignore` of `*` into every out-dir**, which would
   silently keep the committed artifacts out of a tag. The sync script deletes
   both, and the README's build block says so. Sound, high.
10. **The spike vendors `pkg-web/` under that name, not `pkg/`.** THE SCENARIO:
    with two targets committed upstream, a bare `pkg/` in the consumer no longer
    says which one is in it. Sound, high.
11. **Names inside the spike followed the crate: `[galley]` log tags →
    `[engine]`, `window.__galleyReady` → `__engineReady`, `/tmp/galley-probe` →
    `/tmp/engine-probe`, the re-exported `GalleyBlock`/`GalleyLine`/`GalleySpan`
    → `Engine*`.** THE SCENARIO: leaving them would teach the wrong name at
    every console line and screenshot path. PERF.md's and README-SPIKE.md's run
    commands were updated with them (they are instructions); GAPS.md got a
    header note instead, because it is a dated report and rewriting its body
    would be rewriting history. Sound, high.
12. **`src/cm/grammar/` and `scripts/probe-grammar.mjs` deleted from the
    spike.** Verified no live import: the only references left were a comment in
    `probe-cm.mjs`, prose in `cmWysiwyg.md`, and the demo book's own text. The
    reference model predates `analyze` supplying the same structure. `cmWysiwyg.md`
    got a note at the top saying the model is gone and its RULES are live in
    `src/cm/`; the demo book's text was left alone, because the probe asserts
    against those bytes. Sound, high.
13. **Not done.** The subdirectory problem for GitHub-tag installs is RECORDED,
    not solved: `npm i github:org/usfm_onion_2#tag` installs the repo root, and
    `onion-wasm/` is not the root. That wants a decision (subdirectory-aware
    installer, or a split publish repo) before anyone is told to install from a
    tag; it is in the sketch's §Distribution.

## pass 11 — content_from single-delimiter (2026-08-25, self-report)

The bug Will hit driving the editor (GAPS.md ask 14, his framing): `analyze`
emitted `content_from` as the TOKEN's end, and the scanner's delimiter fold gives
a marker/designator token its WHOLE horizontal-whitespace run. Verified live
before touching anything — `\v 1 Put` → `content_from` 13; type one space at 13;
`\v 1  Put` → `content_from` 14. The editor hides `[num_to, content_from)` as
chrome, so it hid the byte the author had just typed: silent, invisible document
growth. `format`'s delimiter-single row already called run-beyond-one-byte
trimmable, so the two surfaces contradicted each other over the same bytes.

1. **The fix is at the EMIT layer; the scanner's fold is untouched.** THE
   SCENARIO: the fold is what makes a marker token lossless and every other
   consumer (format's `delimiter`, `trimmed` for diagnostic anchors) depends on
   the token owning its run. One helper, `content_after`, is the whole change:
   payload label end + ONE delimiter byte when the folded run is non-empty, the
   label's end when it is empty. Sound, high.
2. **Applied UNIFORMLY, not just to `verse_anchors` where it was reported.**
   THE SCENARIO: `\p    text` had the identical hole with no `num_to` to clamp
   against — the editor hid four spaces. So `chapters.content_from`,
   `blocks.content_from`, `lines.content_from` and `verse_anchors.content_from`
   all run through the same helper, and `blocks` grew a `source` parameter to do
   it. Sound, high.
3. **The ABSENT-designator slot moved with it.** THE SCENARIO: `\v   ` with no
   designator collapsed the propped-open slot onto the MARKER token's end, which
   is past the whole run — the same disease one level over. It now collapses
   onto the marker's own `content_after`, so `\v   ` props the slot open at 3,
   not 5. `\v ` (one space) is unchanged, which is why no existing test moved.
   Sound, high.
4. **`note_parts` had the same disease and was fixed with it.** THE SCENARIO
   (checked, present): a MARKUP part carried its token's whole run, so a space
   typed after `\ft ` became frozen apparatus chrome; and a CALLER's run beyond
   its label belonged to NO part at all, so `\f +   \ft` left three bytes the
   "parts partition the extent" contract did not cover. Now MARKUP is the marker
   plus one delimiter, CALLER stays the trimmed label (it is the analogue of a
   designator's NUMBER span, not of `content_from`), and the remainder of either
   run comes back as ORIGIN/BODY — leading content whitespace, merged into the
   text run that follows it. `\f +   \ft   note\f*` → CALLER `+`, BODY `  `,
   MARKUP `\ft `, BODY `  note`, MARKUP `\f*`. Sound, medium: it is the
   consistent reading, and an apparatus that froze MARKUP had the same silent
   growth, but nobody has driven the apparatus into it yet.
5. **The note's own OPENER is still not a part.** THE SCENARIO: `\f  + x` — the
   opener token is skipped outright by `note_parts`, so its extra space is
   unpartitioned like the rest of the opener. Left alone: the opener is chrome
   the EXTENT names, and making it a part would change what "the parts partition
   the extent" has always meant. Recorded rather than fixed. Sound, medium.
6. **The delimiter is DERIVABLE, not a new field.** THE SCENARIO: the ask
   offered "report the delimiter as its own span". `[num_to, content_from)` is
   now exactly one byte or empty by construction, so a span would be redundant
   wire. No stride changed. Documented in `analyze.rs`'s module doc, the four
   read docs, and the wrapper's `Chapter`/`Line`/`Block`/`VerseAnchor` types.
   Sound, high.

### Will's three rulings, as implemented — PROVISIONAL, pending his review

All three fall out of the single rule; none needed a special case, which is the
argument that the rule is the right one.

7. **`\v 1\ttext`: the single delimiter byte is the TAB.** `content_from ==
   num_to + 1` whichever horizontal-whitespace byte sits there (`payload_label`
   already trims SPACE and TAB alike). `format`'s delimiter-single row
   normalizes it to a space when run, so the two surfaces agree on the byte
   count first and the spelling second. PROVISIONAL.
8. **`\v 1` at end of line, content on the next line: `content_from ==
   num_to`.** No horizontal delimiter EXISTS to step over — newlines never fold
   — so the propped-open slot sits at EOL and the Newline token stays visible
   structure. This is the one case where the delimiter span is EMPTY, and it is
   why the rule is "+1 when the run is non-empty" rather than "+1". PROVISIONAL.
9. **`\v 1` + trailing spaces + no content: `content_from == num_to + 1`.** The
   remaining spaces are visible TRAILING whitespace, which format's at-line-end
   delimiter rule deletes (its `at_line_end` branch replaces the run with `b""`).
   PROVISIONAL.

### Tests

10. **The bug test replays the KEYSTROKE, it does not pin a number.** THE
    SCENARIO: a test asserting `content_from == 13` would pass against a future
    emitter that was wrong in a new way. `typing_a_space_at_content_from_...`
    analyses `\v 1 Put`, splices one space in AT the emitted `content_from`,
    re-analyses, and asserts the boundary is unchanged and the byte at it is the
    space. Sound, high.
11. **The corpus oracle's reference emitter re-derives the rule, it does not
    call the helper.** `ref_content_from` counts the trailing SPACE/TAB run off
    the bytes and keeps one; `analyze` reaches the same offsets through
    `payload_label`. Green on all 226 books. Sound, high.
12. **Nothing in the existing unit tests moved, and no corpus pin moved.** THE
    SCENARIO worth recording: every example in the module doc, the read docs and
    the existing tests uses a SINGLE delimiter, where old and new rules agree —
    so the whole suite (`--include-ignored`) was green with zero pin edits. The
    only rows that move in a real book are ones with a multi-byte run, and the
    oracle re-derives those independently rather than pinning them.

### Wrapper ergonomics (Will's mid-pass addition)

13. **`wants({...})` is a TS-only helper; the wire tag stays `wants: u32`.** THE
    SCENARIO, and the argument: a misspelled key in an object crossing into wasm
    would be silently ignored and the read would come back empty with no error.
    In TS an object literal cannot carry a property the `Wants` interface does
    not declare, so the typo is a compile error. `WANTS` stays exported for
    anyone composing bits dynamically. Sound, high.
14. **The `Wants` keys are named after the DECODER METHODS, not the bit
    constants**: `chapters`, `blocks`, `lines`, `notes`, `tokens`, `textRuns`,
    `verseAnchors`, `diagnostics`. THE SCENARIO forcing a choice: the bits are
    `NOTE_EXTENTS`/`TOKEN_SPANS`/`VERSE_ANCHORS` but the methods are
    `notes()`/`tokens()`/`verseAnchors()`, and a consumer writes the ask and the
    read in the same file — so what you ask for should be spelled the way you
    read it back. `notes` covers both `notes()` and `noteParts()`, which is the
    one key that is not one-to-one, and its doc says so. Sound, medium: it is a
    naming call, and the other direction (mirror the bit names) is defensible.
15. **There is no wrapper-level `analyze` to overload.** THE SCENARIO: the
    better-shaped option Will offered — `analyze(text, Wants | number)` — has no
    home, because consumers call the wasm export directly and the wrapper is
    decoders only. A free `wants()` function was the available shape. Sound,
    high.

### Verification

Engine: `cargo test --workspace -- --include-ignored` green (335 lib tests plus
every corpus oracle), `cargo clippy --all-targets --workspace` clean, both wasm32
targets built by `scripts/sync-engine.sh`. Spike: re-vendored, `npm run build`
clean, `npm run probe:engine` 17/17 — the 16 prior checks plus a new
`typed-space-is-content-not-chrome` that types a space at `content_from`, asserts
the boundary did not move, asserts the line's ON-SCREEN text grew by one
character (the byte rendered instead of being hidden), and types a second
character to show it lands adjacent to visible text. No local clamp existed at
`decorations.ts` `numberSlot` or `structure.ts` `chromeRanges`/`chromeGuard` and
none was needed — those sites read `content_from` and self-healed.

## pass 12 — empty-paragraph chains (2026-08-25, self-report)

Will's live bug: `\p` ␊ `\p` ␊ `\p` ␊ `\p before …` lost ONE empty per format
run. Pass 5 entry 9 declined the chain on purpose — the fix oracle judges a fix
at its own SITE, and deleting one member leaves another empty paragraph at that
byte. Will's ruling: N repeated empties reduce in one run. The fix is now
CHAIN-AWARE, and pass 5 entry 9 is superseded by entry 1 below.

1. **THE RULE, as implemented.** A maximal RUN of consecutive empty paragraphs
   sharing one identical spelling, followed by a same-spelling paragraph that
   HOLDS CONTENT, is duplication: one fix deletes every extent in the run (each
   marker plus the line endings under it, the existing extent logic unchanged).
   The two refusals pass 5 banked are untouched — a mixed spelling (`\m` then
   `\p`, and equally `\p\n\p\n\m text`) offers nothing, and a run whose survivor
   is empty or is EOF offers nothing. `\p\n\p\n\q1` (en_ulb ISA) is the second
   kind: the run is two identical `\p`s and its survivor is a `\q1`, so it stays
   fixless, exactly as before.
2. **The fix ANCHORS on the run's FIRST member; the rest keep fixless Info
   findings.** THE SCENARIO forcing a choice: three empties are three
   observations but one repair, and the report's fix side-table is per
   observation. Filing the one fix on the first member makes "site repaired"
   true for it (the byte becomes the surviving `\p`, which holds content) and
   makes the other two disappear in the SAME transaction — the oracle's
   condition 3 ("no code's count rises") covers them, and they are gone from the
   re-lint. Filing it on the LAST member would work identically for the oracle
   but reads backwards to a human clicking a diagnostic: the run starts at the
   first. Sound, high.
3. **The derivation stays at `Structure::finish`, and the recorded empties are
   SORTED by opening token there.** THE SCENARIO: a run is a contiguous slice
   only if the entries arrive in document order. They do today (a paragraph node
   closes when the next one displaces it), but the grouping now reads run
   adjacency off the SOURCE — "the token after this extent is the next recorded
   empty's opening marker" — so a future close-order change degrades to no fix
   rather than to a wrong extent. Finish-time is also what pass 5 entry 8 ruled
   for the fused-vs-staged oracle's byte-identical fix links, and the whole
   derivation moved inside that same loop, so the two paths still agree. Sound,
   high.
4. **`repaired()` in the fix tests now sends the FIXED slots only.** THE
   SCENARIO: the helper asserted every observation of a code carries a fix,
   which a chain deliberately breaks. Filtering is what the real dispatch does —
   `format::harvest` skips a fixless observation — so the helper now models it.
   `offers_fix` still proves a declaring row offers something. Sound, high.
5. **The corpus count does NOT move: still 25 of 787.** Expected movement, and
   there is none, which is itself the finding. Every run in the 226 books that
   is survived by a same-spelling paragraph holding content is ONE marker long
   (en_ulb ISA's `\q\n\q` pairs, en_ulb EZR's one `\p\n\p`, en_ult PSA's two
   `\q1\n\q1`); the multi-member runs that exist — eight `\p\n\p` and one
   `\b\n\b` — are all survived by a DIFFERENT spelling and were, and remain,
   refused. The pin's comment now says so, so the next reader does not read 25
   as "the chain work did nothing".

### Verification

Engine: `cargo test --workspace --release -- --include-ignored` green (336 lib
tests plus every corpus oracle, including `every_corpus_fix_passes_the_oracle`
and the format corpus's convergence-in-one invariant), `cargo clippy --workspace
--all-targets` clean, `cargo build --target wasm32-unknown-unknown -p onion-wasm`
clean. `onion-wasm/diagnostics.json` needed no regeneration and
`codegen_output_matches_input` confirms it — the row table is unchanged, only the
fix derivation behind it. debug/ dumps unchanged for the same reason: no corpus
book formats differently. Spike (`../onion-2-spike`): re-vendored via
`scripts/sync-engine.sh`, `npm run build` clean, `npm run probe:engine` 18/18 —
the 17 prior checks plus a new `format-empty-paragraph-chain` that formats the
book clean, appends Will's exact shape (`\p chain before` ␊ three empty `\p` ␊
`\p chain after`), asserts the preview reports exactly ONE edit, applies it once,
and asserts the tail collapsed to the two content paragraphs with a re-format
proposing zero.

## pass 13 — designator gate (2026-08-25, self-report)

`planning/sketches/designator-gate.md`, ruled sound. A `Designator` token now
requires a LEADING ASCII DIGIT; anything else after `\c `/`\v ` is ordinary
Text. The six-case table in the sketch ships byte for byte
(`scanner::tests::a_designator_token_requires_a_leading_digit`).

1. **THE GATE IS `\c`/`\v`'s ALONE — `\ca`/`\cp`/`\va`/`\vp` are exempt.** THE
   SCENARIO that forced it, five minutes in: `\cp M` (and `\vp א`, and the
   `\cp` in the USJ/USX/HTML unit tests) carries `Payload::Designator` too, and
   a published label is legitimately a letter. Gating on the payload enum alone
   deleted those tokens and dropped `pubnumber` from three exports. The gate
   therefore reads a second fact — is this row's payload a NUMBER — computed
   from `MarkerKind::{Chapter, Verse}` in `scanner::designator_gated`,
   precomputed into `Hot` for the fast arms and written into `ScanState`
   alongside `pending_payload` at every site that arms one. The sketch says
   "after `\c `/`\v `" and means it; the table was not touched. Sound, high.
2. **`\v \p` ALREADY gets a toc row today, so the banked "malformed `\v` still
   gets a row" ruling stands and toc.rs needed NO code change.** Verified before
   writing anything (`toc()` pushes a `VerseAnchor` on the `\v` MARKER token,
   numbers 0, `designator_span()` `None`), and now asserted as the unification:
   `a_verse_without_a_designator_is_one_row_shape` runs the identical assertions
   over `\v Then He declared` and `\v \p`. The `\v Then` row is a NEW row shape
   for nobody — it is the row `\v \p` has always produced.
3. **The new lint lane is `verse-without-designator`, and it RESYNCS the verse
   sequence; the chapter lane does not change.** THE SCENARIO: `\v 1` `\v Then
   He declared` `\v 3` used to be ONE finding (designator-malformed, which
   resyncs). Without a resync in the new lane the gate would have turned one
   typo into two findings — the absence, plus a verse-gap at `\v 3`. So the new
   code drops `prev_verse` and `first_verse_slot` exactly as the malformed arm
   does. `chapter-without-designator` keeps its existing non-resyncing behavior:
   the sketch asked for the verse counterpart, chapter sequence policy is not
   this pass's, and touching it would move pins for a reason nobody ruled on.
   The asymmetry is deliberate and recorded here rather than fixed silently.
4. **Adding a `Code` variant shifts every later discriminant, so
   `diagnostics.json` was regenerated.** That side-table is index-addressed and
   per-build wire data (a rule's durable identity is its kebab-case name), so
   this is the documented cost of inserting a row in category order rather than
   appending out of place. `codegen_output_matches_input` is the gate and is
   green; the spike re-vendored the JSON with the `.wasm`.
5. **Corpus pins moved by exactly one finding, sideways.** `designator-malformed`
   2 → 1, `verse-without-designator` 0 → 1, total unchanged. The mover is
   bdf_reg ACT 8:17, `\v +` — a bare note caller where the number belongs. `+`
   is not a digit, so no designator is carved and the `\v` names no verse, which
   is the same fact `\v \p` states. en_ulb ZEC 12:7 `\v 7"` STAYS malformed: a
   leading digit is the gate's whole test, and the interpreter still refuses the
   span. Those two are the only `\v `-not-a-digit sites in 226 books.
6. **`NUMBER_SHAPED`'s meaning sharpened to "the interpreter accepted a
   designator that is THERE", and the analyze test that pinned the old meaning
   was rewritten.** THE SCENARIO: the old test used `\v 3b` as its
   digit-start-but-unshaped case, but `3b` is a WELLFORMED verse (segment), so
   after the gate the test had no example left of "token present, flag clear".
   It now uses `\v 012` (leading zero — the interpreter's `[1-9]` law), beside
   `\v  Then He declared` which reports the same empty slot a bare `\v` does.
7. **Exports were verified, not assumed: `number=""` is now the only spelling.**
   `number="Then"` is unwritable — no designator token exists to carry it — and
   both projections lock it (`usj`: `{"type":"verse","marker":"v","number":""}`
   plus the prose as content; `usx`: `<verse number="" style="v" />`). The
   187/195 validated-pass oracles are unmoved; the one `testData` fixture with a
   `\v No number"` line is `<validated>fail</validated>` and was already
   excluded.
8. **Diff and format needed no code and no pin change.** The pass-6 divergence
   ruling stands verbatim (`\v 2"` starts with a digit, still tokenizes, still
   cuts a block at `GEN 1:0` with the `@N` tiebreak); the designator-less twin
   is now asserted beside it in the same test. `designator-ws-single` and
   `dedupe-verse-number` read `Designator` tokens and simply never fire on an
   absent one — the format corpus pins did not move.

### The spike: what scan.ts lost (the second acceptance criterion)

Both halves of the pass-9 phantom-caret patch are DELETED, not one:

- the anchor-authority-over-lines special case (`line.contentFrom =
  Math.min(verse.contentFrom, line.to)` and its paragraph of justification), and
- the `const empty = !anchor.numberShaped` collapse in front of it.

THE FINDING, since the brief asked: deleting only the first would have left the
disagreement alive for a designator that IS there and IS malformed. `\v 2"`
carves a token, so the LINES read reports content past it while the collapse
still pushed the verse row back onto the marker's end — the same one-token
disagreement, on the case the gate deliberately does not touch. With both gone,
the two reads agree BY CONSTRUCTION for every input: both read past the
designator token when there is one (`analyze::lines` and `analyze::slot` call
`content_after` on the SAME token) and both stop at the marker's content
boundary when there is not. `numberShaped` is no longer read anywhere in the
spike; it stays on the wire as the interpreter's verdict, for styling and lint,
never geometry. GAPS #16 and the pass-9 "NOT an ask" note are closed in
`GAPS.md` with that reading.

### Perf (go-slow law)

`playground --serial --iters 20`, en_ulb 66 books, max of 8, INTERLEAVED
before/after from two copies of the binary (a non-interleaved run first reported
a 25% "gain" that was pure machine state — worth recording as a measurement
trap): before 1252.5 MiB/s, after 1306.4 MiB/s. Noise in the predicted
direction of nothing; the gate is one byte-compare in an already-branchy arm.

### Verification

Engine: `cargo test -- --include-ignored` green (339 lib tests plus every corpus
oracle), `cargo clippy --all-targets` clean, `cargo build --target
wasm32-unknown-unknown -p onion-wasm` clean, `cargo test -p onion-wasm` green.
`src/experiments/fused.rs` mirrors the gate — it is verified token-for-token
against `lex` before any timing, so it is not optional. `onion-wasm/pkg-web` and
`pkg-bundler` rebuilt (committed artifacts) and `diagnostics.json` regenerated.
debug/ dumps unchanged: the PSA books they show have no `\v ` without a digit.
Spike (`../onion-2-spike`): re-vendored via `scripts/sync-engine.sh`, `tsc -b`
clean, and the FULL probe suite green — `probe:engine` 19/19 (18 prior plus the
new `empty-slot-takes-the-digit-before-the-space`, which deletes the digit of
`\v 9 The true Light`, clicks the rendered slot and types `9`, asserting the doc
reads `\v 9 The true Light` with the digit in front of the author's space),
`probe:cm` 11/11 (F2 `caret-reaches-empty-slot` GREEN — it was the pass's first
acceptance criterion), `probe:demo` 18/18, `probe:project` 9/9, `probe:stet`
5/5, `probe:bidi` 2/2.

## pass 14 — format_edits_in (2026-08-25, self-report)

`pub fn format_edits_in(source, range: Range<u32>, opts) -> Vec<Edit>` plus the
`formatEditsIn(text, fromUtf16, toUtf16, opts)` export. Spike-gaps ask 11 ("apply
formats cleanly within $scope"), which the spike works around today by filtering
a whole-book edit list in JS — silently dropping boundary-straddling edits and
re-running the whole computation per scope.

1. **The filter runs AFTER precedence, not before.** `Claims::resolve` settles
   which rule owns a contested byte over the WHOLE book; only then is the range
   applied. The scenario that forced it: a straddling claim that BLOCKS an
   in-range claim. Filtering first would let the blocked claim win the bytes, so
   the ranged list would contain an edit `format_edits` never proposes — and the
   spike's "ranged == whole-book filtered" mental model would be a lie. Post-
   resolve filtering makes the ranged list a SUBSET by construction, which is
   the property the corpus law pins.
2. **The claim slot rides out of `resolve`.** It now returns `(Code, slot, Edit)`
   and `format_edits`/`format_claims` drop what they do not need. Atomicity is
   only expressible with the group id: `bridge-empty-verses` emits the `1-3`
   write AND the deletion of the verses it swallowed, and a flat list cannot
   tell those two from two independent edits. A JS-side filter has exactly this
   blindness — it is the strongest reason the ask belongs engine-side.
3. **A pure insertion ON either edge is INSIDE.** `range.start <= from &&
   to <= range.end` says it in one line: a caret at the window's edge is in the
   window. The scenario: `marker-not-ws-preceded` inserts `\n` at the first byte
   of a `\q1` that opens a chapter — with the other rule the chapter-scoped
   format of that chapter would never propose its own opening break.
4. **An inverted range yields nothing; a zero-width one keeps insertions at its
   point.** Falls out of the same rule rather than being special-cased. A caller
   computing `chapter.start..chapter.start` gets the carets there and no spans,
   which is the honest reading of the window it asked for.
5. **IN-SCOPE IDEMPOTENCE IS WEAKER THAN THE BRIEF ASSUMED — the pass's one
   finding.** The brief asked for "apply the ranged transaction, re-run on the
   adjusted scope, get zero" as a flat law. It is FALSE when the window's edge
   cuts INTO a dropped straddler. Concretely: `\c 1\n\p \v 1 a\n\n\n\v 2 b \q1 c\n`
   with a window starting at byte 15, which is the middle of the `14..16`
   blank-line collapse. That collapse is dropped, the verse break at `16..17`
   is applied, and the leftover `\n\n ` is now a run whose collapse lies INSIDE
   the window — real work for a second pass. It converges (three passes on that
   fixture, `a_boundary_cut_through_a_straddler_converges_instead` pins the count
   and the settled bytes) and never oscillates, but it is not one-pass.
   SHIPPED AS: exact in-scope idempotence over a CLEAN boundary — one where no
   dropped edit reaches into the window — and convergence otherwise. A chapter
   span is clean that way, because its edges are marker boundaries and a
   straddler ends where the window begins; the JON corpus test proves it per
   chapter, and also that the per-chapter lists PARTITION the whole-book
   transaction there (nothing straddles at all). The `format_edits_in` doc says
   both, in those words.
6. **No chapter-scope sugar, and the doc says why.** The caller already holds the
   span (`Toc::chapters`, or the `chapters` read across the wall). A second entry
   point would only re-derive what the consumer just read.
7. **The wasm export translates the range on the SAME index the spans go out
   through.** One `Utf16Index` per call, used for `to_byte` on the way in and
   `to_utf16` on the way out — `wire_edits` is the extracted shared tail, so the
   two format exports cannot drift in how they encode an edit. Out-of-range
   `to` clamps (the index is total), so `formatEditsIn(text, from, 0xffffffff)`
   is a legal "from here to the end".

### Verification

Engine: `cargo test -- --include-ignored` green, `cargo clippy --all-targets`
clean, `cargo build --target wasm32-unknown-unknown -p onion-wasm` clean,
`cargo test -p onion-wasm` green, `wasm-pack test --node` green. Both committed
builds (`pkg-web`, `pkg-bundler`) rebuilt. New tests: five unit (straddle drop,
atomic-group drop, boundary insertion, clean-boundary idempotence via a real
chapter span, convergence over a cut straddler) plus the full-range equivalence
over every fixture; two corpus (the equivalence law on GEN/PSA/JON/MAT and over
seven arbitrary windows per book — where straddlers actually get dropped; and
JON's chapter spans against the whole-book list filtered, with per-chapter
idempotence). One wasm native test for the UTF-16 range translation and one node
boundary test over the Devanagari book. Corpus pins unmoved — nothing about the
whole-book transaction changed. debug/ dumps unchanged (no format bytes moved).
The spike is NOT touched: wiring `formatEditsIn` there is Will's.

## pass 15 — verse-bearing paragraphs + empty verse runs (2026-08-25, self-report)

Two ruled specs in one pass, sharing the ancestry/emptiness machinery:
`planning/ideas/candidates/verse-under-heading.md` (missing-paragraph consults
`V_FORBIDDEN_IN_PARAGRAPHS`) and `.../empty-verse-runs.md`
(verse-without-designator gains a fix for the unambiguous empty case).

1. **THE VERSE-BEARING PREDICATE SHIPS AS RULED, AND ITS ONLY OBSERVABLE
   CONSUMER IS `\qa` — THE PASS'S FINDING.** `on_node_open`'s para test is now
   `MarkerKind::Paragraph && !forbids_verse(marker_idx)`, TableCell unchanged,
   the close side untouched because it reads the same `IS_PARAGRAPH` bit the
   open computed. But of the 18 v-forbidden rows, only `\qa` and `\lit`
   actually HOLD a `\v` under today's context masks: `\s`, `\ip`, `\r`, `\cl`,
   `\sp`, `\ms`, `\mr`, `\sr`, `\sd`, `\cd`, `\mte`, `\sts`, `\rem`, `\iex` all
   DISPLACE the verse to the root (the CST probe is in the test comments), and
   `\cp`/`\pb` open no scope at all. So Will's demo document
   (`\s1 Hidden pieces…` ␊␊ `\v 1 Put…`) ALREADY fired missing-paragraph before
   this pass — the spec's "today it is silent" reading was of the counter, not
   of the walker that never lets the counter see the heading. The predicate is
   still right and still wanted (it is the rule stated where it belongs rather
   than as an accident of the masks), but the honest report is: it changes one
   marker's behavior today, `\qa`, plus `\lit`.
2. **CORPUS MOVEMENT FOR CHANGE 1: ZERO, MEASURED BOTH WAYS.** 5,434
   missing-paragraph before and after, per book as well as in total (a
   throwaway per-book diff over 226 books, run against two builds). The corpus
   holds 88 `\qa` — all Psalm 119 acrostic headings, in en_ult, en_ulb and
   examples.bsb — and every one is followed by `\q1 \v N`, a verse-bearing
   paragraph between the heading and the verse. `\lit` appears nowhere. The
   `\d` watch the spec asked for answers itself: `\d` is NOT in the rails'
   forbidden set, so the predicate leaves it counting; the `\sp` watch the same
   way (it is forbidden but is displaced, so it was already firing). The pin's
   comment now carries this, so the next reader does not read "unchanged" as
   "the change did nothing".
3. **NEEDS-WILL, flagged not buried: a bare `\c 1` ␊ `\d psalm` ␊ `\v 1 text`
   fires missing-paragraph, and always has.** `\d` is verse-bearing by the
   rails, but the walker displaces `\v` out of it exactly as it does out of
   `\s1`, so the finding comes from the CST, not from this pass. It looks
   spec-legitimate to fire (usfm.org does not list `\v` as valid in `\d`) and
   the corpus never hits it (`\q` always intervenes in PSA), so nothing was
   changed. If the walker's masks are ever the thing under review, this is the
   case to re-read.
4. **THE EMPTY GATE IS A LOOKAHEAD AT THE ABANDON EVENT, AND IT DECIDES THE
   RESYNC TOO — which is what makes the deletion safe.** THE SCENARIO that
   forced it: `\v 1 a` ␊ `\v` ␊ `\v 3 b`. Pass 13's lane RESYNCS on a
   designator-less verse, so the gap at `\v 3` is hidden; delete the empty `\v`
   and the gap appears — a fix handing back a new finding, which the oracle's
   condition 3 forbids and the codebase's law forbids outright. Rather than
   guard the fix against the sequence, the sequence now steps OVER an empty
   one: a designator-less `\v` with nothing but whitespace before the next
   verse/chapter/paragraph marker names no verse AND holds none, so it is a
   stray marker, not a mis-numbered verse, and `prev_verse`/`first_verse_slot`
   survive it. Pass 13 entry 3's resync stands for the case it was ruled on —
   `\v Then He declared`, where the verse IS there and only its number is
   missing. The corpus does not move (its one case, bdf_reg ACT 8:17 `\v +`,
   holds a caller and takes the resync path).
5. **THE RUN COLLAPSES UNDER ONE FIX ON ITS FIRST MEMBER, DERIVED AT
   `Ordering::finish` — pass 12's chain, verbatim.** The abandon event knows
   the boundary token of ITS emptiness but not whether the next empty verse
   follows, so the `(slot, marker, boundary)` triple is recorded and the runs
   are grouped at finish, with adjacency read off the SOURCE ("this member's
   boundary token IS the next member's marker"), exactly as the empty-paragraph
   chain reads it. One deletion spans the run's first marker to the displacer's
   first byte, which takes the markers and the line endings they sat alone on —
   pass 12's extent, expressed over tokens because a `\v` is a LEAF and has no
   `Cst::extent`. The other members keep fixless findings; the same transaction
   repairs them.
6. **A SECOND REFUSAL, forced by the same law: a run that is its paragraph's
   WHOLE content gets no fix.** THE SCENARIO: `\p \v \v` ␊ `\p b` would repair
   to `\p` ␊ `\p b`, handing back an `empty-paragraph` finding. The test is
   two-sided and cheap — the paragraph survives if the run runs INTO a verse
   (`\p \v \v 2 b`), or if anything but whitespace lies between the paragraph
   marker and the run's first `\v` (`\p a` ␊ `\v \v`). Empty-paragraph's own
   chain fix is the next transaction's business; a fix answers for its own
   site.
7. **THE INTERACTION COMPOSES — NEITHER FIX BLOCKS THE OTHER, and no
   precedence rule was needed.** THE WALKED CASE: `\s1 head` ␊ `\v \v 1 text`.
   missing-paragraph anchors on the run's first `\v` and inserts `\p\n` AT that
   byte (`from == to`); the empty-run fix DELETES `[that byte, the real \v)`.
   `Claims::resolve` accepts a claim whose `from >= claimed`, and a pure
   insertion advances `claimed` only to its own point, so both are seated: the
   output is `\s1 head` ␊ `\p \v 1 text` in ONE transaction, settled in one
   pass (the format tests' `formatted` helper proves the second pass proposes
   nothing). Row order would have given missing-paragraph the byte anyway, but
   it never had to — the two edits are disjoint by construction. The same shape
   over `\qa` is pinned beside it, since that is where the predicate rather
   than the walker raises the finding.
8. **The row becomes a DUAL CITIZEN (`formatter: true`) and declares a label,
   so `diagnostics.json` was regenerated.** No `Code` variant was added, so no
   discriminant moved; the only wire change is that row's `formatter`/`fixLabel`
   pair. `codegen_output_matches_input` is the gate and is green. The
   `a_fix_is_offered_exactly_where_the_row_declares_one` table gained its
   demonstration case (17 declaring rows now, and lint.rs's doc count — stale at
   "15 of the 43" since pass 13 — is corrected to 17 of the 44).
9. **A marker that is neither verse, chapter nor paragraph ends the emptiness
   as CONTENT, so no fix.** `\v \f + \ft note\f*`, `\v \tr`, `\v \s5` are all
   fixless. The spec names the trio and the trio is what shipped: a row-0
   pop-all or a table row is a displacement this fix does not model, and
   guessing there is how a formatter eats somebody's document.

### Perf (go-slow law)

The predicate adds one bit test on an L1-resident 24-byte table, per NODE open
(1.76M of them in en_ult), and the empty-verse lookahead runs on a FINDING, not
on a token. `playground --lint-only --serial --iters 8`, en_ulb 66 books, three
interleaved before/after pairs from two copies of the binary: 1122/1057,
1290/1267, 1253/1252 MiB/s. Noise with at most a ~1% lean, which is the
predicted size of one array read on the node-open path.

### Verification

Engine: `cargo test --workspace --release -- --include-ignored` green (432
tests: 352 lib plus every corpus oracle, including
`every_corpus_fix_passes_the_oracle` and the format corpus's convergence-in-one
invariant), `cargo clippy --workspace --all-targets` clean, `cargo build
--target wasm32-unknown-unknown -p onion-wasm` clean, `cargo test -p onion-wasm`
green, `wasm-pack test --node` green. `onion-wasm/diagnostics.json` regenerated
and both committed builds (`pkg-web`, `pkg-bundler`) rebuilt — the `.wasm`
behavior does change (a new lint fix crosses the wall). New tests: three
ancestry unit (the demo document, the `\qa` case the predicate owns, the
close-side symmetry over `\p` then `\s1`), four ordering unit (sequence
transparency, the run collapsing under one fix, both refusals, plus the fix
table's demonstration case), two format unit (the default bundle collapsing
`\v \v \v 1 text` in one apply and leaving `\v Then text` alone; the
missing-paragraph × empty-run interaction over `\s1` and `\qa`). Corpus pins
unmoved for both changes, with the reasons written INTO the pins. debug/ dumps
verified unchanged by regenerating all six and diffing the bodies and edit
counts (71/29/93 JON, 12/8/128 PSA). The spike (`../onion-2-spike`) is NOT
touched: re-vendoring is Will's.

## pass 15 review (2026-08-25, Will's rulings)

- **\d does NOT hold verses — RULED (Will), current behavior kept.**
  Evidence weighed: usfmtc NESTS the verse inside para[@style=d] (no
  \p synthesized; verified live) and usx.rng leaves \d verse-bearing;
  but the v.html prose page says body+poetry only, and the spec's own
  \d example interposes \q1 before \v 1 (`\d A Psalm of David…` ␊
  `\q1` ␊ `\v 1 O \nd Lord\nd*…`). USFM is underdefined here; Will
  rules with the prose page + example. Our walker's displacement and
  the missing-paragraph advisory on `\d text \v 1` are DELIBERATE
  divergence from usfmtc, recorded here. No context-mask change.
- Pass 15's two silent-spec rulings (empty-\v sequence-transparency;
  no fix when the run is the paragraph's whole content) — RULED OK.
- The "fix never hands back a new finding" law itself: Will questions
  whether it must be iron. Standing for now (it is what makes format's
  convergence-in-one oracle falsifiable and "Format document" a single
  trustworthy transaction); revisit if its conservatism ever blocks a
  fix worth more than the guarantee.

## pass 16 self-report (2026-08-26): the point terminator lands on its marker

One fix's anchor moved, Will's call after seeing it misfire in the
spike: `unterminated-milestone`'s `\*` was inserted at the RECOVERY
extent's content end — wherever the walker happened to recover — which
on `\ts-s` mid-verse filed three verses of prose inside the milestone
as attribute text. A point's span is only its attribute list (the
row's own comment), so the terminator now lands at `point_end`: the
end of the marker's trimmed span, or of its attribute list when one
follows. Container/Plain closers keep the extent-end anchor — their
content genuinely belongs inside.

Choice where the spec was silent: with the `\*` missing, a TRAILING
attribute list never lexes as AttrList (nothing terminates it) — it
decays to Text. `point_end` therefore recognizes the orphaned list by
its pipe FIRST BYTE on the token right after the marker (AttrList
proper still arrives in the self-terminating node-initial `|cat="x"|`
form). Known ambiguity, accepted: prose following attrs INSIDE that
same pipe-initial Text run (`\ts-s |sid="x" then prose`) is kept
inside the terminator, since where attrs end and prose begins is not
decidable without interpreting attribute syntax — the fix oracle
holds either way. Scenario that forced it: the existing
`\qt-s |who="Levi"` fix test, whose repaired spelling
(`…"Levi"\*`) is banked behavior this change must not regress.

Verification: `cargo test --lib lint`, `cargo test --test
lint_corpus` (fix oracle included), full default `cargo test`, and
`cargo clippy --all-targets` all green. New fix test: the swallowed-
prose case (`\ts-s swallowed prose` → `\ts-s\* swallowed prose`).
NOT rebuilt: onion-wasm pkgs (behavior crosses the wall — rebuild
when Will vendors next).

## pass 17 self-report (2026-08-26): duplicate-id and the paragraph ahead of \c

Two new lint lanes from planning/ideas/committed/positional-gaps.md
(Will's rulings recorded there), plus the walk now tells Flat whether
each leaf sits in a POSITIONAL context (for a node's own opener, its
parent's — read off the stamped CST, precomputed per frame as two
bools so the hot loop pays a select).

1. **`duplicate-id`** (Payload, Error, fixless): the FIRST `\id` is
   the identification, every later one is the finding with `second`
   pointing back (the numbering-mix shape). RULED (Will): never legal
   anywhere.
2. **`paragraph-before-first-chapter`** (Structure, Warning,
   fixless): a BODY or POETRY paragraph the band judge would have
   silently advanced into ChapterContent ahead of the first `\c`.

Choices where the spec was silent, and what forced each:

- **The band's container-abstention is now conditional — but for
  PARAGRAPH rows only.** First cut judged every container-carrying
  marker at positional positions; `\c 1 \ca 2\ca*` promptly flagged
  `marker-out-of-band`, because `\ca`'s context slice is WALKER
  MECHANICS (it keeps the frame poppable), not a positional license.
  Paragraph rows are the class whose masks state placement truthfully.
- **Scripture-book gate** (BOOK_CODES ahead of the FRT..NDX tail,
  read at the BookCode leaf): a top-level `\p` in FRT/GLO is legal
  PeripheralContent. `books::is_scripture_code` is new.
- **Deferred to a new `Flat::finish`**: "before the first chapter"
  is only a fact once the book shows it HAS one (the test-helper
  corpus of `\id`-prefixed snippets with no `\c` made every body
  paragraph light up). No `\c` → missing-chapter's territory; verses
  up there → the run is verse-before-first-chapter's finding, never
  both. One finding per book (the first offender), band advances so
  the rest stay quiet.
- **`seen_chapter` counts only a top-level `\c`**: the ancestry demo
  (`\p out\esb \p in \c 1 more\esbe`) has its one `\c` inside the
  sidebar — a swallowed `\c` is not the book reaching its chapters.
- **Section paragraphs (`\ms`, `\s`) before `\c 1` stay silent**:
  live corpus practice, spec-ambiguous — ruled open in the plan doc.

Verification: `cargo test --lib lint` (76), `cargo test --test
lint_corpus` — ZERO new corpus findings, no pins moved, fix oracle
green — `cargo clippy --all-targets` clean, full `--include-ignored`
suite run at pass end. diagnostics.json regenerated (two new rows).
NOT rebuilt: onion-wasm pkgs (the new codes cross the wall — rebuild
at next vendoring).

## pass 18 self-report (2026-08-26): token_spans obeys the one-delimiter rule

Will's editor probe caught two derivations of one fact disagreeing:
`token_spans` emitted raw scanner extents (the fold gives a marker
its WHOLE trailing whitespace run), while every curated read ends
chrome at label + ONE delimiter byte. `\v      Put` painted six
spaces as marker chrome in the source pane while `contentFrom` said
content starts at byte 16 — the first Left press appeared to do
nothing. Fixed at the EMIT, per the frame: `token_spans` now clips a
folding token's `to` through the same `content_after` helper; the
scanner and the Token vec are untouched (token extents stay the
partition oracle format and fixes splice against).

Choices where the spec was silent, and what forced each:

- **Which kinds clip: the five whose fold is a DELIMITER before
  content** — opening Marker, Milestone (both `folds_delimiter`
  kinds), and the three carved payloads (Designator, NoteCaller,
  BookCode — each takes its `ws_run_end` with it). Everything else is
  untouched: closers never absorb, Text whitespace is content, and
  `\b`-style SingleNewline rows fall out for free (`content_after`
  adds nothing when nothing was folded — same answer the curated
  reads give).
- **AttrList is NOT clipped, both forms.** Node-initial DOES fold
  (`absorbs_trailing_ws` re-arms the whitespace arm), but U25001 puts
  the `<HS>*` INSIDE the attribute_list production — list bytes, not
  a delimiter before content — and no curated read emits a
  `contentFrom` for it, so there is nothing to agree with. The
  trailing form settled it: its pre-closer whitespace
  (`|lemma="x"  \w*`) is genuine list bytes a trim would misreport.
  OPEN for Will: `\p|cat="x"|   text` still paints the folded run as
  markup — the caret symptom can recur on node-initial lists; ruling
  wanted on whether that form should clip too.
- **The clip filter now tests the EMITTED span, not the raw extent**:
  a viewport starting inside a clipped-away run would otherwise keep
  a row the whole-book read says ends before the viewport, breaking
  the corpus's subsequence law. The dropped bytes belong to no span
  either way.
- **The corpus tiling law weakened to exactly the new claim**: spans
  are sorted-disjoint and every gap (trailing gap included) is
  space/tab only — a folded run's remainder, checked against the
  bytes through `Utf16Index::to_byte`. `ref_tokens` re-derives the
  clip by the oracle's own route (`ref_content_from`, kind-gated).

Verification: `cargo test --lib analyze` (24, two new: the four-case
probe-agreement test asserting the last chrome span's `to` EQUALS the
lines read's `contentFrom`, and the milestone clip), `cargo test
--test analyze_corpus` (all books, laws + differential), full default
`cargo test`, `cargo test -- --include-ignored`, and `cargo clippy
--all-targets` all green. No corpus pins moved (the oracle is
differential, not pinned). NOT rebuilt: onion-wasm pkgs (the clipped
spans cross the wall — rebuild at next vendoring); wasm wrapper and
node test docs updated in source only.
