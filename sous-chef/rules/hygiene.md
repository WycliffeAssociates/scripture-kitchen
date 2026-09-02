# Level 1a — deterministic hygiene

Hygiene finds text states that are mechanically suspect independent of
language convention. It walks projected content at high speed and coalesces
maximal same-class runs. Deterministic lane: enable or disable, no bands, no
support floor.

Scan implementation: [`../core/src/hygiene.md`](../core/src/hygiene.md).
Wire lanes: [`../core/src/codec/README.md`](../core/src/codec/README.md).

## The shared contract, answered once

1. **Observation:** maximal same-class runs of the checks below, in projected
   content.
2. **Claim:** "this content contains bytes that are mechanically suspect."
3. **Not established:** intent. A tab-separated table or an escaped-backslash
   convention is content the reviewer may accept.
4. **Conditioning:** none. Every hit is its own evidence.
5. **Map/reduce:** the scan over one chapter's masked text is the whole
   observation. `Carry = ()` — a run is maximal within its chapter, and a run
   abutting a masked `\c` marker is two findings by design.
6. **Config:** none changes observations; enablement only filters.
7. **Wire:** class and run length ride the two payload lanes; the span is the
   run, snapped out to grapheme-atom edges.
8. **Pinned:** the tests named in each row below.

## Checks

Every emitted span is snapped out to grapheme-atom edges, so a finding never
splits a rendered grapheme (charter invariant 6). A run whose scalars hang off
a base therefore publishes a span one atom wider than its code-point count.

### Byte-pattern checks — no Unicode data

| check | observation | silent when | pinned by |
| --- | --- | --- | --- |
| `C0Control` | a C0 control run | tab, LF, and the CR of a valid CRLF pair | `two_hundred_twenty_three_nuls_produce_one_finding_spanning_the_run`, `tab_and_lf_pass_while_other_c0_and_del_are_runs_by_class` |
| `Delete` | U+007F | — | `tab_and_lf_pass_while_other_c0_and_del_are_runs_by_class` |
| `C1Control` | U+0080..=U+009F | the `C2` lead introduces any other scalar | `c1_and_replacement_runs_confirm_their_continuation_bytes` |
| `ReplacementChar` | U+FFFD | the `EF` lead introduces any other scalar | `c1_and_replacement_runs_confirm_their_continuation_bytes` |
| `StrayCarriageReturn` | CR not followed by LF | inside CRLF | `crlf_is_silent_and_a_stray_cr_is_reported` |
| `StrandedBackslash` | a backslash in content | Onion masked it out as a marker | `backslashes_in_content_are_reported_as_a_run` |
| `ConflictMarker` | a line-initial run of three or more `<`, `=`, or `>`; the whole line is the span | mid-line, or a run shorter than three | `conflict_markers_are_line_initial_runs_of_three_or_more` |

### Classifier-dependent checks

These read `sous-core::unicode` bits rather than approximating them with byte
rules, and each makes only a deterministic claim.

| check | observation | silent when | pinned by |
| --- | --- | --- | --- |
| `Noncharacter` | `U+FDD0..=U+FDEF`, every `U+xxFFFE`/`U+xxFFFF` | — | `noncharacters_are_reported_as_runs` |
| `FreeCombiningMark` | a `Mark` whose preceding scalar is absent, whitespace, a control, or a non-glue format character | the mark has a base — a decomposed grapheme is ordinary text | `a_decomposed_graphemes_combining_mark_is_not_a_free_mark`, `a_bare_combining_mark_after_a_space_or_at_the_start_is_reported` |
| `MisplacedFormat` | a `Cf` scalar outside any glue position | ZWJ/ZWNJ between letters, or a Prepend introducing what follows it — both are doing their defined job | `zwj_and_zwnj_between_letters_are_silent`, `a_stray_byte_order_mark_mid_text_is_reported` |
| `NoBreakSpace` | U+00A0 beside other whitespace, or at an edge of the analyzed text | NBSP inside a phrase, which is convention rather than damage | `nbsp_speaks_only_where_the_claim_is_deterministic` |

Cross-cutting: `clean_multilingual_text_is_silent` and
`every_emitted_span_lies_on_atom_boundaries`.

## Required examples

- 223 contiguous NUL bytes produce one finding spanning the run.
- CRLF is silent; a stray CR is reported.
- A backslash in masked-out USFM markup is silent; a stranded backslash in a
  content span is reported.
- A line-initial `<<<<<<< ours` line is reported; the same text mid-line is
  not.
- A decomposed grapheme's combining mark is not mistaken for a free mark; a
  bare U+0301 after a space is reported.
- ZWJ and ZWNJ between Indic letters are silent; a joiner with nothing to
  join is reported.
- U+FDD0 is reported; U+FFFD keeps its own class.
- A stray U+FEFF mid-verse is reported; NBSP inside a French phrase is not.

## Deferred

- **NBSP leading or trailing a *verse*.** `scan` sees one projected book, so
  "edge" currently means the edge of the analyzed text. The verse-grained form
  waits for the chapter/verse-aware walk in Stage 2.

## Not hygiene's job

Marker validity, empty marker structure, chapter/verse ordering, and metadata
consistency are Onion/editor responsibilities — see
[outside-sous.md](outside-sous.md). Through the Onion producer this includes
any lone backslash: Onion lexes it as a marker, well-formed or not, and masks
it out, so only a `\\` pair reaches Sous as content. A vref producer keeps
every backslash as content. "Empty verse content" may become a Sous check only
when Onion has already established a valid verse anchor and exposes an empty
analyzable unit.
