# Next steps

The OPEN QUEUE only: what to write next and the settled design it embodies.
Implementation history is not kept here — code and doc comments are the
record, git is the log. Vocabulary: GLOSSARY.md. Lint/braid designs:
ideas/committed/. Unproven perf leads: investigate-later.md.

## Where we are (2026-08-18)

The scanner is COMPLETE (~1.7 GiB/s prose, ~1.0 aligned): all token kinds,
the marker table + codegen, fused fast arms, attribute lists (U25001
node-initial and legacy trailing), the three carved payloads (designator,
note caller, `\id` book code), and `ParseHeader` as its own pass
(src/parse_header.rs). Green: `tests/partition_oracle.rs` (226 books,
`concat(spans) == source`), `tests/fast_path_identity.rs`,
`tests/parse_header_oracle.rs`.

Nothing else consumes tokens yet. The editor direction is CodeMirror
(byte/position-native — see "Settled facts"), which changed nothing in the
engine queue but deleted several once-planned pieces (external-token
restamping, id plumbing, any event stream, any envelope adapter).

## Next code 1: `cst::build`

`cst::build(&[Token]) -> Cst` — pure function of stamped tokens (stamping
is the scanner's; tokens are always born stamped), no source needed.
Build order: the frame stack + pop predicate first with the oracle, no
consumers; the positional-context lane after.

### Types

    struct Node {
        token: u32,            // opening marker's token index
        children: Range<u32>,  // into Cst.child_ids, NOT into nodes
        reason: u8,            // CloseReason — the walker's verdict
        ctx: u8,               // stamped context (computed for the pop
                               //   predicate anyway — hand it forward)
    }                          // 14 bytes → pads to 16; two spare
    struct Cst { nodes: Vec<Node>, child_ids: Vec<u32> }

    enum CloseReason {  // (collapsed 2026-08-18: the discriminator is the
                        //  POPPED row's own `closing` column, applied at
                        //  pop time — no frame-kind checks at lint time)
        Explicit,  // its own closer arrived (\x*, \*, \esbe, -e)
        Implicit,  // ended BY DESIGN: the popped row never (paragraphs)
                   //   or optionally (note peers) closes explicitly —
                   //   the next block/sibling ending it IS the grammar.
                   //   Lint-silent. Includes paragraphs dying under a
                   //   pop-all (their death is normal-shaped; the
                   //   unknown marker itself is the finding).
        Recovery,  // ended FOR SANITY: the row expected an explicit
                   //   closer and did not get one — \c hitting an open
                   //   footnote, an unclosed \add, everything usfmtc
                   //   papers over; also unclosed-frame casualties of an
                   //   unknown-marker pop-all. Always a finding.
        Eof,       // input ended with the frame open — finding iff the
                   //   row wanted a closer (paragraph@EOF silent,
                   //   footnote@EOF finding)
    }

- **Children ids are tagged u32s**: high bit 0 = token id, 1 = node id.
  Children lists carry the CONTENT — leaf tokens are IN `children`
  (`[text, node, text]` mixed), or they'd be lost. An `AttrList` token
  stays a child id; attributes never fuse into markers.
- **A node's id is its index in `nodes`** — no id field, no assignment
  step. Per-build, never persisted.
- **Root node RESERVED at `nodes[0]`** (it closes last; reserve the slot
  at build start, patch once at the end). `token = u32::MAX` — a
  container in a projection structure, not a synthesized token.
- **`reason` is the structural-lint interface** — lint READS it, never
  re-derives "was that close okay". Non-negotiable. The displacing token
  is derivable: first token after the node's last child id.

### Build mechanics (three Vecs total, no per-node/per-frame allocation)

One shared SCRATCH stack: proper nesting means the open frame's pending
children are always a contiguous TAIL of it. Leaves and closed-child ids
push on; at close, flush `scratch[frame.mark..]` to the arena, truncate,
push the closed node's own id. Frames carry just `mark: u32` for this.
`child_ids` final length is exactly `tokens.len() + nodes.len() - 1` —
pre-reserve. Each node's range is stamped AT ITS OWN CLOSE at the arena
write cursor, so the arena is in close order — fine, since every read
enters through a node's explicit range, and document order comes from ONE
`Cst::in_order()` iterator (depth-first through children lists), shared
by the oracle and every in-order consumer.

Do NOT try `children: Range<u32>` into the node vec itself: close order
interleaves a child's descendants between siblings (`P(A(a) B(b))` closes
`a A b B P`; A and B are never adjacent in any linear order). The id
arena exists to sidestep exactly that.

