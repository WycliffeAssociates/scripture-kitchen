# Choices

Per-pass decision ledger. Newest section first.

## 2026-09-04 — test-loop shrinkage: pre-shrink evidence

Frozen before `testData/exampleCorpora` loses `en_ult`. Plan:
`planning/ideas/candidates/test-loop-shrinkage-plan.md`.

### Baseline timings (debug, warm, M1 Max)

    warm `cargo test --no-run`   1.7 s (mbx serving 496 compilations)
    `cargo test` wall          275 s

    221.52 lint_corpus      1.16 fast_path_identity   0.39 usx_corpus
     13.60 format_corpus    0.95 vref_corpus          0.35 usj_corpus
     11.17 mask_oracle      0.84 attr_corpus          0.31 unicode_generator
      7.77 atom_conformance 0.75 utf16_oracle         0.31 sous_core
      5.79 html_corpus      0.69 toc_oracle           0.14 chain_oracle
      1.76 usfm_onion       0.64 equivalence          0.11 mise
      1.38 cst_oracle       0.61 partition_oracle     0.11 hygiene_scalar_reference
                            0.57 cli                  0.09 diff_corpus
                            0.55 utf16_corpus         0.06 diff_laws

### `playground --lint-stats testData/exampleCorpora`, 226 books

    playground: loaded 226 source(s), 113633979 bytes total
    lint-stats docs=226 tokens=7126524 findings=20821 with-fix=7074 books-without-id=1
      unclosed-note x2 (2 with a fix)
      orphan-closer x1 (1 with a fix)
      unknown-marker x13636 (0 with a fix)
      missing-paragraph x5434 (5434 with a fix)
      designator-malformed x1 (0 with a fix)
      verse-duplicate x1 (0 with a fix)
      verse-gap x28 (0 with a fix)
      missing-verse-one x1 (0 with a fix)
      missing-id x1 (0 with a fix)
      verse-without-designator x1 (0 with a fix)
      numbering-mix x52 (0 with a fix)
      delimiter-shape x1 (0 with a fix)
      empty-paragraph x787 (762 with a fix)
      delimiter-surplus x875 (875 with a fix)
      unclosed-char x0
      unclosed-at-eof x0
      unterminated-container x0
      unterminated-milestone x0
      orphan-terminator x0
      orphan-container-end x0
      content-outside-sidebar-rule x0
      nested-spelling-misuse x0
      chapter-duplicate x0
      chapter-out-of-order x0
      chapter-gap x0
      verse-out-of-order x0
      verse-before-first-chapter x0
      missing-chapter x0
      book-code-unknown x0
      book-code-not-uppercase x0
      chapter-without-designator x0
      ca-cp-placement x0
      va-vp-placement x0
      caller-shape x0
      marker-not-ws-preceded x0
      attr-trailing-form-deprecated x0
      attr-both-lists x0
      attr-terminator-mismatch x0
      attr-pipe-hint x0
      attr-unknown-name x0
      attr-malformed x0
      attr-required-if x0
      deprecated-marker x0
      deprecated-attribute x0
      marker-out-of-band x0
      duplicate-id x0
      paragraph-before-first-chapter x0
      duplicate-usfm x0
      remove-marker x0
      bridge-empty-verses x0
      dedupe-verse-number x0
      block-marker-own-line x0
      char-marker-line-join x0
      collapse-blank-lines x0
      normalize-newlines x0
      trim-text-edges x0
      delimiter-single x0
      designator-ws-single x0
      marker-ws-at-line-start x0

### Corpus manifest, 226 books (path, bytes)

Full listing: `planning/pre-shrink-manifest.txt`. Totals:

    bdf_reg: 33 files
    en_ulb: 74 files
    en_ult: 78 files
    examples.bsb: 74 files
    testData/exampleCorpora/bdf_reg  1.6M
    testData/exampleCorpora/en_ulb  4.5M
    testData/exampleCorpora/en_ult  99M
    testData/exampleCorpora/examples.bsb  4.7M

### After the shrink — the same sweep over the new tier

    playground: loaded 160 source(s), 12798725 bytes total
    lint-stats docs=160 tokens=723998 findings=19893 with-fix=6151 books-without-id=1
      unclosed-note x2 (2 with a fix)
      orphan-closer x1 (1 with a fix)
      unknown-marker x13636 (0 with a fix)
      missing-paragraph x5403 (5403 with a fix)
      designator-malformed x1 (0 with a fix)
      verse-duplicate x1 (0 with a fix)
      verse-gap x28 (0 with a fix)
      missing-verse-one x1 (0 with a fix)
      missing-id x1 (0 with a fix)
      verse-without-designator x1 (0 with a fix)
      numbering-mix x50 (0 with a fix)
      delimiter-shape x1 (0 with a fix)
      empty-paragraph x728 (706 with a fix)
      delimiter-surplus x39 (39 with a fix)

Old → new pins (every one regenerated and re-explained in the test comments):

    books                     226 → 160        bytes    113.6 MB → 12.8 MB
    missing-paragraph       5,434 → 5,403      numbering-mix       52 → 50
    empty-paragraph           787 → 728        its fixes          762 → 706
    delimiter-surplus         875 → 39         all findings    20,821 → 19,893
    all fixes               7,074 → 6,151      format edits   108,979 → 88,648
    attribute lists     1,253,766 → 27,033     attributes   4,352,929 → 101,362

Every pinned QUIRK survived: the classes that carry them (unclosed-note 2,
orphan-closer 1, designator-malformed 1, verse-duplicate 1,
verse-without-designator 1, missing-verse-one 1, missing-id 1, verse-gap 28,
unknown-marker 13,636) are unchanged. Only en_ult's own itemised sub-counts
left, so en_ulb and bdf_reg were kept whole rather than excerpted — see the Log
in `planning/ideas/candidates/test-loop-shrinkage-plan.md`.
