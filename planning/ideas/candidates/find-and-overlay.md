# Find + skeleton overlay — where they live (ruled with Will 2026-08-24)

## Find: GALLEY, not onion (ruled)

The vision (§4.2/§12.1) words "search and replacement planning across
hidden markup" as an Onion ask, but the engine's contribution already
exists: the mask (projection + two-way offset map). Galley's Find is:

    memmem / regex over mask.text()  →  hits in mask space
    → to_source per hit              →  source spans (possibly
                                         DISCONTIGUOUS across a
                                         footnote gap — the offset map
                                         says so honestly)
    → replacement = ordinary Edits at those source ranges

Deps: memchr::memmem for literal (free — the engine already carries
memchr); the `regex` crate if/when regex mode ships — a GALLEY dep,
like serde (app-flavored deps live in the bindings crate, never the
engine). aho-corasick only if a genuine many-patterns consumer arrives
(termlist highlighting — every Translation Word in one sweep; smells
like sous/galley). Note aho-corasick is built ON memchr (same
maintainer); regex uses both internally.

No new onion capability. Delete the "hidden engine ask" framing.

## Skeleton overlay: primitive in ONION, workflow in GALLEY (lean)

The job (vision §16.5 adjacent): pour an existing project's verse text
into a generated skeleton's verse slots — copy-structure, refill
content. Its heart is Sid-keyed alignment between two documents, which
is diff's identity layer re-aimed:

- both sides get a Toc; verses pair by the (book, chapter,
  verse-start) Copy key — diff's pairing_key, verbatim;
- output is SpliceEdits: insert the donor's verse-text range into the
  skeleton's slot. Zero owned text; apply_splices is the replay.

Engine-shaped (pure, two-input, deterministic, corpus-testable) and
small because diff built the hard parts — an onion candidate sketch
when the product promotes it. NOT engine-shaped: the policy — donor
text with no slot (extra verses, notes, headings), bridge mismatches,
leftover flagging vs dropping. That's the galley/editor command.

Prerequisite: the skeleton GENERATOR itself (vision §16.5 — \c/\v
scaffold from a versification; the one place never-synthesize inverts,
because generating is its whole job). Unsketched; product must promote
it first.
