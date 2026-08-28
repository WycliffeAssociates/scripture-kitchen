# Investigate later

Simplification leads that look real but haven't been checked. Each one's
GOAL IS NET DELETION — if the investigation ends with more code than it
started, the answer was no. Nothing here is committed to; nothing here
blocks anything.

## `ws_after_name` — does the scanner need it? (2026-08-13)

**What:** Check whether `\b` is the only marker where the delimiter-fold
column changes the scanner's answer (everything else is pure open/milestones-
fold shape); if lint can own that one report, delete `folds_delimiter`'s row
read, `Hot.folds`, and its `fused_plain` branch — the column itself stays,
lint still needs it.
**Why:** 151 rows, one counterexample, and `\b` takes no content anyway — a
likely real net deletion. Do after 5B, before further `HotIdx` surgery.

## Unicode whitespace where spec says `hs` (2026-08-17)

**What:** NBSP/NNBSP/ideographic-space/ZWSP after a marker name stay content
and never fold — matches spec (`hs` = space/tab only), and lint already has
what it needs from span-vs-name-length.
**Why:** Zero occurrences across all 226 corpus books, so nothing to weigh
against the spec reading. Closed until a real book shows a non-`hs`
whitespace in delimiter position.

## A `\w` fused arm (2026-08-13)

**What:** Aligned corpora (en_ult) are dominated by `\w`/`\zaln-s`, neither
with a fused arm, so the hottest path runs fully general-case. A dedicated
arm could fuse marker + delimiter + back-position for the whole word shape.
**Why:** 5B measured en_ult at 1017 MiB/s vs prose's ~1700, concentrated on
exactly this shape — but re-measure frequencies on an aligned corpus and run
the "one-load marker path" spike first, since that helps every marker.

## header_scan / ParseHeader / the TOC slab — one shape? (Will, 2026-08-19)

**What:** lint's `header_scan`, `ParseHeader`, and the sous TOC slab all read
the same book-header neighborhood; check whether they want a shared
low-level primitive.
**Why:** lint must stay ParseHeader-free (it's a consumer-side index), so
this only pays off if unification stays a primitive, not a dependency.

## Pinned-count tests lean on gitignored corpora (Will, 2026-08-20)

**What:** lint_corpus.rs, attr_corpus.rs, cst_oracle.rs etc. pin exact counts
against example-corpora/, which is gitignored — a fresh clone silently skips
them and proves less than it looks like it proves.
**Why:** Fine short-term (the numbers are derived and reconciled), but
eventually bundle a small stable subset or retarget a tier at committed
fixtures so a bare clone still exercises real-data pins.

## The k/v attribute rules read 31 MB byte-at-a-time (Will, 2026-08-20)

**What:** `Flat::read_attributes` walks every attribute list byte-at-a-time;
en_ult's 792,414 `\w` lists cost ~31 MB at ~0.85 GB/s (+5.7 ns/token on
`--lint-only`).
**Why:** Measured and accepted, not solved. Best lever: vectorize the
interior walk in `attributes.rs` (attr_corpus.rs is a ready-made oracle); a
whole-list pre-filter was tried and rejected — same byte-at-a-time cost.

## Chapter navigation grid (2026-08-21, from the Toc pass)

**What:** A tappable chapter grid needs `ChapterRow`'s `\c` token (already
carried) for raw/capped labels, plus a plural `chapter_spans(n)` for
duplicated chapter numbers.
**Why:** Pure client concern once a UI pulls it — build none of it before
then.

## Mask `text()` newline-as-space render option (2026-08-21)

**What:** A `text()` render option could substitute space-for-newline (both
one byte) for prettier aligned-corpus display, with `to_source` still
pointing at the real newline byte.
**Why:** Invariant only weakens to "byte-identical except space-for-`\n`" —
cheap, but build on pull, not before.

## hegel-rust for random-document property tests (2026-08-24)

**What:** Format's invariant 7 is exhaustion-checked, not randomized;
hegel-rust (Hypothesis-lineage, better shrinking than proptest) is the
candidate for the input-generation gap.
**Why:** Fails the boring bar today (beta, breaking-changes warning) — Will,
2026-08-24: "ok on hegel for now." Parked until 1.0, not chosen; candidates
then are format's random-documents gap and the diff port's three algebraic
laws.
