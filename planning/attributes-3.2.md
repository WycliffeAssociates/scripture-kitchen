# Two approved USFM 3.2 additions that change the SCANNER

**Status: RULED (2026-08-10), not yet implemented.** Both routing decisions are
now settled — §1 by ruling [B], §2 by ruling [A]. Implementation lands in
NEXT-STEPS step 4 (the scanner arms) and step 5 (the AttrList token). What
remains open is listed per section under "Still open".

**Correction to an earlier claim in this file and in `schema.rs`:**
tcdocs/usx.rng is NOT purely a 3.1 grammar — it references `AttributesPreAll` in
19 places, which is U25001's node-initial mechanism. Treat it as a 3.2-track
grammar whose prose documentation lags, and keep the rule that a 3.2 PAGE
outvotes it.

**Original status: analysis only.** Both proposals
below are marked approved for 3.2 in their own text, so under the round-4 version
policy (the table conforms to 3.2, latest, no version column) they are in scope —
not speculative future work. Neither touches the marker table; both touch the
scanner arms, which is why they are written up together.

Read from the proposal sources, not summaries:
`repos_to_compare/tcdocs-main/proposals/2025/` in the usfm_onion checkout
(`U25001 Attributes.md`, `U25004 Explicit Unicode.md`).

Sibling proposals seen while reading, for the record: U25003 Lists and Tables,
U25008 vid, U25009 Pl Marker, U25010 rx/ep/ebr — not read, not analysed.
**U25002 Anchors (inline referencing) is PARKED** per Will ("not sure we're
interested in taking this on yet"); it is the source of the `\a` milestone and
the `aid` attribute that U25001 references, so the two are entangled — see §1.6.

---

## 1. U25001 — Generalised (node-initial) attributes

> "Reorganised ready for final standardisation. Add to USFM 3.2."

### 1.1 The exact syntax

Quoted verbatim from the proposal's grammar block:

```
<marker>          ::= "\" <HS>* <marker_name> (<attribute_list>
                      | <default_attribute>)? content <marker_end>
<marker_name>     ::= <letter>+ <digit>*
<attribute_list>  ::= "|" <attribute> (" " <attribute>)* "|" <HS>*
<default_attribute> ::= "|" <ATTRIBTEXT> "|" <HS>*
<attribute>       ::= <ATTRIBNAME> "=" '"' <ATTRIBTEXT> '"'
```

So: **both a leading AND a trailing pipe are required**, then optional horizontal
whitespace, then content. The closing pipe is load-bearing and the proposal says
why:

> "The requirement for the attribute list to be delimited by a final `|`
> character is required even if the content of the node is empty. Thus
> `\w|Jesus|\w*` is valid while `\w|Jesus\w*` is not due to the ambiguity with
> the previous interpretation in 3.1."

Worked examples, verbatim:

```
\p|cat="emphasised"| \v|script="Arab"| 1 And the \nd Lord\nd* said ...
\f |aid="mynote"| + \cat person\cat*\fr 1:3 \ft Got some text at last\f*
\f |aid="mynote" cat="person"| + \fr 1:3 \ft Got some text at last\f*
\w |lemma="Jesus"|Jésus\w*
\w |Jesus|Jésus\w*
```

### 1.2 Which markers admit them

> "we also allow them at the front and also in the elements: `<para>`, `<verse>`,
> `<chapter>`, `<note>`, `<cell>`, `<figure>`, `<link>`, `<periph>`, `<ref>`,
> `<row>`, `<sidebar>`. (`<table>`, `<list>` are represented in USFM by
> milestones.)"

Mapped onto our `MarkerKind`: Paragraph, Verse, Chapter, Note, TableCell, Figure,
Periph, TableRow, Sidebar, Character, Milestone. **That is every kind we have
except `Header` (`\id`, `\usfm`) and `Meta` (`\cat`)** — and `<link>`/`<ref>` are
character markers, so Character is in via two routes. Treat it as "effectively
all of them" and confirm the two exceptions if it ever matters.

### 1.3 What this does to `defined_attributes` — the semantic shift

The `\v|script="Arab"|` example is annotated:

> "This contrived example allows the addition of an **undefined** `script`
> attribute to a verse"

So arbitrary, spec-undefined attributes are legal on any admitting node. That
retires the reading of `defined_attributes` as "the attributes allowed here".
Its meaning becomes **"the attributes 3.2 NAMES for this marker, with their
status"** — which is exactly the shape it already has, and exactly the same
carve-out already recorded for `x-`/`z-` attributes. Consequence for lint: an
unknown attribute is not an error, only un-named; the finding is at most
informational.

`AttrStatus::Deprecated` also gains a second, bigger user:

> "The use of attribute lists at the end of a node is deprecated as of the
> version of the standard that incorporates this proposal (3.2) and will be
> removed at the next major release (4)."

That deprecates a **position**, not an attribute name. Nothing in the table can
say it (it is not per-marker, it is per-occurrence), so it is a lint rule keyed on
where the scanner found the list. Recorded as open question Q-A3 below.

### 1.3a RULED [B] — both forms, one token kind

1. **The scanner supports BOTH forms**, emitting the **same `AttrList` token
   kind** for each. Admission rules differ, and only one needs the stack:
   - **trailing (legacy)**: a pipe while an attribute-taking char/milestone scope
     is open — needs the open-marker stack, as originally planned;
   - **node-initial (3.2)**: a pipe adjacent after a marker name, both pipes
     required — **zero scope state**.
2. **Lint flags the trailing form as deprecated.** The spec deprecates the
   position, not any attribute name, so it cannot be a table column; the scanner
   records which side the list was on and lint reports it.
3. **`defined_attributes` means "NAMED by the spec", not "allowed".** Undefined
   attributes (`\v|script="Arab"|`) are legal data — the same carve-out already
   recorded for `x-`/`z-`.
4. **The HS collision resolves in favour of the pipe:** a pipe **TERMINATES a
   marker name universally**. `TAGEND` already permits it
   (`/(?:${ws}+|(?=[\\|]|$))/`), so a node-initial list satisfies the delimiter
   position even on `AtLeastOneHorizontalWhitespace` rows like `\v` and `\c`.
   **This is OUR INTERPRETATION of an internally inconsistent proposal** — see
   §1.5 for the inconsistency — adopted because it is the only reading under
   which the proposal's own `\v|script="Arab"|` example is legal. A lint hook
   noting "no whitespace after a marker name that normally requires it" is
   optional and not required by the ruling.

### 1.4 The pipe arm: what breaks, and what actually gets EASIER

The planned rule (NEXT-STEPS step 2) was:

> "an attribute list becomes ONE `AttrList` token once the stack exists — the
> stack knows a `\w` or milestone is open; a bare pipe in ordinary text stays
> content."

U25001 breaks the *premise* of that rule — attribute lists are no longer confined
to char markers and milestones. But it does **not** make the disambiguation
harder. It splits into three cases, and the new one is the easiest:

| case | shape | state needed to decide |
|---|---|---|
| **node-initial** (new in 3.2) | `\p|…|` — the pipe is the next non-HS byte after a marker NAME | **none.** A local lookbehind the scanner already has: "did a marker name just end?" |
| **node-final** (3.1, deprecated in 3.2) | `\w Jésus|lemma="Jesus"\w*` | the open-marker stack, as originally planned |
| **content pipe** | a pipe anywhere else | neither of the above ⇒ content |

Three things fall out of this that are worth stating plainly:

1. **The stack is no longer needed for the common case.** Node-initial attributes
   are decidable with zero scope state, because the deciding fact is adjacency to
   a marker name. The stack requirement survives only to support the form 3.2 has
   just *deprecated* and 4 will remove. That inverts the dependency NEXT-STEPS
   step 3 was built around ("the attribute parser needs the open-marker stack") —
   for new-style attributes it does not.
2. **A node-initial list is BOUNDED, which the trailing form never was.** `|` …
   `|` means the AttrList token's end is findable by a single memchr for the
   second pipe, instead of scanning to the next `\`. That is strictly better for
   the "emit fewer tokens" direction NEXT-STEPS banked as the remaining
   performance lever.
3. **"Bare pipe in plain text is content" stays decidable** — and stays decidable
   *without* lookahead. All three cases are settled by what precedes the pipe.

The genuine new cost is a **bail path**: seeing `|` right after a marker name, the
scanner must find the closing `|` before it can call the span an AttrList. If the
line has no second pipe, the first pipe was content after all (or the document is
malformed). So the arm needs "scan for the matching `|`, and if it is missing,
re-classify as content" — a bounded, recoverable bail, but a bail.

### 1.5 The collision with `ws_after_name`

The grammar puts `<attribute_list>` immediately after `<marker_name>`, with no
whitespace between them — matching `\p|cat="emphasised"|` and `\v|script="Arab"|`.
Our `ws_after_name` column says what may follow a marker name, and the two do not
agree everywhere:

- markers whose value is `TagEndDelimiter` are **fine**: `TAGEND` is
  `/(?:${ws}+|(?=[\\|]|$))/` (tcdocs/def.txt), which already permits a `|` to
  terminate the name with no whitespace. The spec anticipated this.
- markers whose value is `AtLeastOneHorizontalWhitespace` are **in conflict**:
  `\v` is one of them (its curated row requires whitespace after the name), yet
  the proposal's own example is `\v|script="Arab"| 1` with no space. `\c`, `\id`,
  and every `ParaIdentification` / `ParaTitlesSections` paragraph share the
  problem.

Meanwhile the proposal's *own examples are internally inconsistent* about the
space: `\f |aid="mynote"| +` has one, `\p|cat="emphasised"|` does not, and the
grammar as written permits neither (`<HS>*` sits between the backslash and the
name, not after it). Open question Q-A1.

### 1.6 The `\a` milestone entanglement (parked)

`aid` is the *anchor id*, and it is the default attribute of the `\a` milestone
from U25002 — which is parked. U25001 leans on `\a` throughout its discussion
section (`\ip \a|authorship\* The book of John was written…`) as the alternative
spelling for the same underlying content model. **The node-initial mechanism does
not depend on `\a`**: `\ip |aid="author"| …` works without it. So parking U25002
is safe here, but note that `aid` appearing in U25001's examples is a borrowed
term whose defining proposal is parked.

### 1.7 Status after ruling [B]

**Closed:**

- **Q-A1 — RULED.** A pipe terminates a marker name universally; whitespace
  before it is permitted but not required, and `AtLeastOneHorizontalWhitespace`
  rows are satisfied by the pipe. Our interpretation, recorded as such (§1.3a.4).
- **Q-A2 — RULED.** One `AttrList` token for both forms, interior unparsed by the
  scanner. The bounded `|`…`|` span makes this the cheap option as well as the
  consistent one.
- **Q-A3 — RULED.** Lint reports the trailing form; the scanner records the side.
- **Q-A4 — RULED.** Yes, both forms are supported. The stack is carried for the
  legacy form deliberately and knowingly.

- **Q-A5 — RULED (round 9): accept as data, lint flags it.** Multiple bare values
  are representable, and *ridiculous is not the same as unrepresentable*. This is
  the same posture the domain already takes on duplicate chapters (GLOSSARY
  "Occurrence": "messy text under revision legitimately contains duplicates").
  "Refuse, never invent" governs input that **cannot be represented**, not input
  that is merely silly — so no new precedent is set, and none was needed.
- **Q-A8 — RULED (round 9): LAST wins**, deterministically, and lint flags the
  duplication. Worth recording honestly that this is a CHOICE, not a
  platform-inherited default: **HTML attribute duplication is first-wins, CSS
  declaration duplication is last-wins.** We follow CSS, and we follow the
  proposal, which says "if there is any conflict over the value of an attribute so
  defined twice, the later definition wins". Cited as Will's ruling 2026-08-10 so
  nobody later "corrects" it toward the HTML convention.

**Still open:**
- **Q-A6 — RULED (round 10): the three-rung ladder.** See §1.8.
- **Q-A7 — `aid` specifics.** `aid` is the anchor id, whose defining proposal
  (U25002) is PARKED. Its value grammar, uniqueness requirements, and whether
  anything validates it are therefore unspecified for us. `\ip |aid="x"|` parses
  regardless; nothing yet says what `x` may be.
  (Q-A8 was here; see the ruled list above. It is an INTERPRETER rule over two
  AttrList tokens, not a scanner rule — the scanner emits both lists and never
  reconciles them.)

### 1.8 RULED — the three-rung resolution ladder (Q-A6)

The rule for **a pipe adjacent after a marker name**, verbatim as ruled:

> 1. **closing pipe before the bound (newline)** → node-initial `AttrList` (the
>    3.2 form);
> 2. **no closing pipe, but the marker is an attribute-taking char/milestone and
>    its CLOSING MARKER arrives** → legacy trailing `AttrList` + deprecation lint;
> 3. **neither terminator** → it was never an attribute list: the pipe is ordinary
>    content, with a lint hint "unterminated attribute list?".

Rung 2 is the one that earns the ladder. It **rescues `\w |lemma="x"\w*`** — legal
3.1 markup: empty content plus a trailing attribute list. Under a two-way
"closing pipe or bust" rule that would silently fall through to rung 3 and become
content, losing a real attribute list in a form the corpora actually contain. It
must not fall through.

**Paragraph markers have no rung 2.** There is no pre-3.2 trailing attribute form
for `\p`/`\q`/`\s`, so nothing can rescue an unclosed pipe on a paragraph: it goes
straight from rung 1 to rung 3. Worth stating because it means the ladder's arity
is per-marker-kind, not universal — attribute-taking char markers and milestones
have three rungs, everything else has two.

Note the ladder is ordered by **how far the scanner must look**, cheapest first:
rung 1 is bounded by the newline, rung 2 by the closing marker, rung 3 is the
fallthrough. That ordering is also the correctness order — a closing pipe before
the newline is unambiguous, so it should never be overridden by a later closing
marker.

**Implementation caution, on the record (Will):** *"we'll need to write all this
part slowly."* This ruling settles SEMANTICS only. It is step-4/5 scanner work;
no scanner code changes now.

---

## 2. U25004 — Explicit Unicode (USV escapes)

> "Approved for addition to USFM 3.2."

### 2.1 The exact spelling

Two escapes, distinguished by the CASE of the letter and by digit count:

| form | digits | example |
|---|---|---|
| `\uXXXX` | exactly 4 uppercase hex | `\u0020` (space, U+0020) |
| `\UXXXXXXXX` | exactly 8 uppercase hex | `\U0001F600` (grinning face, U+1F600) |

No terminator character — the sequence ends when its fixed digit count is
consumed. Use example, verbatim: `\p \u0020Text` ("A regular space at the
beginning of a paragraph").

### 2.2 Where it may appear: content only

Three independent pieces of evidence, all pointing the same way:

1. The proposal: "The resulting character is inserted in the **content** of the
   file… While these are two markers in USFM, they are **not considered part of
   the grammar**… They are typically handled during the **lexical phase** of
   parsing and so do not appear in usx.rnc."
2. Its Issues section: "other uses of `\u` for structurally parsed stuff would not
   happen so `\u005cp` is not the same as `\p`." A USV can never produce markup —
   it does not re-enter the marker grammar.
3. **Attributes are excluded by the existing grammar**, not by the proposal:
   `ATTRIBTEXT` is `/(?:\\["\\=~/|]|[^\\"])+/` (tcdocs/def.txt), whose escape set
   is `" \ = ~ / |` — `u` is not in it, so `\u0020` inside an attribute value is
   not valid `ATTRIBTEXT`. The proposal never says this; it falls out. Flagged as
   Q-U2 because inference is not the same as a statement.

### 2.3 What our scanner does with it TODAY

Both facts verified against `src/scanner.rs`:

- **The text arm** (the `BACKSLASH =>` match) continues the text run only for
  `\/`, `\~`, `\\`, `\|`. Anything else after a backslash is "a real marker
  start", so `\u0020` **breaks the text run**.
- **`marker_end`** then walks the name with `is_ascii_alphanumeric()`, **which
  accepts uppercase**. So:
  - `\u0020` → one `Marker` token named `u0020`
  - `\U0001F600` → one `Marker` token named `U0001F600`

They are not mis-split, which is a small mercy — each becomes exactly one wrong
token rather than a cascade. Both then resolve to marker index 0
(unresolved/custom) because no row matches, and under ruling [F] an unresolved
marker is a **recovery event: pop all the way out and start fresh.** So today a
single `\u0020` inside a verse would tear down the entire scope stack. That is the
real severity, and it is worth fixing before it can appear in a corpus.

### 2.3a RULED [A] — option (a), the text-arm escape fold

**USVs are part of Text.** The text arm's peek set grows to include the exact
patterns `u` + 4 uppercase hex and `U` + 8 uppercase hex — fixed width, no
terminator. **No new `TokenKind`.** Three things the ruling settles beyond the
routing:

1. **The exact USV pattern beats any marker claim.** Precedence is by pattern,
   not by table lookup: `\u0020` is content even though `u0020` is a
   marker-shaped name.
2. **Lowercase hex is a LINT flag, not a scanner rejection.** `\u00e9` is
   ill-cased, not not-a-USV. This pairs with a new lint fact — **"marker cased
   wrong"**: the scanner tolerates uppercase in marker names as DATA (they cannot
   resolve to a row, so they land on index 0) and lint reports the casing. Which
   is why tightening the name scan to lowercase-only was **not** adopted (Q-U3
   closed against it).
