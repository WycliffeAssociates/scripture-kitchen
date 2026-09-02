# Parked — delimiter pairing

Bracket and quote pairing is stack-shaped and does not fit the counter model.

The provisional direction:

- if Unicode defines a bracket pair, recognize it by default;
- allow per-glyph/family opt-out through Galley suppression workflow;
- keep bounded chapter observations plus small pending-open seam state;
- do not infer quote roles or promise general nesting correctness;
- prefer honest limited support over losing chapter-granular rebuilds.

Status: parked. Needs a dedicated bounded-carry probe and a rule contract
before implementation.
