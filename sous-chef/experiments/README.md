# Experiments

Competing approaches that were measured for speed or ergonomics. The winner
promotes into the library; the loser stays here as a plain `.rs` file that **no
crate compiles** — nothing in this folder is a Cargo target, a module, or a
dependency of anything.

The point is to keep "how did X measure?" answerable without paying build time
for an answer nobody calls. A file here is evidence, not code: it may not even
compile against today's library, and that is fine. Do not `include!` it, do not
add it to a `[[bench]]`, do not fix its warnings.

Every file opens with a header block:

- **Question** — what was being decided;
- **Date** — when it was measured;
- **Numbers** — the measurement, with the machine and command;
- **Verdict** — why it lost;
- **Winner** — where the shipped code lives now.

The measured rows themselves live in [`../evidence.md`](../evidence.md), which
is append-only. This folder holds the code those rows describe.

| file | question | verdict |
| --- | --- | --- |
| `unicode-lookup-flat-bmp.rs` | a lazy 128 KiB flat BMP array against the static two-level table | rejected on size at a near tie |
| `unicode-lookup-swar-decoding.rs` | the SWAR ASCII chunk over the decoding lookup instead of over the byte trie | rejected: taxes Indic and Greek to speed English |
| `needle-search.rs` | site-rescan crossover: memchr-per-needle vs memmem-per-needle vs one Aho-Corasick pass, by N and hit density, on three scripts | memmem per needle ships; memchr on a lead byte loses 6× on Devanagari; Aho-Corasick only pays at ≥10 rare needles per book |
