# Parked — delimiter pairing

Bracket and quote pairing is stack-shaped and does not fit the counter model.

The provisional direction:

- if Unicode defines a bracket pair, recognize it by default;
- allow per-glyph/family opt-out through Galley suppression workflow;
- keep bounded chapter observations plus small pending-open seam state;
- do not infer quote roles or promise general nesting correctness;
- prefer honest limited support over losing chapter-granular rebuilds.

## The bounded-carry shape

The first experiment handles balanced pairs inside one chapter at a fixed
maximum depth, with an explicit overflow state. On overflow, discard every
conclusion that depended on the lost stack context; a truncated stack must
never manufacture a missing-closer finding.

A cross-chapter extension needs an ordered summary of unmatched opens and
closes plus an exact rule for composing two of them. Counts alone do not
establish correct nesting. If the bounded summary cannot preserve that
distinction, keep the claim chapter-local or abstain rather than pretending
to be a whole-book parser.

Quotes stay separate: glyph role is ambiguous, nested speech crosses
paragraphs, apostrophes occur inside words, and Bible conventions vary. A
quote is not a bracket. Suppressions filter what is published; they do not
rewrite the observed balance facts.

Status: parked. Needs a dedicated bounded-carry probe and a rule contract
before implementation.