### Oracle + tests

Lifted partition oracle over the 226 books: every token id appears in
exactly ONE children list, exactly once; `in_order()` recovers
`0..tokens.len()` in order. Same spirit as `concat(spans) == source`,
one level up. Unit tests pin each CloseReason and each walker rule below.

### Walker internals (settled; "walker" = the module's internal loop)

- **One generic loop**: frames are stamped with context at push time
  (`frame.ctx = row.contributes_context() or inherited`; Character frames
  are transparent). On a marker:
  `pop_while(top frame's stamped ctx ∉ row's context_mask)`, then push if
  `opens_scope`. That single predicate reproduces every displacement:
  `\c` unwinding a note and a paragraph, `\p` closing the previous `\p`,
  `\pb` leaving its paragraph alone, nested `\add`.
- **Dead ends, do not re-derive** (tombstones in schema.rs): a PARENTS
  table and a rank/`precedence()` were both built and deleted — the
  stamped-context predicate does their job.
- **Two hand rules** (~10 lines each): `\X*` searches the stack for the
  Note|Character frame with matching `marker_idx`, pops through;
  `\*`/`\esbe` pops the topmost frame of the kind it closes.
- **Two data-keyed clauses**: `ScopeKind::Sidebar` is a POP BARRIER (only
  `\esbe` closes it; `\c` inside `\esb` stops and lints); an incoming
  `closing == OptionalExplicitUntilNoteEnd` marker first pops an open
  frame of that same class (the 19 real note peers, column-marked).
- **Only rows that DISPLACE run the pop predicate** — derived boolean
  (`opens_scope.is_some() || kind ∈ {Chapter, Verse}`). The mask serves
  legality AND displacement; they diverge exactly on the empty-mask
  adjacency rows (`ca` must never pop the stack).
