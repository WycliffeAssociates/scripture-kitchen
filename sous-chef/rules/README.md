# Rules

The review lanes Sous Chef 2 ships or plans, one file each. This page holds
the contract every lane answers and the index of where each lane lives.

Cross-cutting ownership and data contracts are in
[../charter.md](../charter.md); sequencing and gates are in
[../roadmap.md](../roadmap.md); measured numbers are in
[../evidence.md](../evidence.md).

## The shared rule contract

Every rule answers these eight questions before implementation:

1. What exact observation is made?
2. What narrow user-facing inference may it support?
3. What does it not establish, and what legitimate case resembles the error?
4. What are the conditioning variables, primary signal, opportunity count,
   support floor, and abstention conditions?
5. What is mapped per chapter, what boundary state is stitched per book, and
   what is judged across the corpus? These are the `Observation`, `Aggregate`,
   and `Config` of a `sous_core::ChapterPass`, and its `map`, `fold`, and
   `judge` — see [`../core/src/pass.md`](../core/src/pass.md).
6. Which config changes observations and which merely re-judge them?
7. Which raw counts or facts must reach the finding detail and compact wire
   digest?
8. What synthetic true case, counterexample, seam case, and cold-versus-edit
   equivalence test pin the claim?

## The four lanes

- **Deterministic:** the documented domain does not admit the condition.
  Enable/disable only; no sensitivity model.
- **Convention-learned:** the target corpus supplies a comparison population.
  Raw observations are retained and judgment is a transparent fraction.
- **Source-compared:** target and declared source pair through aligned units.
  Findings describe disagreement with that source, not absolute quality.
- **Census-only:** useful descriptive evidence that cannot honestly support an
  error-shaped finding.

## Index

| level | lane | file | kind | status |
| --- | --- | --- | --- | --- |
| 1a | hygiene | [hygiene.md](hygiene.md) | deterministic | landed (one deferred item) |
| 1b | character inventory | [character-inventory.md](character-inventory.md) | convention-learned | designed; Stage 3 |
| 2 | word conventions | [word-conventions.md](word-conventions.md) | convention-learned | designed; Stage 4, bands blocked |
| 3 | length proportionality | [length-proportionality.md](length-proportionality.md) | source-compared | landed (S1) |
| 3 | source-copy residue | [source-copy-residue.md](source-copy-residue.md) | source-compared | parked pending re-adjudication |
| 3 | presence and shear | [presence-shear.md](presence-shear.md) | source-compared | parked; needs its own claim |
| — | delimiter pairing | [delimiter-pairing.md](delimiter-pairing.md) | deterministic | parked pending a carry probe |
| — | not our job | [outside-sous.md](outside-sous.md) | — | outside Sous |

Implementation detail sits beside the code it describes:
[`../core/src/pass.md`](../core/src/pass.md),
[`../core/src/hygiene.md`](../core/src/hygiene.md),
[`../core/src/unicode/README.md`](../core/src/unicode/README.md),
[`../core/src/codec/README.md`](../core/src/codec/README.md),
[`../core/src/proportionality.md`](../core/src/proportionality.md).
