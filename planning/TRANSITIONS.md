# Transitions: can the context machine be table-encoded?

Answers the question recorded in NEXT-STEPS.md:

> Can context-machine TRANSITIONS (what pushes/pops/implicitly closes) be fully
> table-encoded, or do a few hand rules remain around the table?

**Referee for every spec fact: USFM 3.2** (<https://docs.usfm.bible/usfm/3.2/>),
the latest spec, with no version column and no version-tracking anywhere in the
table (ruled round 4: multi-version support "gets gnarly, not interested").
Where 3.2 deprecates something, that is what the `deprecated` flag and
`AttrStatus::Deprecated` record. The 3.1 USX grammar in tcdocs/usx.rng is
fallback evidence only — good for confirming a marker exists or reading
cardinality the prose omits, never a vote against a 3.2 page.

Method: read what onion actually *implements* (not what its types suggest),
inventory every distinct behavior, then ask of each one "is this a column?".
Sources: `planning/onion_reference/marker_defs.rs`, and read-only in
`/Users/willkelly/Documents/Work/Code/usfm_onion/src/`: `walker/mod.rs` (the
machine), `parse/mod.rs`, `lexer.rs`, `export_tree.rs`.

## Ruling ledger (NEXT-STEPS.md step 2, 2026-08-10)

| # | Proposal | Status |
|---|---|---|
| [A] | `opens_scope: Option<ScopeKind>` column; precedence in a ~13-row auxiliary table keyed on scope kind | **SETTLED** — column landed in `schema.rs` |
| [B] | `closes_scope` column, retiring the `\esbe` phantom frame | **SETTLED** — column landed |
| [C] | `contributes_context` as an authored column | **RESTRUCTURED** — see §2.E. The collapse was accretion; `kind` × `category` replaces it and the value is now a derived `const fn` |
| [F] | unresolved/custom marker recovery | **SETTLED, CHANGED from onion** — pop all the way out, not down to the nearest structural parent (§6) |
| [H] | precedence encoding: onion's fixed 3-pass vs one generic `pop_while` | **INVESTIGATED — §3 is the answer.** Recommendation: one `parents` mask + one composed predicate |
| [I] | note recovery folds into the generic driver via a push-time context stamp | **SETTLED** — and it *downgrades* this document's earlier claim: with the stamp, recovery is an instance of the generic driver, **not a hand rule** (§2.E, §3) |
| [J] | `\X*` name-matched pop and `\*` kind-matched pop | **ACCEPTED as the only two hand rules** |
| [L] | lint must be correct over `Vec<Token>` alone | **STANDING CONSTRAINT** — §5 |
| [M] | fused single pass, layering inside the pass | **OVERRIDES this document's earlier §4 deferral** — §5 |

Round-3 rulings (2026-08-10), all applied:

| Subject | Status |
|---|---|
| `qt` overload | **SETTLED** — lexical shape is authoritative; encoding is **option (b)**, rows keyed by (name, `SpellingShape`). §7 is the comparison |
| `\fig` scope | **CONFIRMED** `opens_scope: Some(Character)` — the deviation from onion stands |
| paired-milestone `sid`/`eid` | **CONFIRMED** on the row; start/end narrowing stays on the span |
| `\xt` `link-href` | **RESTORED, marked deprecated as of 3.1** — per-attribute deprecation is an open encoding question |
| `\z` customs / index 0 | **SETTLED** — index 0 is the generic empty row; a `zaln-s` fast check needs no row, so the earlier `zaln` flag is RETIRED (§6) |
| `cp`/`ca`/`va`/`vp` | **CONFIRMED** — payload-takers that open no scope; now asserted by a test |
| `p1`/`p2` | **CONFIRMED** `ParaPeripheral` (USFM 3.1.1 moved them to PeriphPara) |
| `b`, `cl`, `ta`, `ipc`, `id`, `lit`, `rq`, `tr`, `cat` | **CONFIRMED** as filed; `cl` gained the `BookChapterLabel` context |
| `t-s`/`t-e` | **DELETED as errata.** Present in tcdocs/usx.rng `Milestone.style.enum` lines 1868-1869 with `sid?`/`eid?` — so this is a decision, not an oversight. But 3.2 documents exactly five milestones (list, table, qt#, ts, vid) and has no `ms/t.html`, and under the 3.2-only policy the grammar entry does not survive |
| `priority` | **MEASURED**, replacing the guesses — see planning/marker-frequencies.md |

Round-6 rulings (2026-08-10), all applied:

| Subject | Status |
|---|---|
| [A] explicit Unicode (U25004) | **RULED option (a)** — text-arm escape fold, no new TokenKind; USV pattern beats any marker claim; lowercase hex is a lint flag. Plus a new lint fact, "marker cased wrong": uppercase in marker names is tolerated as DATA and reported by lint, so the name scan is NOT tightened |
| [B] attributes (U25001) | **RULED both forms, one `AttrList` kind.** Trailing form needs the stack; **node-initial needs zero scope state**; lint flags trailing as deprecated. A pipe TERMINATES a marker name universally — our interpretation of an inconsistent proposal |
| [C] `html_element` | **RULED and POPULATED.** Column stays in the main table (Q-H7); u4→u5 (32 slots); `Ruby` added; milestones = self-closing span; `b` = span; heading levels via the `HEADING_BASE_LEVEL` aux side table |
| [D] `otherpara` | **EXTRACTED and RESTRUCTURED.** The rail groups do not align with `Category` at all, so the rule is now a MARKER set, `V_FORBIDDEN_IN_PARAGRAPHS`. Closes Q14 |
| [E] cap sweep | **DONE.** All 22 numbered families verified against their own 3.2 page; no row needed changing. Closes Q15 |
| [F] `list`/`table` milestones | sid/eid added as Optional — **Will's judgment, not a page** (both pages are silent) |
| [G] category ledger | all 20 rulings **CONFIRMED CORRECT** and dated; the eyeball item is closed |

Round-5 rulings (2026-08-10), all applied:

| Subject | Status |
|---|---|
| `wj-s`/`wj-e` | **DELETED — ruled char-only**, same errata class as `t-s`/`t-e` and on the same evidence (§7.1's table). `Category::MilestoneWj` removed with it. Closes Q11. `SpellingShape` is back to one user (`qt`); the mechanism stays |
| `imte` | cap **1–2** confirmed per the 3.2 revisions ("The variable # (1-2)") |
| `liv` | **`Numbering::Unbounded`** — 3.2's page shows `\liv1` and states no maximum; character marker with `\liv …\liv*`, as onion already had it |
| `\v` in `otherpara`/`sectionpara` | **LINT RULE** — keyed on the enclosing paragraph's category, which no context mask can express. The frame already carries `marker_idx`, so the category is one array read: no new frame field. `otherpara` has no `Category` counterpart — flagged |
| `markers.ext` futures | recorded as doc-notes: 3.2 adds `*` attribute wildcards (`a-*`) and a `standalone` category |
| U25002 anchors | **PARKED** |
| U25001 attributes · U25004 explicit Unicode | **ANALYSED** in planning/attributes-3.2.md — both approved for 3.2, both change the scanner ARMS, neither touches this table |

Round-4 rulings (2026-08-10), all applied:

| Subject | Status |
|---|---|
| version policy | **SETTLED** — conform to 3.2, no version column. Closes the version flag and Q8 |
| `t-s`/`t-e` | **DELETED** — errata, with the rng citation kept so it reads as a decision (closes Q7) |
| `wj-s`/`wj-e` | **ADDED** as a `MilestoneOnly` row — but the 3.2 verification came back negative, so the row rests on the 3.1 grammar alone. §7 records the tension |
| `vid` | **FIXED** — standalone, `ref` required + default, `h` optional, no sid/eid (ms/vid.html). Closes Q9 |
| attribute referee | **3.2 per-marker pages** — three residual divergences from the [N] shorthand, all resolved in 3.2's favour (§7) |
| `AttrStatus` | **ENCODED** — `defined_attributes` is now `&[(&str, AttrStatus)]`. Closes Q10 |
| `ParaPeripheral` | **KEPT** — the Para-prefix convention wins |

---

## 0. Two findings that reframed the question

**Finding 1 — the transition rules are keyed by SCOPE KIND, not by marker.**
Onion's `apply_open_precedence` matches on the *incoming scope kind* (13
values) and never on the marker name. So the bulk of the machine is not marker
table columns at all: it is a small **auxiliary table keyed on `ScopeKind`**,
and the marker table's only job is the column that maps marker → scope kind.
Ruled in as [A].

**Finding 2 — `StructuralScopeKind::closes_unclosed_note` is DEAD CODE.**
Defined at `marker_defs.rs:249` with a careful "single source of truth" doc
comment; the only other mention in the entire onion tree is a *doc comment* in
`walker/mod.rs:126`. Nothing calls it. The walker replaced it with
`marker_needs_note_recovery` (a finer, context-validity test — §2.E). Its
declared set `{Block, Chapter, Verse, Sidebar, TableRow, TableCell, Header,
Periph}` is a coarse approximation that is no longer what onion does. **Do not
port it**, and the NEXT-STEPS note that cited it as evidence for
"mostly-table-with-exceptions" should be considered withdrawn.

---

## 1. `opens_scope` — SETTLED [A]

Onion derives its `StructuralScopeKind` per marker in `structural_marker_info`:
a 1:1 rename for 12 of the 13 `MarkerDefKind`s (except `Figure`→`Block`), and
for `Paragraph` a **marker-name prefix cascade**
(`fast_paragraph_structural_info` → `is_list_marker_name` /
`is_section_marker_name` / `is_para_marker_name` /
`is_non_inline_paragraph_marker_name`): ~90 lines of `matches!` blocks with
`starts_with` fallbacks. The comment on `is_section_marker_name` records one of
its bugs — `starts_with('s')` swept in `sc`, `sig`, `sls`, `sup`, `sts`.

That cascade is now data: `opens_scope`, authored from `kind` × `category`.
155 visible values replace ~90 lines of name guessing, and a wrong row is one
readable line where a wrong `starts_with` is invisible.

`opens_scope` is not redundant with `kind`, and after ruling [C] it is not
redundant with `category` either — it is what the MACHINE does, where kind and
category are what the SPEC says. They diverge exactly where behavior and
taxonomy disagree:

- `\pb` — kind Character, category `CharBreaks`, `opens_scope: None`. This is
  the ruling's load-bearing example, and it is where onion is wrong: onion files
  `\pb` as a Paragraph, so an incoming `\pb` closes the paragraph it sits inside.
- `\esbe` — `opens_scope: None`, `closes_scope: Some(Sidebar)` [B].
- `\fig` — kind Figure, `opens_scope: Some(Character)`. **A deliberate deviation
  from onion**, which maps Figure→Block and therefore closes the enclosing
  paragraph on an inline, `\fig*`-closed element. Flagged for confirmation.

---

## 2. Inventory of every transition behavior onion implements

### A. Open — the ordinary case

Every resolvable marker pushes one frame of its scope kind. **Column:
`opens_scope`.** Table-encodable, no rule.

### B. Open — `\c` / `\v` with no number

`handle_marker`: if the scope kind is Chapter or Verse **and the next token is
not a `Number`**, the marker opens nothing and is a leaf. `chapter_segments`
repeats the same test to find chapter boundaries. For onion this costs one token
of lookahead — hence the whole `WalkableToken::next_is_number` trait method, two
`walk` entry points to resolve it, and a default-`false` footgun for foreign
token streams.

**In our design this is not lookahead and not a hand rule.** The `payload`
column already says `\c`/`\v` consume a `NumberRange`, and the scanner's
pending-payload mode already decides whether a payload token materializes. So
the rule becomes a table-driven statement about *when* the push happens:

> A row with `payload != Payload::None` opens its scope on the PAYLOAD token,
> not on the marker token.

One `if` on a column the table already has, in the same arm that pushes. It
reads as data, not as a special case, and it removes onion's lookahead
machinery entirely.

### C. Open — unresolved / custom markers

See §6 — ruling [F] changed this behavior, so it gets its own section.

### D. Close — implicit, by an incoming open

Onion's `apply_open_precedence`, verbatim as three sequential passes:

| incoming | pass 1 (pop while in) | pass 2 (pop while in) | pass 3 (pop until in) |
|---|---|---|---|
| Chapter | — | — | *pop all* |
| Header, Meta, Periph | — | — | *pop all* |
| Sidebar | — | — | `{Chapter}` |
| Verse | inline\*, Verse | Header, Meta | — |
| Block | inline\*, Verse | Block, TableCell, TableRow, Header, Meta | `{Chapter, Periph, Sidebar}` |
| TableRow | inline\*, Verse | TableCell, TableRow, Block | `{Chapter, Periph, Sidebar}` |
| TableCell | inline\*, Verse | TableCell, Block | `{TableRow, Chapter, Periph, Sidebar}` |
| Note, Character, Milestone | *(no structural pops — §E)* | | |

`inline*` = `is_inline_scope` = `{Note, Character, Milestone}`.

As prose: pass 1 clears inline junk and the current verse, pass 2 clears
same-or-lower-level block siblings, pass 3 unwinds to the nearest legitimate
structural parent.

The **chapter row is "pop all"** — the same claim as the slot model in
NEXT-STEPS ("no block state legitimately crosses `\c`") and as the standing
`--chunked` verify. Two independent lines of evidence for one rule.

Whether this is three rules or one is exactly question [H]. **§3.**

### E. Close — note recovery — RESTRUCTURED by [C] + [I]

When a `Note | Character | Milestone` opens, onion does no structural pops.
Instead it loops `while marker_needs_note_recovery(incoming) { pop }`, and that
predicate is:

1. `effective_context()` — **walk the stack top-down** and return the first
   frame that contributes a context. Per frame kind: `Note` → the frame's
   `note_context`; `TableRow`/`TableCell` → `Table`; `Block` → its
   `inline_context` mapped, or `ChapterContent` if none; `Chapter` →
   `ChapterContent`; `Periph` → `PeripheralContent`; `Sidebar` → `Sidebar`;
   `Header`/`Meta` → `Scripture`; `Verse`/`Character`/`Milestone`/`Unknown` →
   **transparent, keep walking**.
2. If that context is `Footnote` or `CrossReference` **and** the incoming
   marker's effective-context mask lacks that bit → pop.

So a footnote survives `\ft`, `\fq`, `\+nd`, `\w` (all legal in a note) and is
force-closed the instant a marker arrives that isn't — recovering a missing
`\f*` at the finest possible granularity.

**What this document proposed, and what the rulings did to it.** The earlier
draft proposed collapsing onion's `note_context` + `inline_context` + the
mapping into one authored column, `contributes_context`. Ruling [C] rejected
that: the collapse tidied the accretion instead of removing it. The spec's own
two levels — `kind` × `category` — carry all four of onion's fields, and the
context a frame contributes is *computable from them*. It is now
`schema::contributes_context(kind, category)`, a `const fn` codegen bakes into
the packed row. Not a column, not four columns: a derivation.

**And ruling [I] retires the stack walk.** Stamp the resolved context onto each
frame at push time:

```rust
frame.ctx = contributes_context(row).unwrap_or(parent.ctx)
```

Now step 1 is a top-of-stack field read and step 2 is one mask AND. The earlier
draft called this "a legitimate optimization that makes the hand rule O(1) but
does not remove it". **That was wrong, and [I] is right:** once the walk is
gone, what remains is `pop_while(<predicate over the top frame and the incoming
row>)` — which is precisely the shape of the structural pops in §D. It is not a
hand rule; it is a second instance of the one driver. §3 shows the composition.

### F. Close — explicit `\X*` — ACCEPTED HAND RULE [J]

Search the stack from the top for a `Note | Character` frame **whose marker
equals** the closer's; pop everything above it, pop the match, then emit a
second event for the closing token. Unmatched closers fall through for lint.

A keyed search over a bounded but arbitrary stack depth. In our design the
comparison is an **integer** (`marker_idx` off the token) rather than a string,
but the search remains. ~10 lines. **Hand rule 1.**

### G. Close — `\*` / `\esbe` — ACCEPTED HAND RULE [J]

`\*` pops the topmost `Milestone` frame, name-agnostic. With [B], `\esbe` uses
the *same* code path against `closes_scope`, so the two are one rule: "pop the
topmost frame of the kind this token closes". ~8 lines. **Hand rule 2.**

This is what retires onion's `\esbe` wart. Onion files `esbe` as a Sidebar
*open*, so it pushes a meaningless frame, and `export_tree.rs:298-325` has to
push a phantom container, discard its children, and retroactively rewrite the
previous sidebar's `close_index` (`stamp_last_sidebar_close_index`, line 611) —
~40 lines across two files, plus a `BlockBehavior::SidebarEnd` variant that
exists only to identify it.

### H. Lexical shape beats the table

Two places where the *spelling* overrides the row, both deliberate:

- `handle_milestone` **forces** `scope_kind = Milestone` for any token the lexer
  shaped as a milestone (`\\[a-z]+[0-9]*-[se]`), whatever the table says. This
  is what makes an unknown `\zaln-s` pair with its `\*`.
- the `+` prefix (`\+nd`) marks a nested character marker, stripped before any
  lookup and recorded on the occurrence.

Not table facts and not driver rules: they are the scanner's classification,
which NEXT-STEPS step 2 already makes authoritative (`TokenKind` + the
`NESTED_BIT`). Ruling [G] adds the third: the milestone SIDE is read off the
span, so a milestone family is one row.

One consequence of [G] needed its own ruling: `\qt …\qt*` is a character marker
and `\qt3-s` is a milestone, and both strip to `qt`. **Ruled: lexical shape is
authoritative** (maximal munch already takes the longest form), and the encoding
is rows keyed by (name, shape) rather than one row plus an override — **§7**.

### I. Leave reasons

Onion's `LeaveReason` (`Explicit · RecoveryClosure · EndOfInput ·
ImplicitByOpen`) is a pure function of (popped frame's kind, why we're popping).
Plus `WalkBoundary::BeforeChapter`, which exists only so a chapter-parallel walk
produces the same reasons as a whole-book walk.

Under [L] this stops being a walker field at all: lint's context must arrive as
a token-stream property or an **explicit emission**, so "note closed without
`\f*` at token N" is a diagnostic the driver emits, not a reason code a consumer
inspects. See §5.

---

## 3. [H] — is the 3-pass real, or a fossil?

**Verdict: a fossil.** The three passes are three instances of one predicate
family, and for every incoming kind they are provably equivalent to a single
`pop_while`. Here is the argument, then three options.

### 3.1 The proof

Write `S` for pass 3's stop-set and `P₁`, `P₂` for the pass 1 and pass 2
pop-sets. Read off the §D table:

| incoming | P₁ ∪ P₂ | S | disjoint? |
|---|---|---|---|
| Block | Note, Char, Milestone, Verse, Block, TableCell, TableRow, Header, Meta | Chapter, Periph, Sidebar | yes |
| TableRow | Note, Char, Milestone, Verse, TableCell, TableRow, Block | Chapter, Periph, Sidebar | yes |
| TableCell | Note, Char, Milestone, Verse, TableCell, Block | TableRow, Chapter, Periph, Sidebar | yes |
| Sidebar | ∅ | Chapter | yes |
| Chapter, Header, Meta, Periph | ∅ | ∅ (pop all) | yes |

Two observations do all the work:

1. **All three passes pop from the top.** So the machine's only real question is
   *where does the unwinding stop?*
2. **`P₁ ∪ P₂` is disjoint from `S` in every row.** So neither pass 1 nor pass 2
   can ever pop a frame that pass 3 would have stopped at, and pass 3 cannot
   stop before reaching a frame in `S`. Both formulations therefore stop at the
   **first frame from the top whose kind is in `S`** — the same frame.

The only way they could differ is a frame kind that is in neither `P₁ ∪ P₂` nor
`S`: pass 3 alone would pop it, the three passes would not. Across all rows
that kind is exactly `Unknown` — and `Unknown` **is never on the stack**
(`handle_unknown_marker` pushes no frame; ruling [F] keeps it that way). So for
every incoming kind with a pass 3, **pass 3 alone is the whole rule** and passes
1 and 2 are dead weight.

`Verse` is the one row with no pass 3, so it needs its own argument. Its rule is
"pop `{inline, Verse}`, then pop `{Header, Meta}`", which as a single set is
"pop while the top is in `{Note, Char, Milestone, Verse, Header, Meta}`" — i.e.
stop at `{Block, TableRow, TableCell, Chapter, Periph, Sidebar}`, which is
exactly a stop-set. Sequential and single-set differ only if a `{Header, Meta}`
frame sits **above** an inline or Verse frame (then the sequential form clears
the Header and stops, where the single-set form keeps going). Unreachable:
`Header` and `Meta` both pop-all on open, so such a frame is always the
bottom-most on the stack and can never sit above anything. This one step is a
*reachability* argument rather than set algebra, so it is the step the
differential test must cover.

### 3.2 What the passes were probably for

The decomposition reads like a hand-written unwind: "first the inline stuff,
then my siblings, then up to my parent." That is a good way to *think* about it
and a redundant way to *encode* it — pass 3 already expresses "up to my parent",
and everything below a parent is by definition poppable.

Two things the 3-pass does NOT buy, both worth ruling out explicitly:

- **Ordering around the push.** Every pop in `apply_open_precedence` happens
  *before* the push, in every arm; there is no pop-after-open anywhere. So no
  sequencing is lost by using one `pop_while`.
- **Event order.** Frames pop top-down in both formulations and each frame's
  event is derived from that frame, so the emitted event sequence is identical.

### 3.3 Option A — keep onion's 3-pass, as data

```rust
struct Precedence { pass1_pop: ScopeSet, pass2_pop: ScopeSet, pass3_stop: Option<ScopeSet> }
static PRECEDENCE: [Precedence; 13] = [ /* 13 rows × 3 masks */ ];

fn open(&mut self, row: &Row, tok: TokenIdx) {
    let Some(incoming) = row.opens_scope else { return };
    let p = &PRECEDENCE[incoming as usize];
    self.pop_while(|f| p.pass1_pop.has(f.kind));
    self.pop_while(|f| p.pass2_pop.has(f.kind));
    if let Some(stop) = p.pass3_stop {
        self.pop_while(|f| !stop.has(f.kind));
    }
    self.push(self.frame(row, incoming, tok));
}
```

~20 lines of driver, 13×3 masks ≈ 78 bytes. Faithful to onion, which makes the
differential test trivially green.

Against it: **39 masks to audit, of which 26 are provably inert**, and three
masks per row that must be kept mutually consistent forever. It also encodes a
shape nobody can justify, which is the thing the audit is supposed to be
removing. And note recovery does not fit it — it would stay a separate loop.

### 3.4 Option B — one `parents` mask, one composed predicate *(recommended)*

The auxiliary table becomes one mask per scope kind: **which kinds are a legal
PARENT for me.** Opening a scope unwinds until it finds one.

```rust
/// Auxiliary precedence table [A]: for each ScopeKind, the kinds that may
/// legally CONTAIN it. Opening a scope unwinds the stack until the top is one
/// of these. 13 × u16 = 26 bytes, the whole precedence machine.
static PARENTS: [ScopeSet; 13] = scope_sets![
    Unknown   => {},                                        // [F] pop all the way out
    Chapter   => {},                                        // top level
    Header    => {}, Meta => {}, Periph => {},
    Sidebar   => { Chapter },
    Block     => { Chapter, Periph, Sidebar },
    TableRow  => { Chapter, Periph, Sidebar },
    TableCell => { TableRow, Chapter, Periph, Sidebar },
    Verse     => { Block, TableRow, TableCell, Chapter, Periph, Sidebar },
    Note      => ALL, Character => ALL, Milestone => ALL,   // never displace
];

fn open(&mut self, row: &Row, tok: TokenIdx) {
    // `\pb` [C] and unconfigured `\z` [F] open nothing and displace nothing.
    let Some(incoming) = row.opens_scope else { return };
    let parents = PARENTS[incoming as usize];

    // ONE driver call. Two clauses, both reading only the TOP frame:
    //  - structural: this frame cannot legally contain the incoming scope
    //  - recovery [I]: this frame's stamped context forbids the incoming marker
    self.pop_while(|f| !parents.has(f.kind) || f.forbids(row.context_mask));

    if incoming != ScopeKind::Unknown {
        self.push(Frame {
            kind: incoming,
            marker_idx: row.idx,
            token: tok,
            // [I] stamp the effective context at push time. No stack walk, ever.
            ctx: row.contributes_context().unwrap_or(self.top_ctx()),
        });
    }
}

impl Frame {
    /// [I] recovery: a note frame force-closes when the incoming marker is not
    /// legal inside it. Deliberately NARROW to note contexts — see below.
    fn forbids(&self, incoming: ContextMask) -> bool {
        self.ctx.is_note() && !incoming.has(self.ctx)
    }
}
```

**~14 lines, 26 bytes of table, and note recovery is inside it** — no separate
loop, no stack walk, no `effective_context()`.

Why the OR-composition is safe, and the one condition on it: the two clauses are
mutually exclusive in practice. For `Note`/`Character`/`Milestone` incomings
`parents` is ALL, so the first clause is always false and only recovery fires.
For structural incomings, a note frame is already popped by the first clause
(`Note ∉ parents(Block)`), and the frames that *survive* clause 1 —
Chapter/Periph/Sidebar/TableRow/Block — never carry a note context, so clause 2
is always false. **This holds only because `forbids` is narrow to note
contexts.** If it is ever generalized to full context legality, the OR would
start popping legal structural parents. That constraint belongs in a comment on
`forbids`, and in a test.

Against it: the equivalence is an argument (§3.1), not an identity, so it must
be pinned by a differential test against onion over both corpora — which we want
anyway. And `Unknown` needs the one-line skip-push.

### 3.5 Option C — put the mask on the marker row

Rejected on sight, recorded so it stays rejected: `parents` is keyed on scope
kind, so putting it on marker rows is 155 copies of 13 distinct values, a wrong
row is invisible, and it contradicts [A] ("precedence in an auxiliary table, not
on marker rows").

### 3.6 Recommendation and the hand-rule count

**Option B.** It is smaller (26 bytes vs 78), the driver is shorter (~14 lines
vs ~20 + a separate recovery loop), it absorbs note recovery instead of sitting
beside it, and — the real reason — its single mask means something a human can
check against the spec: *what may contain this?* Three masks per row mean
nothing anybody can state.

Effect on the hand rules:

| | Option A | Option B |
|---|---|---|
| structural pops | driver | driver |
| note recovery | separate loop (~14 lines) | **same driver call** |
| numberless `\c`/`\v` (§2.B) | payload column | payload column |
| `\X*` name-matched pop [J] | hand rule, ~10 | hand rule, ~10 |
| `\*` / `\esbe` kind-matched pop [J] | hand rule, ~8 | hand rule, ~8 |
| leave reasons | ~5 lines | emissions, §5 |
| **total hand rules** | 2 (+ a recovery loop beside the driver) | **exactly 2, ~18 lines** |

So: **table + the two hand rules [J] already accepted, ~18 lines.** Down from
this document's first pass ("6 hand rules, ~45 lines") — the rulings did that,
not new cleverness: [C]+[I] absorbed recovery, [E]/[K] absorbed the
`\c`/`\v` lookahead into the payload column, [B] merged the two closers, and
[L] turned leave reasons into emissions.

---

## 4. Verdict

**Every marker-keyed and kind-keyed fact is data; the stack discipline is ~18
lines of driver, and it is exactly the two rules [J] accepted.**

The data:

- the 155-row marker table, with `opens_scope` [A], `closes_scope` [B], and
  `kind` × `category` [C] (from which `contributes_context` is derived, not
  stored);
- one **13-entry, 26-byte** `PARENTS` table keyed on `ScopeKind` (§3.4) — the
  whole of onion's 120-line `apply_open_precedence`.

The rules: `\X*` name-matched pop, `\*`/`\esbe` kind-matched pop. Both read
deeper than the top of the stack, which is what a stack machine IS; no per-row
value can express "search downward", and contorting the table to try would be
bigger, less readable, and still need the loop.

Roughly **250 lines of onion** (the ~90-line name cascade, the 120-line
precedence match, the three-way context derivation, the `\esbe` phantom)
**become 155 rows + 13 masks + 2 derived `const fn`s.**

---

## 5. Where the machine lives — [M] and [L]

**This document's earlier §4 recommendation is OVERRIDDEN.** It argued that the
scanner's own need for a stack is narrow (just "is a character marker or
milestone open?" for `AttrList` fusion), so the transitions machine should be
deferred to a walker layer built later, above the scanner.

Ruling [M] rejects the deferral. The vision is a **fused single pass** producing
a valid / recovered / diagnostics-aware result in ONE traversal. Layering
survives *inside* the pass, not as separate passes over the same bytes:

- **arms own position** — only the boundary finders advance the cursor
  (NEXT-STEPS step 2, unchanged);
- **the walker owns policy** — `PARENTS`, the frame stack, recovery, emissions;
- **the walker is listener-gated** — with no listener attached the plain-tokens
  path is untouched, which is what keeps it usable as the oracle for everything
  else.

So the machine belongs in the pass from the start, and the columns land now
rather than "when there's a consumer". The layering discipline the earlier
draft was protecting is preserved by the gate, not by a second traversal.

**Ruling [L], the standing constraint on what the machine may expose.** Lint
must be correct over `Vec<Token>` alone. Anything context-shaped reaches lint as
a token-stream property or an explicitly attached emission — **never** as a
reach into live parser state. And the editor's contract is `Token {id, kind,
text}` and nothing more: no round-trip through USFM text to answer a context
question.

Two consequences for this document:

- `LeaveReason` (§2.I) is not a field a consumer inspects. "Note closed without
  `\f*` at token N" is an emission the driver produces during the pass.
- The frame's stamped `ctx` [I] is driver-internal. If lint needs "what context
  is this token in?", that is either derivable from the token stream or it is an
  emission — it is never a peek at the stack.

---

## 6. Unresolved and custom markers — [F], CHANGED from onion

Onion's `handle_unknown_marker` pops down to the nearest `Chapter | Periph |
Sidebar` and pushes no frame. Ruling [F] changes both halves of the picture:

- **`\z` extensions are never rows.** Definitions arrive as CONFIG (the
  `markers.ext` shape, supplied by the caller, never read off disk by the
  engine). An *unconfigured* `\z` marker has zero behavior: index 0, opens
  nothing, closes nothing, displaces nothing.
- **Unknown/illegal markers (`\s5`) pop ALL the way out and start fresh** —
  not down to the nearest structural parent.

Both fit Option B without a special case: index 0 gets
`opens_scope: Some(ScopeKind::Unknown)` with `PARENTS[Unknown] = {}` (nothing
can contain it, so the unwind clears the stack) and the driver's one-line
`if incoming != Unknown` skip suppresses the push. An unconfigured `\z` marker
instead gets `opens_scope: None`, which skips the whole block — zero behavior,
literally.

This also resolves an open question the first draft raised: onion's
pop-to-structural-parent meant an unknown `\zaln-s` closed the enclosing `\p`,
which was plainly wrong. Under [F] a *configured* `\zaln-s` is a milestone with
`PARENTS[Milestone] = ALL` and displaces nothing, and an *unconfigured* one
displaces nothing either. The bad case is gone from both directions.

---

## 7. The `qt` overload — why (b), rows keyed by shape

Two encodings were on the table once "lexical shape is authoritative" was ruled:

- **(a)** one row carrying the milestone facts, plus a kind-from-shape override
  reconstructing the character form.
- **(b)** rows keyed by (name, `SpellingShape`) — two `qt` rows, the matcher
  picking with the shape bit the lexer already has.

The deciding question was posed correctly: *do the two forms need different
FACTS?* The answer is yes, and the USX grammar makes it unambiguous.
tcdocs/usx.rng `Milestone.style.enum` (line 1880) gives `qt-s` the attributes
`who? sid?` with `who` as its default, and (line 1881) gives `qt-e` `eid?`. The
plain `\qt …\qt*` is a character marker in the spec's Special Text group with no
attributes at all. Tabulated, the two forms differ on **six** columns:

| column | `\qt …\qt*` | `\qt3-s` |
|---|---|---|
| `kind` | Character | Milestone |
| `category` | CharTextFeatures | MilestoneQt |
| `opens_scope` | Character | Milestone |
| `closing` | RequiredExplicit | SelfClosingMilestone |
| `ws_after_name` | TagEndDelimiter | OptionalHorizontalWhitespace |
| `defined_attributes` | *(none)* | who, sid, eid |

Six differing columns is not an override; it is a second row. Under (a) the
"kind-from-shape" hook would have to reconstruct all six — which is precisely
the *hardcoded shadow table* NEXT-STEPS step 3 exists to prevent ("attributes
first would hardcode a shadow table"). The shadow would also be invisible to the
audit: `\qt`'s real contexts and closing rule would live in driver code, not in
a row anyone reviews.

**So (b), implemented.** The shape axis is deliberately TWO-valued
(`PlainOnly` / `MilestoneOnly`, with `Any` as the default every other row uses),
not three:

- The start-vs-end distinction (`sid` on `-s`, `eid` on `-e`) is **uniform across
  every paired milestone** and fully derivable from the span. Making shape
  three-valued would split `ts` and `qt` again, buying nothing and contradicting
  [G]'s "the milestone side lives on the span".
- What is *not* derivable is a name whose plain and suffixed spellings are
  different KINDS. That is what the axis is for, and it currently has exactly one
  member.

Cost: one `shape` field, one extra compare in the matcher **only** for names that
actually collide, and a table test asserting no two rows claim overlapping
spellings of the same name (`SpellingShape::overlaps`). Rows went 154 → 155.

### 7.1 The second member: `wj` — and a tension worth naming

The same RNG enum (lines 1883-1884) defines `wj-s` / `wj-e`, a paired milestone
form of `\wj`, which onion has only as a character marker. Ruled in (round 4):
the row was added, so the `SpellingShape` mechanism now has two users and the
axis is no longer a one-member special case. Good.

But the same round asked for the attributes to be verified against the 3.2 ms
docs, and that verification came back negative on the marker's *existence*. Set
the two markers side by side:

| evidence | `t-s` / `t-e` | `wj-s` / `wj-e` |
|---|---|---|
| in tcdocs/usx.rng `Milestone.style.enum` | yes, lines 1868-1869 (`sid?`/`eid?`) | yes, lines 1883-1884 (no attributes) |
| in 3.2's milestone index (`ms/index.html`) | no — it lists list, table, qt#, ts, vid | no — same five |
| dedicated 3.2 page | no `ms/t.html` | no `ms/wj.html` (404) |
| documented elsewhere in 3.2 | nowhere | yes, as a CHARACTER marker (`char/index.html`, Text Features) |

`t-s`/`t-e` was **deleted as errata** on exactly this evidence, in the same
round `wj-s`/`wj-e` was **added**. If anything the `wj` case is weaker: 3.2 does
document `wj`, but as a character marker, which is a positive statement about
what `wj` *is* rather than mere silence. Adding the row also forced a
`Category::MilestoneWj` variant that no 3.2 page backs.

Both instructions were followed as given. But under the round-4 version policy
("the referee is 3.2; the rng is fallback evidence only, and never outvotes a
page") the two rulings cannot both be right, and this is flagged rather than
quietly reconciled. Recorded as Q11.

## 7.2 Two approved 3.2 additions that land on the arms, not here

Analysed in **planning/attributes-3.2.md**, kept out of this document because
neither is a transition:

- **U25001 generalised attributes — RULED [B].** Both forms are supported and emit
  ONE `AttrList` token kind. The correction to the earlier plan ("attributes need
  the stack") is now precise: **the trailing form needs the stack; the
  node-initial form needs zero scope state** — its admission rule is adjacency
  after a marker name. Both land in NEXT-STEPS step 5. Lint flags the trailing
  form as deprecated.
- **U25004 explicit Unicode** — `\uXXXX` / `\UXXXXXXXX` are content, not markup.
  Today our scanner lexes them as unresolved MARKERS, which under [F] means a
  recovery event that tears down the whole scope stack. That is the one place
  these proposals touch the machine described here.

## 8. Open questions

- **Q1 — CLOSED.** The `qt` overload is settled: option (b), §7.
- **Q2 — CLOSED.** `\fig` keeps `opens_scope: Some(Character)`.
- **Q7 — CLOSED.** `t-s`/`t-e` deleted as errata.
- **Q8 — CLOSED.** Conform to 3.2, no version column.
- **Q9 — CLOSED.** `vid` is standalone; sid/eid removed.
- **Q10 — CLOSED.** `AttrStatus` encoded; required/optional/deprecated all fit.
- **Q11 — CLOSED.** `wj` is char-only; the milestone row is deleted, matching
  `t-s`/`t-e`. The evidence table in §7.1 is kept as the record of the decision.
- **Q14 — CLOSED [D].** Extracted from tcdocs/usx.rng: `OtherPara` (line 1151) is
  `lit cp pb qa k1 k2 sts rem`; `SectionPara` (line 892) is `restore iex ip ms#
  ms mr mte# mte r s# sr sp sd# sd cl cd`. Neither is a coarsening of our
  `Category` — they are a DIFFERENT partition of the same markers, so the rule is
  now a marker set (`schema::V_FORBIDDEN_IN_PARAGRAPHS`).
- **Q15 — CLOSED [E].** All 22 numbered families verified against their own 3.2
  page, citation on every row, and **no cap needed changing**.
- **Q12 — conditional attribute cardinality.** 3.2 says `eid` is required *if*
  `sid` was used (`qt`) and that `sid`/`eid` are optional standalone but required
  when paired (`ts`). `AttrStatus` is per-attribute, so a dependency between two
  attributes has nowhere to live; both rows say `Optional` and the conditional is
  a lint rule. Confirm that is where it belongs.
- **Q13 — do `TableRow`/`TableCell` survive as kinds?** 3.2 has neither: it files
  `th#`/`tc#` and friends under Characters > Tables and only `tr` under
  Paragraphs > Tables. Our kinds are machine conveniences carrying distinct
  precedence; `category: CharTables` already records the spec view.
- **Q3 — the narrowness of `forbids`.** §3.4's OR-composition is only sound
  while recovery is restricted to note contexts. Is full context legality ever
  wanted *as a transition* (popping on any illegal marker), or does it stay
  lint-only?
- **Q4 — `Verse`'s reachability step.** §3.1's Verse argument depends on
  Header/Meta always being bottom-most. That is true of onion's own precedence,
  so it is self-consistent — but it should be a named invariant with a test, not
  a lemma buried in a proof.
- **Q5 — verse inside a Section paragraph.** `PARENTS[Verse]` includes `Block`
  regardless of the block's category, so nothing in the table prevents `\v`
  inside an `\s`, which the spec forbids. Transition (pop the section
  paragraph) or lint emission? Onion treats it as lint only, and [L] makes lint
  the cheaper answer.
- **Q6 — one chapter-boundary definition or two?** Onion's `chapter_segments`
  duplicates the numberless-`\c` test (§2.B), and our slot model needs the same
  boundary. With the payload-token reframing both can read the same fact; worth
  making that explicit so they cannot drift.
