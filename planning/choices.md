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