3. **The transcoder consequence is ACCEPTED.** `unescape(text_span)` stops being
   a subsequence of the source and gains a hex→UTF-8 decode step, so it can emit
   bytes that appear nowhere in the source. `concat(spans) == source` is
   unaffected — the invariant is over spans, not over unescaped output — and the
   cost is taken knowingly rather than discovered later.

Both facts are recorded in `schema.rs` beside `USV_ESCAPE_LETTERS` so the scanner
and the table cannot drift.

#### Step-4 scanner work item (do NOT change scanner code yet)

Recorded because the USV work exposes it rather than causes it: **the main loop
dispatches EVERY backslash to the marker arm.** The escape peek lives inside the
text arm's run-continuation logic, so an escape at the START of a region — a
`\~` immediately after a newline, and in future any `\uXXXX` — never reaches
that peek and is mishandled today. The dispatch needs its own escape-peek before
routing to the marker arm. This is a pre-existing bug for `\~`; USVs only widen
it.

### 2.4 Which arm needs to learn it — the analysis behind ruling [A]

**Both arms are implicated, in different ways:**

- **`marker_end` / `classify_marker`** must stop claiming these. There is a free
  fix available here regardless of routing: marker names in USFM are lowercase
  letters plus digits, so `is_ascii_alphanumeric()` is already too lax.
  Tightening it to `is_ascii_lowercase() || is_ascii_digit()` makes
  `\U0001F600` non-marker-shaped for nothing, and is a correctness improvement
  independent of U25004. It does **not** solve `\u0020`, whose letter is
  lowercase. (Q-U3.)
