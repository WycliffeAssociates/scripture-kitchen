# Format / prettify (roadmap item 2)

Will's brief: "similar to usfm onion's format — remove extra line
breaks, move paras to their own linebreaks but remove them between
verses etc." Plus (2026-08-24 session): formatters are legitimately
configurable; format is OPT-IN and allowed to mutate/delete/invent
bytes (unlike the mask, which never normalizes); wholesale
marker-removal (the en_ulb `\s5` case) belongs IN format via a
parameterized row, not a third lane.

## Architecture (settled with Will, 2026-08-24): the Form channel

One rows table, three consumer surfaces, no formatter subsystem.

**The 5th channel.** The consumer-facing severity scale grows a
variant: Error / Warning / Info / Hint / **Form**. A Form row never
draws a diagnostic — `lint()` does not evaluate it at all — and exists
only for `format_edits()`. This is an explicit variant, NOT
`severity: None` overloaded (None + `escalation` already means
version-determined severity in the rows table). Clippy/rustfmt
analogy: shared walk infrastructure, two products; clippy doesn't
underline collapsible whitespace, rustfmt fixes it.

**Dual citizens keep the bit.** Rows that are genuinely both a
diagnostic and a format action (missing-paragraph, empty-paragraph)
keep their real severity and carry a `formatter: bool` — the bit is
what makes a one-click "fix as part of formatting" possible from the
diagnostic side too. For Form rows the channel IS the membership.

**Parameterized rows.** A row is a rule IDENTITY; its emit walk may
consult `FormatOptions` for targets (clippy precedent:
`disallowed_names` is one static lint whose names come from
clippy.toml). "Remove every `\s5`" = one `remove-marker` row whose
finding set is driven by `remove_markers: &["s5"]`; empty list
(default) = zero findings. Fixes flow through check_fixes / one
transaction / idempotence like everything else.

**Options over profiles.** No profile type. Per-rule bools plus a
small number of true axes:

```rust
pub struct FormatOptions<'a> {
    // axes
    pub verse_breaks: VerseBreaks,        // Keep | Remove — a CodeMirror
        // editor rendering line breaks wants Keep; flowing text wants
        // Remove. Default: Remove (the brief).
    pub char_marker_breaks: CharBreaks,   // Keep | Join — Join rewrites
        // the aligned-corpus one-\w-per-line newlines into real spaces
        // (en_ult). Resolves the mask sketch's NEEDS-USER cleanly: the
        // mask never normalizes, but format legitimately EDITS — pretty
        // aligned view = format with Join, then mask. Default: Keep.
    pub newline: Newline,                 // Lf | CrLf — the byte form
        // of every newline in the OUTPUT: inserted breaks use it and
        // existing endings normalize to it (RULED). Default: Lf.
    // parameterized rows
    pub remove_markers: &'a [&'a str],    // e.g. ["s5"]; default empty
    pub repairs: &'a [Code],              // lint fixes opted into the
        // transaction; default empty
    // per-rule bools — whitespace canon defaults TRUE, the uW-era
    // content rows default FALSE (dedupe_verse_number, bridge_empty_verses)
}
```

Presets, if ever, are documented constructors over this struct — same
no-presets-vs-options stance as the mask Filter.

**Surface sugar (RULED: both):**

```rust
pub fn format_edits(source: &[u8], opts: &FormatOptions) -> Vec<Edit>
    // the EditList — editor/CM session applies through the UTF-16 index
pub fn format(source: &[u8], opts: &FormatOptions) -> Vec<u8>
    // = edit::apply(source, &format_edits(source, opts)) — CLI/batch
```

## Lint interaction: two internal passes, zero duplicated rule logic

(Answering "are we duplicating lint?") No. `format_edits()` is:

1. **Call the existing `lint()` internally**, exactly as-is, and keep
   only the fixes of formatter-bit rows + `repairs`-allowlisted codes.
   Nothing re-implemented, no findings drawn — format is a consumer of
   lint's fixes, invoked internally so callers don't run a "lint
   first" protocol (which would also invite stale findings).
