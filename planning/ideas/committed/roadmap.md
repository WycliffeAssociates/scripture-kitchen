1. The attribute k/v interpreter (small, first, on purpose). It's the designator interpreter's sibling — pure span → judgment over an AttrList's interior, defined_attributes matching, the a-* wildcard, "later definition wins" merge. It's the single piece that unblocks two queues at once: exports need it for the k/v splatting (the lossy step), and the two owed lint rules (attr-unknown-name, attr-required-if) are waiting on exactly it. Building it standalone means both consumers get it tested rather than entangled.

2. Exports — USJ first, then USX, HTML last. USJ first because usfmtc gives us a free oracle: we can diff our fold's output against usfmtc's USJ over the whole corpus, the same move that's already paid off three times (nesting probes, the nd ruling). USX adds the eid-derivation quirks; HTML brings the authored aux tables (heading levels, caller rendering). Close the two attr lint rules in the same window since the interpreter's now in hand.

3. vref + the sous slab, as part of the export family. Same artifact at two granularities per the settled design. This is where "first bits of caching" become concrete — and the key point from the earlier ruling: the caching lives on sous's side, not the engine's. The slab exporter stays pure and total per call; sous's content-addressed stats store skips its own expensive walk on checksum hits. The engine-side chunk memoization (chunk-memoization.md) stays parked until a real measurement demands it — nothing in this step builds state into the engine.
--MAYBE A MASK INSTEAD TO CREATE? PROB WORTH SPITTING OUT A REAL VREF, BUT ALSO WORTH HAVING A MASKED VERISON I THINK, SO MIGHT JUST GENERATE FROM A MASK?

4. The diff port — with the offset-world question settled before porting. My lean on your question: port onion's algorithm wholesale (it's trusted), but re-speak its boundary in this repo's vocabulary — hunks as crate::edit::Edit lists over byte offsets, addressed by the content-derived book/chapter/verse coordinates the slab just gave us. That's why diff sits after exports rather than before: it gets its addressing and its output vocabulary for free, and a diff-as-Edit-list composes with everything that already exists (the CM session applies it like a fix; apply + the oracle pattern can even test it). If the port fights that boundary, the fallback is port-as-is and adapt later — but I'd try the offset skin first since edit.rs exists precisely for this.

5. Version-family lint — cheap, slots anywhere; the only real work is a small authored "deprecated-since-which-version" dataset (rows only carry the bool).

6. wasm analyze() when the editor prototype pulls for it — pure Rust until then, per the standing order.

7. Braid last — and, per its own trajectory, possibly nothing beyond "call the stateless analyze, debounced" plus multi-book concerns.
ATM UNDER THIS WORLD BRAID WOULD BE, CACHING WORK? INCREMENTAL UPDATES / TILING BINARY?