- **The text arm** is the natural owner, because the sequence is content. And the
  code already has the exact precedent: the `SLASH` arm splits an `OptBreak`
  token out of the middle of a text run and resumes. A USV token would be a
  drop-in analogue of that handling.

Two routings, and this is **the decision Will asked to have flagged rather than
made**:

**(a) Fold into the text run** — add `u`+4hex / `U`+8hex to the escape set so the
run simply continues. Zero new token kinds. Consistent with the proposal's own
framing ("no semantic difference between representing a character in the rendered
form or as a USV… syntactic sugar").

**(b) Its own `TokenKind::Usv`** — split it out like `OptBreak`. Costs the 11th
kind (and `kind_bits` room), but every consumer sees explicitly that this span is
not literal text.

The argument that decides it, and it is not the obvious one. Our Text token
*already* carries escapes that do not render literally (`\~`, `\\`, `\|`, `\/`),
so it already needs an unescape interpreter and (a) looks consistent. But there is
a property that unescape has today and would lose:

> **`unescape(text_span)` is currently a SUBSEQUENCE of the source bytes** — it
> only ever deletes backslashes. With `\u`/`\U` folded in it becomes a
> *transcoder*: it emits bytes that do not appear anywhere in the source (`\u0020`
> → a space; `\U0001F600` → four UTF-8 bytes).

That is a design event by the GLOSSARY's standard, not a detail. `concat(all
spans) == source` still holds either way — the lossless invariant is safe. But
"kinds must be enough to render from" gets fuzzier under (a): a consumer holding a
Text token can no longer treat unescaping as trivial byte-dropping. Under (b) the
new obligation is *visible in the kind*, which is what the kind is for.

Leaning (b), for that reason and because the `OptBreak` precedent makes it cheap —
but it is a genuine call about where lexical sugar belongs, and it is Will's.

### 2.5 Status after ruling [A]

**Closed:** Q-U1 (option (a), text-arm fold), Q-U3 (do NOT tighten to
lowercase — uppercase names stay tolerated as data and become a lint finding),
Q-U4 (lowercase hex is a lint flag, so the scanner's validator is effectively
`is_ascii_hexdigit` with a casing diagnostic).

**Still open:**

- **Q-U2.** Attribute values: excluded by *inference* from `ATTRIBTEXT`'s escape
  set, never stated by the proposal. If USVs ARE legal there, the attribute
  interpreter — not the scanner — has to handle them.
- **Q-U5.** The proposal's own Issues list leaves "Handling of escaped characters
  like the backslash" open: how `\u005C` interacts with the existing `\\` escape.
  Unresolved upstream; we should not invent an answer.
- **Q-U6 — RULED (round 10) for pattern failure, via the Q-A6 analogy. One wrinkle
  the analogy does NOT reach, flagged below.**

  The analogy is clean, and it collapses to **two rungs** rather than three,
  because a USV has no terminator to search for — it is a fixed-width validation,
  so there is nothing corresponding to rung 2:

  1. **the pattern matches** (`u` + 4 hex, or `U` + 8 hex) → a USV, folded into the
     text run;
  2. **it does not** → the bytes are whatever they would otherwise have been:
     normal backslash routing, i.e. the marker arm claims `\u12G4` as a marker
     named `u12G4` (which resolves to index 0), plus a lint hint.

  Same shape as A6: a failed adjacency-peek resolves to the fallthrough reading and
  the diagnostic carries the suspicion. Three sub-cases confirmed determinate:

  - **partial hex** (`\u12` at end-of-input, or before a newline) — fewer than the
    required digits available, so the pattern fails and rung 2 applies. No
    ambiguity: width is fixed by the letter's CASE, so a short `\U` never
    reinterprets itself as a `\u`.
  - **no trailing delimiter needed** — `\u0020Text` consumes exactly 6 bytes and
    leaves `Text` as text, which the proposal's own example (`\p \u0020Text`)
    confirms. So the peek must NOT require a delimiter after the digits, and this
    is where "the USV pattern beats any marker claim" does real work: normal
    routing would otherwise have read one marker named `u0020Text`.
  - **lowercase hex is not a failure** — ruling [A] made it a lint flag, so the
    validator is `is_ascii_hexdigit` and casing is diagnostic. Only a non-hex byte
    or a short window reaches rung 2.

  **The wrinkle — the pattern can SUCCEED while the value is unencodable.** Two
  ranges do this: a UTF-16 surrogate (`\uD800`–`\uDFFF`) and, for `\U`, anything
  above `U+10FFFF`. Both are syntactically perfect and neither is a Unicode scalar
  value, so neither can be encoded as UTF-8. This matters concretely because the
  proposal itself discusses JSON spelling `U+1F600` as the surrogate pair
  `\uD83D\uDE00` — nothing stops someone writing that pair into USFM, where it
  would decode to two lone surrogates.

  A6's ladder has no rung for "syntactically fine, semantically impossible", so
  the analogy genuinely does not reach it and it is flagged rather than forced.
  One framing that may help: this is not a SCANNER question at all. The scanner
  only spans the bytes; the failure appears in the unescape transcoder, so the
  refuse-or-substitute decision belongs to the interpreter, alongside the
  verse-designator interpreter's "not cleanly numeric" flag. Recorded as **Q-U7**.

- **Q-U7 (new).** Syntactically valid USVs with unencodable values — surrogates
  `\uD800`–`\uDFFF`, and `\U` beyond `U+10FFFF`. Refuse, substitute U+FFFD, or
  pass the span through untouched and let the consumer decide? Note the lossless
  invariant is unaffected either way (the span is preserved); only `unescape`'s
  output is in question.
