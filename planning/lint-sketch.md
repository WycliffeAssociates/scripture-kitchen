# Lint sketch (NOT APPROVED — pseudocode to react to)

Requested 2026-08-18 while Will was away from the machine. Nothing here is
ruled; it exists so the design conversation has something concrete to mark
up. Prior art: onion's lint (its CODES were mostly right — port the list,
not the machinery) and ideas/committed/linter.md (whose `push_token` funnel
is retired; that file still needs its edit when lint is built).

## Ruling reversal to record (Will, 2026-08-18)

Ordering DOES flag verse sequence problems. Earlier leaning was to abstain;
reversed: verses that are **non-contiguous, out of order, or duplicated**
are all findings — "no one of which is real Bible data." Severity may
differ per code (a gap is suspicious; a duplicate is broken), but all three
exist.

## Shapes

```rust
// CodeMirror's exact severity ladder — Diagnostic.severity is one of
// "error" | "warning" | "info" | "hint", and the session maps 1:1.
enum Severity { Error, Warning, Info, Hint }

enum Category {
    Structure,   // nesting, closers, barriers, recovery
    Ordering,    // chapter/verse sequence (the interpreter's lane)
    Attributes,  // attr lists: forms, terminators, unknown names
    Payload,     // designators, callers, book codes — malformed content
    Form,        // whitespace/delimiter shape, marker-not-ws-preceded
    Version,     // deprecated/removed forms keyed on \usfm
}

struct Observation {
    code: Code,            // u16 enum, one per finding kind
    anchor: u32,           // token index into the linted slice
    second: Option<u32>,   // the other party (opener for an orphan closer…)
    fix: Option<FixId>,    // index into LintReport.fixes, see below
}

struct LintReport {
    book: Option<u32>,     // BookCode token idx; None IS the missing-\id finding
    observations: Vec<Observation>,
    fixes: Vec<Fix>,       // side table, only for observations that have one
}

// The rules table (authored, like the marker table):
// code -> { category, default_severity, severity_by_version, template }
// Message RENDERING is the consumer's; messageParams audit vs onion still owed.
```

## The fix model (the open question, sketched)

Two candidate models; sketch recommends (A).

**(A) Fixes are byte-splice edit lists** — CM-native, engine-neutral:

```rust
struct Fix {
    label: &'static str,          // "insert \\f*", "renumber to 12"
    edits: Vec<Edit>,             // sorted, non-overlapping
}
struct Edit { from: u32, to: u32, insert: CompactString }  // byte offsets
```

- A CM `Diagnostic.action.apply(view, from, to)` just dispatches the edits
  (session converts byte→UTF-16). One transaction, one undo step.
- **Offered, never applied** (the probe's own rule). Never-synthesize
  governs TOKENS; a fix is proposed TEXT — once the user applies it, the
  bytes are real and the next re-lex tokenizes them honestly.
- "Fix all X" = concat every fix's edits for a code, sort, dispatch once —
  the cmQuestions §7 `\cl` panel pattern (lint list as work queue) falls
  out for free.
- Composability rule: a fix's edits must be computable from (source,
  tokens, cst) alone and must leave the partition re-lexable — enforced in
  tests by apply-then-relex-then-relint (the fixed finding must be GONE,
  and no new finding may appear: the "fix oracle").

**(B) Token-space transforms** (onion's model — mutate token objects, ship
back): rejected in sketch. We no longer ship objects; a transform would
need re-serialization we deleted, and CM applies text changes anyway. The
one thing (B) did well — "replace this whole note" granularity — (A) gets
by making the edit span the node's extent (the CST gives every node's
byte range; one Edit can swap an entire `\f…\f*`).

**UTF-16 (asked 2026-08-18): fixes ride the existing shim, no new
machinery.** Edits are BYTE offsets inside the engine; the session
converts `{from,to}` through the same `Utf16Index` it already uses for
every diagnostic range, and `insert` crosses as a STRING (natively UTF-16
on the JS side — no offset conversion exists for it). Applying a fix
comes back as an ordinary CM transaction through `apply_edits` in
pre-edit UTF-16, like any keystroke. If the index is right for
diagnostics it is right for fixes; tested once.

**Node-level reasoning still emits byte edits — three derivations, each
mechanical** (`Node.children` is an ARENA range, not a byte range):

    reasons over NODES   (reason/ctx verdicts)
      → addresses in TOKENS  (anchor = token index, via Node.token/children)
        → emits BYTES        (Edit.from/to = token.start/token.end())

"Insert `\f*` at the displacement boundary" = Recovery node → last child
id → that token's `end()`. "Replace this whole note" = the node's BYTE
EXTENT (first token's start → last descendant's end) — the same
derivation as the app's `content extents` session read. Owed when lint is
built: ONE `Cst::extent(node, tokens) -> Range<u32>` helper, shared by
fix generation and the session read.

