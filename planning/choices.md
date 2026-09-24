# Choices

Per-pass decision ledger. Newest section first.

## Designator members, labels on the Toc, and options objects at the wall

**A segment is a coordinate and a list has holes** (Will). `designator::verse`
keeps its HULL — every existing consumer sorts, bridges and renumbers by it
unchanged — and `designator::members` is the second reading: what the span
covers, member by member, allocation-free over the label it borrows. The
alternative, changing `Designator` itself, would have moved every hull
consumer at once for a fact only two of them want.

**Lint reads holes only inside a list, and never reports one left open.**
`\v 1,3,5` then `\v 2` fills a hole (no finding); a number a list already
covers is a duplicate; outside an open list every verdict is the old one, which
the corpus and oracle tests pin. An unfilled hole is NOT a gap: a list may be
exactly what the translation means, and flagging it would be a new rule over
every corpus. Two segments of one verse (`\v 2a` … `\v 2b`) are no longer a
duplicate; the same segment twice, or a bare number beside a segmented one,
still is. The example tier has no segmented designators, so no verdict there
moved.

**The label and the members are SPANS on the Toc, not a string table.** A row
carries `labelStart..labelEnd` into the source, and a verse row names a run in
`Toc::members`; both wires (the dish and the census) carry them as offsets, so
UTF-16 conversion is the rule every other offset already follows and nothing
is copied. The census's old "no token, no designator" boundary still holds for
TOKEN indices; what changed is that the retained Toc now keeps the label's
position when it is built. Chapter rows still tile. Dish v5, census v2.

**Options are objects, and an unknown key throws.** Every door that took a
positional boolean or a trailing optional flag takes one options object, last,
typed in the `.d.ts` by a `typescript_custom_section` interface
(`unchecked_optional_param_type` on a `JsValue`): the machinery the crate
already used for `setExtensions`, and no serde on the wall. The positional form
existed because `Reflect::get` reads a misspelled key as `false`; that is now
answered twice — a compile error for a typed caller, and a refusal naming the
key (`onion_wasm::options::bag`) for anyone else. The existing option bags
(`find`, `mask`, the overlay doors) got the same check, and `skeleton`/
`*NodeFor` lost their redundant trailing `utf16`.

## The extensions doors — on galley, mirrored by linkage

**A door that lands only on `onion-wasm` does not exist.** `usfm-galley` is the
superset module and the only build a host vendoring one package gets, so
anything onion can do at the wall galley must do too. The rule is now in
`galley/src/wasm.md` beside the door list, and it is enforced where it can
fail loudly: `tests/sous_conformance.mjs` pins the EXACT export list.

**Both doors are DEFINED in `onion-wasm` and re-exported by
`galley::wasm::onion`, because that is the only shape that links.** Galley
defines no free `#[wasm_bindgen]` function of its own and cannot: its cdylib
links `onion-wasm`'s shims, so a second definition under the same `js_name`
would collide symbol for symbol — which `wasm/onion.rs`'s own doc already
says about wrappers. The re-export is the surface; the pinned list is the
check. Adding the two names cost 0 new machinery and 2 lines there.

**Two doors, not one, and reading installs nothing.** A host shows what a
`markers.ext` gave it before acting on it, and a host whose markers come from
a `custom.sty` or its own UI feeds `setExtensions` the list directly.

**A file report carries its `line`; a list report has no `line` key.** The
review flagged `Malformed.line == 0` as a sentinel. The shared mise type keeps
it — the file reader is the common case and owns a real line — and the WIRE
drops the field instead of crossing a zero, which is the boundary where the
honest shape is free.

**The category is judged at the door, every other reason at the registry.** An
unknown `\category` word cannot reach `Extensions::new`, which takes a typed
`ExtensionCategory`; everything else (no name, not `z`-initial, not
alphanumeric, duplicate) is the registry's and answers the same reasons a file
would get. Only malformed JSON throws — a bad ENTRY never costs a host the
rest of its list.

**`set_extensions`'s body is split from the door** the way `diffed` is:
`JsError` cannot be constructed off a wasm target, so `installed()` is what
the native tests call.

## Custom `\z` markers behave as their category

**Templates over runtime rows.** A user marker resolves to one of seventeen
rows appended to the authored table, one per behaviour-bearing `\category`
word, each a copy of the spec row that category behaves as. Nothing is built
at runtime, the wire does not move, `MarkerIdx` stays a `u8`, and a host that
was never told about extensions decodes every buffer. `rows.rs`'s
`every_template_copies_its_source_row` DIFFS each template against its source,
so curating `\p` reaches `zpara` or the build fails.

**A template owns four columns, not three.** `marker`, `shape` and
`numbered_max` as planned, plus `priority`, which is a MEASURED hot-marker
rank: copying `\p`'s onto `zpara` would be a measurement claim about a row no
document byte ever reaches by name. Templates are `None`.

**The shape column carries the spelling rule.** The plan's §2 said `Any` for
every template but `zms`; its §3 said a `char` name spelled `\zfoo-s` must be
row 0. §3 wins, and the cleanest place for it is the column that already means
"which spellings this row claims": `PlainOnly` for every template but
`milestone` (`MilestoneOnly`) and `standalone` (`Any`, as `\ts`). The check is
then one `SpellingShape::overlaps` — the same predicate `by_name_arms` uses —
rather than a second table. The token KIND is decided by the spelling before
any row is consulted, so a row that disagreed with it would hand the walker a
milestone token sitting on a character row.

