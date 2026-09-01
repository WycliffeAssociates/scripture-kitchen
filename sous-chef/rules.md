# Rules and examples

This is the durable catalog of Sous Chef 2 review lanes. It says what each
lane may claim, the evidence it needs, when it abstains, and the examples that
must survive implementation. Cross-cutting ownership and data contracts live
in [charter.md](charter.md); sequencing and port decisions live in
[roadmap.md](roadmap.md).

## Shared rule contract

Every rule must answer these questions before implementation:

1. What exact observation is made?
2. What narrow user-facing inference may it support?
3. What does it not establish, and what legitimate case resembles the error?
4. What are the conditioning variables, primary signal, opportunity count,
   support floor, and abstention conditions?
5. What is mapped per chapter, what boundary state is stitched per book, and
   what is reduced across the corpus?
6. Which config changes observations and which merely re-judge them?
7. Which raw counts or facts must reach the finding detail and compact wire
   digest?
8. What synthetic true case, counterexample, seam case, and cold-versus-edit
   equivalence test pin the claim?

Rules fall into four lanes:

- **Deterministic:** the documented domain does not admit the condition.
  Enable/disable only; no sensitivity model.
- **Convention-learned:** the target corpus supplies a comparison population.
  Raw observations are retained and judgment is a transparent fraction.
- **Source-compared:** target and declared source pair through aligned units.
  Findings describe disagreement with that source, not absolute quality.
- **Census-only:** useful descriptive evidence that cannot honestly support an
  error-shaped finding.

## Level 1a — deterministic hygiene

Hygiene finds text states that are mechanically suspect independent of
language convention. It walks raw bytes at high speed, checks that a hit lies
inside analyzable content, and coalesces maximal runs.

### Initial checks

Landed in `sous-core::hygiene`, needing no Unicode data — every hit is a
byte-pattern decision:

- C0 controls except tab, LF, and the CR in a valid CRLF pair;
- DEL and C1 controls;
- U+FFFD replacement characters;
- a stray CR;
- backslashes in content;
- line-initial merge-conflict markers.

Deferred until the Stage 1 classifier exists; they need Unicode classes and
are not approximated by byte rules:

- invalid/noncharacter code points beyond U+FFFD;
- zero-width space and misplaced NBSP/format characters where the rule can
  make a deterministic claim;
- combining marks without a base.

Marker validity, empty marker structure, chapter/verse ordering, and metadata
consistency remain Onion/editor responsibilities. Through the Onion producer
this includes any lone backslash: Onion lexes it as a marker, well-formed or
not, and masks it out, so only a `\\` pair reaches Sous as content. A vref
producer keeps every backslash as content. “Empty verse content” may be a
Sous check only when Onion has already established a valid verse anchor and
exposes an empty analyzable unit.

### Required examples

- 223 contiguous NUL bytes produce one finding spanning the run.
- CRLF is silent; a stray CR is reported.
- A backslash in masked-out USFM markup is silent; a stranded backslash in a
  content span is reported.
- A line-initial `<<<<<<< ours` line is reported; the same text mid-line is
  not.
- A decomposed grapheme's combining mark is not mistaken for a free mark
  (waits for the classifier).

## Level 1b — nonletter convention inventory

One shared observation substrate counts punctuation, symbols, digits, and
nonletter grapheme atoms. It records placement, directed neighbors, run shape,
and dispersion without retaining occurrence lists. These are views over one
model, not twenty independent rule implementations.

### Evidence questions

#### Placement and neighbor ladder

Ask where a glyph or run sits, from coarse to fine:

| grain | comparison | example for comma |
| --- | --- | --- |
| G0 | outer class: letter, space, edge | attached versus spaced |
| G1 | outer class conditioned by run composition | attached inside a digit-bearing run |
| G2 | pooled neighbor category | followed by a quote or digit |
| G3 | exact nonletter neighbor | followed specifically by `.` |

Letters are never individuated as neighbors. Fine grain is used only when its
opportunity count clears the evidence floor; otherwise judgment falls back to
the coarser pool rather than becoming silent.

Directed pairs belong to their first member and are described as logical
start/end relationships, never visual left/right.

#### Run length and composition

Run length is judged against that glyph's own run history and is conditioned
by composition so `12,345,678` does not teach punctuation pile-up. Same-glyph
continuations and mixed runs remain distinguishable in retained observations.

#### Absolute rarity

A very low corpus count produces a roster for review. Rarity is a list, not a
claim that a glyph is wrong. Accepted glyphs use suppression, not a threshold
distortion.

#### Dispersion

Books-touched versus books-possible is annotation and ranking only. Genre can
legitimately cluster punctuation, so dispersion does not convict. A small
“forgiven but clustered” review group may surface above-band patterns without
turning dispersion into a gate.

### Fraction bands

The initial readable staircase is a shipping candidate, not an eternal
constant:

| opportunity count | minority share eligible for review |
| --- | --- |
| under 5 | abstain; rarity owns the roster |
| up to 10 | 25% |
| up to 100 | 10% |
| up to 1,000 | 3% |
| up to 10,000 | 1% |
| above 10,000 | 0.3% |

Fleet evidence found this approximates `0.8 / sqrt(n)` and sits in a broad
valley between slip-shaped minorities and competing conventions. Keep the
five explicit bands for speakability. A master sensitivity control, if later
needed, moves a calibrated global mapping; it does not fit itself to the open
project.

Each evidence channel judges independently. A site fires when any entitled
channel passes. The UI may headline the strongest reason, but the finding
retains every independently sufficient reason and one run still yields one
finding.

### Required examples