**Format/prettify is the same machinery**: a formatter is a rule bundle
whose findings are all `Hint` + auto-fixable (normalize delimiter ws,
newline-before-marker, canonical attr spacing…). "Format document" =
apply every formatter fix in one transaction. No separate subsystem.

## Code list, v1 (port of onion's + what the CST already surfaces)

### Structure (read off `Node.reason` — a linear pass)
- `unclosed-note` — Recovery on a Note frame (3 already live in the
  corpus: en_ulb ISA/MRK, bsb GEN). Fix: insert `\f*`/`\x*` at the
  displacement boundary.
- `unclosed-char` — Recovery on a Character frame. Fix: insert `\X*`.
- `unclosed-at-eof` — Eof on a frame whose row wants a closer.
- `unterminated-container` — Recovery on a Container (version-keyed:
  optional 3.2 / required 4). Fix: insert `\list-e\*` before displacer.
- `unterminated-milestone` — Recovery/Eof on a Point (missing `\*`).
- `orphan-closer` — ClosingMarker leaf (no matching frame). Fix: delete.
- `orphan-terminator` — `\*` leaf. Fix: delete.
- `orphan-container-end` — `-e` point with no container in reach.
- `content-outside-sidebar-rule` — `\c` (etc.) leaf inside a Sidebar
  frame (the barrier kept it; the spec says it shouldn't be there).
- `unknown-marker` — row-0 Marker (also the pop-all recovery event).
- `nested-spelling-misuse` — `\+X` where the row isn't Character, or
  nesting depth says plain form belonged.
- `note-peer-explicitly-closed?` — NO finding: NotePeer/Implicit are silent.

### Ordering (tokens only; needs the verse-designator interpreter)
- `designator-malformed` — fails the spec VERSE pattern (`12text`, `?`).
- `verse-duplicate` / `verse-out-of-order` / `verse-gap` — per chapter,
  from leading integers; ranges (`12-14`) count as their span. Fix for
  duplicate/out-of-order: renumber to next expected (the probe's demo).
- `chapter-duplicate` / `chapter-out-of-order` / `chapter-gap`.
- `verse-before-first-chapter`, `missing-chapter` (book has none).
- `missing-verse-one`? (chapter starts at \v 2) — same gap machinery.

### Payload
- `missing-id` — `LintReport.book == None` (real: BSB Ecclesiastes).
- `book-code-unknown` / `book-code-not-uppercase` — vs the books aux
  table (authored, membership only — copy list from onion; STILL UNBUILT).
- `chapter-without-designator` — `\c` then no Designator token.
- `caller-shape` — note caller not in `+ - ?`-or-word set (Hint).

### Attributes (the four owed from linter.md, re-derived post-scan)
- `attr-trailing-form-deprecated` — AttrList span not ending `|`
  (version-keyed: deprecated 3.2, removed 4).
- `attr-both-lists` — two AttrLists adjacent to one marker.
- `attr-terminator-mismatch` — span shape (opened `|…` never re-piped).
- `attr-pipe-hint` — rung-3: Text containing `|` inside an attrs-capable
  node (Hint: "did you mean an attribute list?").
- `attr-unknown-name` — k/v interpreter vs `defined_attributes` (with the
  `a-*` prefix-wildcard matcher — the ONE place that learns the
  convention), `attr-missing-required` (e.g. vid ref), `attr-family-
  cardinality` (ta's "one or more").

### Form
- `marker-not-ws-preceded` — byte before a marker token is non-ws
  (`content\s1`; ruled 2026-08-18).
- `delimiter-shape` — the ws_after_name derivations (NBSP-after-name
  etc.; the Unicode-hs open question lands here).

## Two subsystems, one report

```text
lint_prepared(source, tokens, cst):
    o = vec![]
    structural: for node in cst.nodes[1..]: match node.reason …   // linear
    adjacency:  single token walk (attr rules, form rules, payload)
    ordering:   filter Designator/BookCode → interpreter → sequence check
    fixes computed per-rule beside the finding
    return LintReport { book, observations: o, fixes }
```

Suppressions: `{code, reference}` — content-derived addresses, per the
no-minted-identity law. Rules table + severity(version) fed by `\usfm`'s
adjacent Text.

## Open (to rule when back)
1. Fix model (A) confirmed, or something else?
2. Severity defaults per code (sketch: structure=Error, ordering
   duplicates=Error / gaps=Warning, attributes deprecated=version-keyed,
   form=Warning, hints=Hint).
3. Does `verse-gap` respect versification schemes (some traditions skip
   verses legitimately) — vref-aware later, plain-contiguity now?
4. messageParams audit vs onion (templates and their params).
5. Code numbering stability (u16 with reserved ranges per category?).