**The registry branch is in `extensions::marker_idx`, not
`generated::marker_idx`.** The generated table is a projection of the authored
rows; a runtime global reaching into it would couple the wire's own generated
file to process state. `generated::marker_idx` stays the SPEC lookup and still
answers `UNRESOLVED` for every `z` lexeme; `crate::extensions::marker_idx` is
the door the scanner calls, and the only place a template is reachable. That
also keeps the ~8 call sites that look up spec names (`c`, `pb`, `usfm`, the
hot names, lint's rows) free of an extensions argument they can never use.

**`lex_with` is real, so the registry is threaded, not read per lexeme.**
`Scanner` holds an `&Extensions`; `lex` reads the process registry once and
hands it in. A per-lexeme global read would have made `lex_with` a lie. The
global is an `RwLock<Option<Arc<_>>>` and a `u64` generation — no new
dependency, and the lock is taken once per document.

**Cell alignment moved to one helper reading the SPELLING.** `\tcr1`'s row
name ends in `r` but its spelling ends in its column index, so "read the
spelling" needed the digits trimmed first. `export::cell_align` is the one
place; USJ and USX take its two values, HTML its three. Only the spec's own
`tc`/`th` stems are read: the plan wanted `\ztcr2` to align `end`, but the
spec's `cell` category carries no alignment, so an extension has none to
spell — reading one out of a `z` name would invent a convention and silently
align a `\zaligner` cell.

**`rename`'s splice is the spelled name minus its digits.** It was the row
name's length, which a template makes wrong. No canonical name ends in a digit
(`tables::emit` asserts it), so the new rule is byte-identical for every spec
marker and still carries `\ph2` → `\li2`'s level along.

**The Pantry flushes on a generation change; it does not drop books.** Both
caches key on content, which is right only while the same bytes parse the same
way. `check_registry` clears the chunk store and re-derives every book from
its retained TEXT — the host's, never derived. A book retaining no text keeps
what it has: dropping it would make `books()` lie about what the host
registered, where stale source counts only misreport a reference until it is
registered again. The re-derivation is EAGER, and that was weighed: installing
a registry is a project-open event, not a keystroke one, so paying the whole
corpus once on the next door is the cheaper thing to reason about than a
per-book lazy flag.

**`Marker.name()` answers `null` on a template row.** The JS table's entry is
named for the template (`zpara`), not for the marker a document spelled
(`zmyp`), so answering it would be a lie a consumer cannot see through. Row 0
already answers `null` for the same reason, and a consumer that handles one
handles the other; `TokenView.spelling(text)` stays the one way to read what
the author wrote.

**One name check, in `mise`.** `check_name` holds the spec's three rules —
non-empty, `z`-initial, ASCII alphanumeric — and the file reader and the
registry both ask it, so a name legal in one is legal in the other. The
duplicate-name and missing-category reasons stay with whoever is assembling a
list, which is where they differ.

**A poisoned registry lock is recovered from, not treated as empty.** What the
lock guards is one whole `Arc` replaced in a single assignment, so a panic
elsewhere cannot leave it half-written and poison carries no information. The
old code installed nothing on poison and then bumped the generation and
returned reports as if it had.

**What did not change.** `cst.rs`'s `name == "li"`/`"w"`/`"esb"`/`"c"`
comparisons: a `list`-category extension is not `\li` for the list-container
rules, which is right — the spec gave a category, not a claim to be `\li`.
`lint/structure.rs`'s `container_end_text` reads the row name and now
`debug_assert!`s no template reaches it, since none copies a U25003 container.
Heading level falls back to `<h3>` for a `title`/`header`/`sectionpara`
extension, the same answer an unknown family has always had.

## 0.1.3 — runs that say where and what, three named cuts, dish range queries

**The markup pass takes what the `text` cut DROPS, not "every non-Text
token".** The plan listed `Newline` and the pad kind as markup, but both
survive `Filter::text()` and so already go through the word differ; claiming
them twice would double-cover the span. The split is the mask's own: the text
pass covers the kept bytes, the markup pass the gaps. Tiling then holds because
the two passes partition the tokens, which is why `settle` only debug-asserts
it.

**A `Pad` run is `Markup`, not `Whitespace`.** §2.3's own recipe — drop
`what == "markup"`, concatenate the rest, get the reading — only works if
`what != Markup` is EXACTLY the text cut. `Whitespace` therefore means spacing
inside the reading (a newline, a blank text token), never a delimiter's
surplus.

**L4 compares concatenations, not run lists.** Coalescing is per side, so one
side can carry as a single run what the other splits across a dropped token;
the unchanged TEXT bytes and the unchanged MARKUP bytes each match across
sides, and that is the law. Pairing run-for-run is not true and never was.

**L3 is stated in the flags' own terms.** Both unit flags are byte claims made
after ASCII whitespace is stripped, and the markup pass is token-grain — `\p`
reads as changed when only the space after it moved. So the law compares what
one side LOST against what the other GAINED, whitespace-stripped, rather than
asserting every text run is unchanged.

**The reader's range helpers live on `Tree`, not on a dish class.** `Dish` is
an interface, and `Tree` already holds the token rows, the arena, `extent`,
`owners` and `parents` — the four helpers are that walk, so they belong beside
it.

**The dish-query goldens are Rust-written and JS-asserted.** `Cst::extent` and
`Cst::owners` are the oracle; `onion/tests/dish_queries.rs` writes 544 entries
over one synthetic shapes fixture, three usfmtc cases and one real book, and
`conformance.mjs` reproduces every one through the reader. Neither half can
drift without the other failing. `UPDATE_GOLDENS=1` rewrites.

**A run carries no bytes.** It is a span; `source[from..to]` on its own side is
its text. Shipping both would put the same bytes on the wire twice for
everything a run touches, so the `text` field the old positionless run
carried goes with the positions' arrival — a consumer slices the source it
already holds.
