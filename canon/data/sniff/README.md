# Sniffing rules

`rules.json` is nida-institute/versification's
`versification-sniffing/rules/merged_rules.json`, copied unchanged at commit
`519af776ab65d89a51e37f750779250f940459ef`. It is derived from TVTMS
(STEPBible's Translators' Versification Traditions with Methodology for
Standardisation, Tyndale House, Cambridge). Licence and attribution:
`../../NOTICE`.

403 rules. Each names a passage where numbering traditions differ, the
traditions as columns, and per column the tests a text passes when it follows
that column:

```text
Gen.2:25=Last           the verse is its chapter's highest
Exo.8:28=Exist          the verse is there      (=NotExist: it isn't)
Gen.5:31.2=Exist        part 2 of the verse (USFM 31b) is there
Neh.4:6*2>Neh.4:3       twice 4:6's length beats 4:3's
```

How canon reads it, and where that differs from nida's sniffer:

- **Votes, not a custom scheme.** nida applies each passing rule's `ranges` to
  build a new versification for the text. canon votes for a named built-in
  scheme instead, so `ranges` is not read. The generator works out which
  columns each scheme fits by running the structural tests against the
  scheme's own counts.
- **Parts are parts.** nida parses `.2` and then tests the whole verse, so
  `Gen.5:31.2=Exist` passes for any text with 5:31. canon tests the part
  (`31b`), from the segments onion reads.
- **Abstention.** A rule sits out where the text can't answer it honestly:
  - a book or chapter the text lacks;
  - a `Last` or `NotExist` test in a chapter with a gap, a duplicate or a
    verse out of order;
  - a length test on a bridged or missing verse;
  - the four tests written over a range (`Pro.20:20-22`, `Luk.22:43-44`),
    and part 0, which the sniffer doesn't read.

To update, copy the new file over this one, rerun `cargo run -p canon-gen`,
and review `galley/tests/sniff.rs`, which pins how each example corpus sniffs.