2. **Run the Form-row pass** — a separate walk over the same
   tokens/CST checking the whitespace/newline patterns lint never
   looks at (they're Form-channel; `lint()` skips them entirely).

Then merge both fix sets, sort, check_fixes, one transaction. Both
passes are per-call and ns/token per the crate's laws; diagnostics
consumers never see Form rows and never filter anything.

**Repairs allowlist** (`repairs: &[Code]`, opt-in memberships over
EXISTING fixes):

- `unclosed-note` / `unclosed-at-eof` ("insert the closer") — yes.
  The "unambiguous location" condition is inherited: where the fix
  derivation can't place a closer safely it already offers no fix.
- Renumbering (`verse-duplicate` renumber, gap fills) — NEVER (RULED:
  too much intent assumption). Not in any tier.

## The rule candidates, as parameterized rows

Will's rules of thumb (2026-08-24): all block-like markers start their
own line — poetry `\q#` is NOT special-cased, it's a block/para marker
like any other (its own line, containing its content — the "poetry
exemption" dissolves because the newline before each `\q` IS the
block-marker newline); whitespace runs reduce toward a single SPACE
(never tab/newline) at EDGES only; delimiters reduce to a single;
designator whitespace reduces to one; **interior verse-text whitespace
is never touched**.

| Form row | rule | params | fix shape | before → after |
|---|---|---|---|---|
| `block-marker-own-line` | every block-like marker starts its own line; `\v` is the configurable citizen | `verse_breaks` (Keep = `\v` joins the block list; Remove = delete the Newline before `\v` inside a paragraph); `newline` picks inserted bytes | insert `\n`/`\r\n` before a block marker not at line start | `\c 1\v 1 Text` → `\c 1` ␊ `\v 1 Text`; (Remove) `\p` ␊ `\v 1 a` ␊ `\v 2 b` → `\p \v 1 a \v 2 b` |
| `collapse-blank-lines` | vertical runs → one newline | — | delete Newline spans 2..N | ␊␊␊ `\c 2` → ␊ `\c 2` |
| `normalize-newlines` | every existing line ending rewrites to the configured form | `newline` | rewrite the ending's span | `a\r\nb` → `a\nb` (Lf) |
| `trim-text-edges` | collapse ws at a text run's EDGES (against markers/newlines) — editors write `[lots of leading ws]text`; the interior is content and untouched. NBSP is content, never collapsed (ruling line in rule doc) | — | rewrite the edge run to the canonical single space (or nothing at line start) | `\p   Text` → `\p Text`; `\v 1 In  the beginning` UNCHANGED |
| `delimiter-single` | the marker's own delimiter reduces to a single | — | rewrite the delimiter span (the marker token owns it post-fold) | `\p␣␣␣` → `\p␣` |
| `designator-ws-single` | ws around a designator payload (chapter/verse number) → one space | — | rewrite the span | `\v␣␣12␣␣Text` → `\v 12 Text` |
| `marker-ws-at-line-start` | no indentation before a line-leading marker | — | delete the leading-ws span | ␊ `   \v 1` → ␊ `\v 1` |
| `char-marker-line-join` | char-marker boundaries keep author breaks unless asked | `char_marker_breaks` (Join: newline abutting a char-marker boundary → space; the aligned-`\w` case) | rewrite `\n` span with `" "` | `\w In\w*` ␊ `\w the\w*` → `\w In\w* \w the\w*` |
| `remove-marker` | wholesale removal of a named marker's nodes | `remove_markers` | delete `Cst::extent`, empty insert | `\s5` ␊ `\p` … → `\p` … |
| `dedupe-verse-number` | text after `\v N` starting with the literal N again (ws-stripped) → splice the duplicate out. Happens nowhere legitimately in scripture (uW artifact) | bool, default FALSE (opt-in, RULED) | delete the duplicated number span + its ws | `\v 2 2 men went` → `\v 2 men went` |
| `bridge-empty-verses` | a run of EMPTY `\v N` markers bridges into the verse where text finally appears | bool, default FALSE (opt-in, RULED) | rewrite first designator to the range, delete the empty markers | `\v 1\v 2\v 3 asdf` → `\v 1-3 asdf` |

**Char-marker boundary law** (onion's real mechanism was
`is_protected_whitespace_boundary`; the earlier `with_original_spacing`
reference was wrong — no such thing exists): a char marker is followed
by space or TAGEND, and NO space is needed after a closing `*` — the
closer glues to following text legitimately (`\nd Lord\nd*'s
Battles`). Never insert a separator at a character/note/milestone
boundary; edge-collapse shrinks existing ws only.

**Dual citizens (existing rows, formatter bit):**

- `marker-not-ws-preceded` — already Hint + `\n` fix. Collision with
  `block-marker-own-line` (`text\p` vs `text \p`): one emit site, the
  zero-ws case keeps the old code, the ws-but-wrong case is the new
  row.
- `empty-paragraph` — Info; formatter membership adds a fix ONLY for
  the unambiguous case (RULED): consecutive IDENTICAL para markers
  where the first is empty (`\p\p` → `\p`) is clear duplication;
  mixed pairs (`\m\p`) are ambiguous about which was intended and get
  no format fix — the diagnostic still points, a human chooses.
- `missing-paragraph` — Warning + formatter bit (RULED): after `\c`,
  if no paragraph marker of any kind opens before the first `\v`, the
  fix inserts `\p` on its own line immediately before that `\v`
  (FixStr `\p` + newline, anchored to the first verse). In the
  default bundle.

**Not ported, with reasons:**

| onion rule | disposition |
|---|---|
| RecoverMalformedMarkers (`before\q1 after` as ONE text token → split) | MOOT here, not refused: onion formatted editor-mutated token VECTORS where a marker could be lost inside a text node; we take bytes and re-lex, and the scanner already tokenizes `\q1` mid-text as a marker. The state it repairs cannot exist. |
| RemoveOrphanEmptyVerse (`\v 5\v 6 content`, v5 empty → drop v5) | not ported (RULED): ambiguous which verse the content belongs to — force the human to choose. Note `bridge-empty-verses` (opt-in) covers the run-of-empties shape differently, without deleting a verse identity. |
| RemoveBridgeVerseEnumerators (`\v 2-3 2. foo 3. bar` → strip `2.`/`3.`) | not ported (RULED): content rewrite riding on bridging. |
| MoveChapterLabelAfterChapterMarker | not ported YET (RULED): `\cl` before chapter 1 means the book-wide "Chapter" label; `\cl` after a `\c` means that chapter's specific label. A blind move changes meaning; doing it right needs the positional semantics — maybe one day. |

Not inherited because onion never had them: BOM handling, table
formatting. New design if ever wanted; a future BOM lint code can join
the repairs tier.

## Invariants (the readable layer — each stated checkably)

What must ALWAYS be true of format's output, regardless of options.
Every one is either an assert in the code, a corpus test, or a proptest
— none is prose-only.

1. **Idempotence.** `format(format(x), opts) == format(x, opts)` for
   every opts. Checked: apply once, re-walk, zero findings among Form
   rows + formatter-bit + opted repairs (next section).
2. **Interior verse text is untouched.** No edit's span intersects the
   interior (non-edge bytes) of a Text token inside a verse extent,
   except rows the caller explicitly opted into (`dedupe-verse-number`,
   `remove-marker` extents). Checked: default-bundle corpus run diffs
   only whitespace/marker-boundary bytes — `verse_text` mask output of
   input and output are byte-identical modulo the newline/space forms
   the opts request.
3. **Format never invents content.** Every inserted byte is whitespace
   or an engine FixStr from an existing lint fix (`\p`, a closer) —
   never document prose. Checked: the Edit list's inserts are
   enumerable by construction; a test asserts every insert matches the
   whitelist.
4. **One transaction, no overlaps.** The full edit set passes
   check_fixes (sorted, disjoint, in-bounds) before any apply.
   Checked: existing check_fixes machinery, corpus-wide.
5. **Meaning-bearing diagnostics are conserved.** For every
   non-formatter lint code, finding COUNT before == after on the
   default bundle (formatting never creates or destroys a real
   problem). Checked: corpus assertion over all 226 books.
6. **Determinism.** Same bytes + same opts → byte-identical output,
   no environment, time, or iteration-order dependence. Checked:
   double-run equality in the corpus test.
7. **Options are total.** Every combination of axes/bools/params
   yields a valid transaction (possibly empty) — no panicking
   combinations, no order-dependent option interactions. Checked:
   proptest over random FormatOptions against fixture inputs.

Attack surface (same readable-layer duty): input is arbitrary bytes
(same contract as lint — malformed UTF-8, junk markers, 4.5MB books);
output is bytes + an Edit list; format touches NOTHING else — no I/O,
no state, no allocation retained across calls. It runs after the same
scanner/CST the rest of the crate trusts; a malformed document formats
to a malformed-but-tidier document, never an error.

## The idempotence oracle

Onion asserted NO idempotence anywhere (verified) — this clause is new
strength, not parity:

    format(format(x)) == format(x)
    — concretely: apply the whole transaction once, re-walk, assert
    ZERO findings among Form rows + formatter-bit + opted repairs
    (stronger than fixpoint-in-2).

Overlapping-edit REJECTION in check_fixes guards the composed
dispatch. Rules whose edits collide (two rows touching one Newline)
yield ONE owner: precedence = row order, first-writer wins, the second
row's finding isn't emitted on a claimed span (dedup at emit).

## Tests (plain English)

- Each Form row: minimal snippet → exactly its finding + fix; apply →
  re-walk → zero Form findings (per-rule idempotence).
- Verse-text interior sanctity: `\v 1 In  the beginning` survives the
  full default bundle byte-identical; edge ws (`\p   Text`) collapses.
- Poetry falls out of the block rule: a `\q1`/`\q2` stanza formats to
  one marker per line; `verse_breaks: Remove` still leaves every `\q`
  on its own line.
- Both `verse_breaks` values over one fixture: outputs differ ONLY in
  verse-break bytes.
- `newline: CrLf`: every break in the output is `\r\n`, inserted and
  pre-existing alike (normalization ruled in); `Lf` symmetrically.
- Char closer gluing: `\nd Lord\nd*'s Battles` survives byte-identical
  — no separator ever inserted after a closing `*`.
- `char_marker_breaks: Join` on an en_ult chunk: one-word-per-line
  becomes flowing text; mask verse_text on the RESULT reads clean.
- `remove_markers: ["s5"]` over en_ulb: every `\s5` extent gone,
  nothing else changes; empty list emits zero findings.
- `dedupe-verse-number` (opted in): `\v 2 2 men went` → `\v 2 men
  went`; `\v 2 2000 men` UNTOUCHED (number-boundary, not prefix);
  default-off leaves both alone.
- `bridge-empty-verses` (opted in): `\v 1\v 2\v 3 asdf` → `\v 1-3
  asdf`; default-off leaves it (lint still flags).
- empty-paragraph nuance: `\p\p text` → `\p text`; `\m\p text`
  UNTOUCHED by format (diagnostic remains).
- missing-paragraph: `\c 1\v 1 In…` → `\c 1` ␊ `\p` ␊ `\v 1 In…`.
- Repairs: truncated `\f` + `repairs: [UnclosedNote]` gets the closer
  in the same transaction; without, untouched.
- The en_ulb `\s5` stress block: empty-dup deletion + blank-line
  collapse in one transaction; converges in one pass.
- Bundle over all 226 books: convergence-in-one, partition holds, and
  no non-formatter finding's count changes (ordering counts identical
  before/after).

## Resolved (2026-08-24)

- `remove-marker` safety rail: NO — lint on the output is the rail;
  format never promises the result is cleaner than what was asked for.
- `newline` scope: NORMALIZE — a `normalize-newlines` Form row rewrites
  every existing ending to the configured form; mixed endings are
  exactly the "form" this feature exists for.
- Block-like classification for `block-marker-own-line`: derive from
  the tables codegen rows' macro-level category ONLY. No authored
  marker list, no comparison against onion's sets — this is an
  evolution, not a pure port; our table is the authority.
