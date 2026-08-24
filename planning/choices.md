# Choices ledger

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