- `ng'` used 851 times across books is learned as convention and stays silent.
- 818 letter-attached commas in only three books become one pattern row in the
  “forgiven but clustered” group, not 818 findings and not silence.
- `word?.` can fire on the exact pair when its specific opportunity set is
  strong even though the individual glyphs are common.
- `,..,` can fire on run length when its outer placement is ordinary.
- `...` in an ellipsis-writing corpus is excused by its entitled run history.
- A strange comma/backtick pair fires through the rare member when the common
  comma has no entitled exact-pair evidence.
- French guillemet spacing, Amharic `::`, uncased Amharic, and a glottal-stop
  apostrophe convention self-abstain without allow-lists.
- In a ten-verse draft, a 1-of-6 comma pattern is a cheap review rather than a
  small-sample silence.
- Low Line used 28 times across six books reads “rare but dispersed”; `}` used
  once or twice appears in the rarity roster.
- ZWJ/ZWNJ in Indic text never enters the nonletter inventory.

### PO checklist absorbed by this lane

The substrate covers spacing around punctuation, free-floating spacing-clone
marks, orphaned punctuation, repeated punctuation, punctuation/quote order,
phrase-ending marks at logical seams, word-medial punctuation, unexpected
glyphs, lowercase after a learned terminal, and digit-lane census shapes.
These are catalog views/reasons, not separate hot-loop rules.

## Level 2 — word conventions

Word work starts only after Level 1 observations and judgment have proven the
common substrate. It uses words in free positions; forced positions are
classified from the learned terminal table rather than from a hard-coded
punctuation list.

### Approved directions

- **Casing convention:** compare a case-folded word's forms in free positions.
  Bivariant words abstain. Uncased scripts pay almost nothing and stay silent.
- **Doubled words:** keep adjacent and punctuation-separated counters distinct.
  A recurring French `vous vous` convention can excuse itself; a lone residue
  remains reviewable. State crosses verse seams within a book.

Word-casing produced roughly eight times glyph-rule volume under shared bands
in the probe artifact. It needs an independently calibrated support floor or
band column before shipment; copying glyph defaults is blocked.

### Idea shelf, not approved rules

- character n-gram surprisal;
- hapax-rate context;
- word length against the corpus distribution;
- compound/split comparison against the corpus vocabulary.

These remain probes until each can state a narrow claim, fair comparison
population, counterexamples, and actionable result. “Not in the vocabulary”
is never sufficient; names, loanwords, and productive morphology are standing
counterexamples.

## Level 3 — source-compared rules

Source rules consume the aligned-unit contract in [charter.md](charter.md).
They are silent or return a typed refusal when a trustworthy pairing is not
available; they never pair by incidental array position.

### Length proportionality

For every nonempty paired verse unit, retain target/source grapheme-length
ratio and its projected target span. Judge the ratio against two distributions:

- ratios in the same book; and
- ratios in the whole paired project, so a short book can still be judged.

Absent or empty counterparts produce no ratio. Exact duplicate keys pair by
occurrence ordinal only when unambiguous. A bridge pairs directly with the same
bridge, or with the exact contiguous set of constituent verses on the other
side after those constituent texts are coalesced. The bridge contributes one
ratio over the two range totals; it is not divided into guessed per-verse
lengths or repeated in the distribution. Any partial overlap abstains.

The v1 donor uses a median with separate above/below median absolute
deviations, falling back to pooled MAD when one side has fewer than three
strict deviations. This remains the algorithm because the short side is
bounded by zero while the long side is open-ended. V2 retains the calibrated
defaults (`z_long = 3.5`, `z_short = 3.5`, `min_verses = 50`) and reproduces
the paired survey, pairing semantics, and seeded-fault behavior as regression
evidence rather than reopening the defaults without contrary evidence.

The finding may claim only: “this verse's length is unusual relative to this
declared source and the surrounding paired verses.” It cannot establish an
omission, mistranslation, or wrong language.

Known limits are part of the rule:

- empty target/source units have no ratio and need a separate presence check;
- 10–20% truncations were essentially undetectable in v1's seeded survey;
- source-language paste can have an ordinary length;
- results legitimately change with the chosen source;
- adjacent opposite extreme ratios may indicate versification shear, which is
  a separate structural/source-comparison observation rather than a length
  finding.

### Source-copy residue

An untranslated-word/source-copy rule may reuse aligned units but not the
ratio's statistics. It needs its own typed observation and excusal rules. Port
the old candidate only after its exact-token, normalization, case, proper-name,
and shared-vocabulary counterexamples are re-adjudicated.

### Presence and shear

Target-only/source-only aligned keys and adjacent opposite length extremes are
useful alignment facts, but are not length findings. The producer adapter
reports missing or incompatible units as typed facts; proportionality skips or
abstains. Sous owns evidence over content that was successfully paired. Any
future presence or shear finding requires a separate actionable claim and rule
contract rather than being hidden inside proportionality.

## Parked — delimiter pairing

Bracket and quote pairing is stack-shaped and does not fit the counter model.
The provisional direction is:

- if Unicode defines a bracket pair, recognize it by default;
- allow per-glyph/family opt-out through Galley suppression workflow;
- keep bounded chapter observations plus small pending-open seam state;
- do not infer quote roles or promise general nesting correctness;
- prefer honest limited support over losing chapter-granular rebuilds.

This needs a dedicated probe and rule contract before implementation.

## Explicitly outside Sous Chef

- marker validity and unsupported markers;
- repeated, missing, bridged, or out-of-order verse markers;
- book/chapter title and label consistency;
- expected verse counts from a versification authority;
- section-title placement and other layout-shaped checks;
- metadata comparison against an external publication catalog.

Onion or the editor owns these because they require structure, layout, or an
external authority rather than content convention.