- **Milestone spelling overrides the table FOR UNKNOWN ROWS ONLY**:
  lexer-shaped `\name-s/-e` with row 0 is scope-kind Milestone (pairs an
  unknown `\zaln-s` with its `\*`). Known rows: `category` picks the
  frame — that is how `\table-s`/`\list-s` open their containers
  (`MilestoneTable`/`MilestoneList`; see `ScopeKind::List`'s doc
  comment). The walker must not nest two frames when `\table-s` is
  followed by `\tr` (both paths: synthesize on `\tr`, accept explicit).
  The U25003 closure rule ("closing milestone required before anything
  that would otherwise end the list/table") is a pop barrier + lint
  event, same shape as Sidebar; requirement level is version-keyed.
- **Displacement pops stamp `Implicit` or `Recovery` by the popped row's
  `closing` column** — sanity closes are lint events, by-design closes
  are silent, decided at pop time.
- **Unknown/illegal markers recover by popping ALL frames** — keyed on
  row 0 (`ScopeKind::Unknown`).
- The walker is LINE-BLIND: a mid-line `\s1` displaces identically (the
  para railroad allows horizontal-ws-only before paragraph markers).

### Positional context (the second lane, after the stack works)

- The positional band (`Scripture → … → ChapterContent`) is MONOTONIC,
  no new data: ordering = enum declaration order, transitions =
  `allowed_contexts`. Stay if current allowed, else advance to the lowest
  allowed context above current, else it's behind us → lint, don't move.
  Two instructions: `mask & !((1 << (cur+1)) - 1)`, `trailing_zeros()`.
  No marker "enables" regions; `\id` needs no special case.
- Markers listing two positional contexts (`mt#`, `cl`, `ip`) resolve by
  lowest-above-current; `cl`'s dual semantics falls out.
- `ca`/`cp`/`va`/`vp` are adjacency lint rules, not context questions —
  empty context slice, the machine abstains.
- The lane's exact encoding (per-token sidecar vs stamped on nodes) and
  the rules-table shape stay open until the stack exists, then get
  tested against real rules. Node has spare layout room.

## Next code 2: lint

Entry points (tokens are always born stamped; there is no external-token
door — the editor sends TEXT and every transaction re-lexes):

    pub fn lint_prepared(source: &[u8], tokens: &[Token], cst: &Cst)
        -> LintReport                       // the worker
    pub fn lint(source: &[u8]) -> LintReport // sugar: lex → build → worker

    struct LintReport {
        book: Option<u32>,  // BookCode token idx; None IS the missing-\id
                            //   finding (real: BSB Ecclesiastes), never a crash
        observations: Vec<Observation>,
    }
    // Observation: { code, anchor: u32, second: Option<u32> } — anchors are
    // token indices, per-build; severity/category/template live in a rules
    // table; message rendering is the consumer's. Audit onion's
    // messageParams before finalizing.

Internal invariant to document on `lint`: it never reorders, inserts, or
drops tokens (the session's token→span→UTF-16 mapping relies on it).

Two subsystems:

1. **Structural** — largely a linear `match` over `Node.reason` (the CST
   is a flat vec; no recursion): `Recovery` always a finding, `Eof` a
   finding iff the row wanted a closer, `Explicit`/`Implicit` silent.
   Owed findings beyond that: the four attribute ones
   per ideas/committed/linter.md (deprecated trailing form = AttrList
   span not ending in `|`; both-lists = two AttrLists adjacent to one
   marker; mismatched terminator = span shape; rung-3 hint = Text
   containing `|` inside an attrs-capable node); displaced/EOF/recovery
   closes; a `\c` with no designator (ParseHeader records an empty label
   and judges nothing); **marker not preceded by whitespace** (the para
   railroad requires `\n`/ws before a marker; `content\s1` still lexes
   as a marker — byte before the marker token is non-ws → finding).
   Lint is a PASS, the scanner's `push_token` funnel is retired —
   linter.md still says funnel, EDIT IT when building. Escape hatch: a
   finding proven un-re-derivable rides the scanner individually; the
   general funnel does not come back.
2. **Ordering** — tokens only, ignores the CST: filter
   `Designator`/`BookCode` kinds, run the verse-designator interpreter
   per span, compare across the sequence. Needs the interpreter (spec
   `VERSE` pattern `/[1-9][0-9]*[\p{L}\p{Mn}]*(‏?[-,][0-9]+[\p{L}\p{Mn}]*)*/`
   — pure text rules, zero table; its doc comment IS the comparison
   rules vref needs later) and the books aux table (valid codes only,
   membership — copy the list from onion).

Neither lint nor `cst::build` takes `ParseHeader` — it is a
consumer-side index; if ordering lint wants chapter runs it computes
them. Lint suppressions (when they come) key `{code, reference}` —
content-derived, per the no-minted-identity law.

## Standing laws

- **Partition oracle is not negotiable.** A change that wants to break
  `concat(spans) == source` is a design event — stop and log it.
- **Rows change only through the spec-diff**
  (`planning/spec_contexts_diff.py`, needs a tcdocs clone). The MARKER
  PAGE is the referee; spec fuzziness is absorbed by LINT SEVERITY,
  never by inventing table values.
- **Never synthesize tokens.** Flag, never repair. Every place the
  reference implementation normalizes is a place we lint.
- **Normalization is never the lexer's.** Spans keep their bytes exactly;
  trimming happens when a consumer asks for values.
- **Go slow** (Will, 2026-08-10): one behavior at a time, each behind the
  oracle, each with its perf delta read before the next. Deltas under
  ~15% need MAX-of-8 runs and a re-measured baseline in the same window.

## Settled facts that constrain later work (one line each)

- **Editor direction is CodeMirror 6** (spike live and promising): the
  text buffer is truth — the engine's own model. No Lezer grammar EVER
  (a second parser = drift), no LSP for the app (in-process wasm), no CM
  `Language` (nothing consumes `syntaxTree()`; escape = implement
  lezer's `Parser` interface over the session if that ever changes).
- **wasm surface = a `BookSession` handle**: owns `{bytes, tokens, cst,
  header, utf16 index}`; `apply_edits` takes a transaction's changes in
  pre-edit UTF-16 coordinates, splices back-to-front, ONE re-lex per
  transaction; reads are typed arrays of UTF-16 numbers (`token_spans`,
  `blocks`, `chapters`, `note_extents`, `text_runs`, `diagnostics`).
  JS never holds a token; no JS twin, no binary sidecar.
- **UTF-16 is a boundary index** (byte↔UTF-16 drift breakpoints, binary
  search both ways; rust-analyzer's `line-index` is the precedent) —
  never an eager conversion, never in the core.
- **No JS table**: the table's JUDGMENTS cross stamped on ranges —
  category + `payload_kind` (2 bits) + `closing` (2 bits) per marker
  range. NOTE `Category` has 26 variants (> 4 bits): either two bytes,
  or — likely right — ship a COARSER class axis (para/char/note/
  milestone/chapter-verse/sidebar, 3 bits); the app's shape inference
  needs only the class, and per-marker CSS comes from the marker NAME in
  the document itself. Decide at session-build time; no table/codegen
  change either way. Command pick-lists cross as codegen'd constants
  from the rows.
- **The editing model is the APP's** (cmWysiwyg's TEXT/FIELD/ATOM/CHROME/
  BREAK vocabulary never enters the library). The library ships neutral
  judgments + node CONTENT EXTENTS; the app infers intent around hidden
  markers from them. Sufficiency is proven (segment kinds ≈ TokenKind
  rename; its four shapes = table columns; anchors = adjacency + node
  extents). App-side wrinkle: segments may be FINER than tokens (a
  delimiter space split off a Text span) — views may subdivide, tokens
  stay truth.
- **No minted identity, ever**: per-build array indices internally,
  content-derived addresses (book code, chapter ordinal, verse
  reference) for anything durable, `ChangeDesc.mapPos` in-session.
  Nothing can drift from the bytes, so there is nothing an id would
  protect.
- **Ownership**: no owned twin — offsets only; the bundle
  (`Document/BookSession { source, tokens, cst }`) answers who holds the
  buffer; owned strings exist only where serialization allocates anyway.
- **Version data lives in LINT's rules table**
  (`severity(code, declared_version)`, fed by `\usfm`): no version
  column on `MarkerRow`; the decisive fact (trailing lists deprecated
  3.2 / removed 4) is owned by no row. 46 spare row bits if ever truly
  unavoidable.
- **Exports are folds over the CST** (USJ/USX/HTML): codegen'd
  name-mapping (official USJ names — check the schema in usfm-grammar),
  the attribute interpreter for k/v splatting (the lossy step), plus
  per-format quirks that never feed back (content→attribute markers
  project onto the ENCLOSING element; USX `eid`s are derived during
  iteration — never-synthesize governs tokens, not projections; HTML
  wrappers + kind-keyed NoteCaller rendering; heading base levels are an
  authored aux table). usx.md holds the six content→attribute target
  names still to be read off their marker pages. vref is the ordering
  lane's sibling, not a fold.
- **The attribute interpreter** (k/v over an AttrList span, default
  attribute, comma/colon splits) is the designator interpreter's
  sibling — pure span → judgment, shared with exports/lint.
- **`\z` custom markers are CONFIG-provided** (markers.ext shape; row 0
  already gives zero-behavior-unconfigured). Config must handle `a-*`
  prefix wildcards and the `standalone` category (no new Category
  variant needed).
- **`assign_marker_indices` is PARKED, unbuilt** — only caller would be
  a persistence path deserializing rows across a table-version change;
  ~10 lines on the scanner's resolver if that day comes.
- **Perf rules**: stop density is the wall — speed = emitting fewer
  tokens; arms stay `#[inline(always)]`; hand a found needle forward.
  A second pass over token rows is cheap (~1 ns/token floor, measured on
  ParseHeader; `playground --parse-header-only` prices any pass), so the
  CST and lint never need to fuse into the scan. Chapter-par only pays
  on big books. Criterion only when two real alternatives exist.
  One-load SWAR marker-path spike: see investigate-later.md.

## Parked (do not start)

Braid — now a SYNC layer, much smaller than onion's (hold the byte
buffer, apply mapped changes, schedule re-lex, own the UTF-16 index,
eventually chapter-scope the re-lex; id-stability-across-edits no longer
exists as a problem). Multi-book project format, diff/merge (onion's
diff is trusted — expect a nearly wholesale port), publish, anchors/
U25002/`aid`.

Large-piece order (Will, 2026-08-17): CST + lint → exports → diff port →
wasm/BookSession (pure Rust until then) → braid last.
