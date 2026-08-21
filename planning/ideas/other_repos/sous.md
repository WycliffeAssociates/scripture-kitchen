# sous — the proofreading suite (sibling repo), as onion sees it

The editor/onion/sous lifecycle with respect to UTF-16, as a pseudo
call stack. The rules it encodes: every public offset is SOURCE BYTES;
a mask's consumer translates before emitting; UTF-16 exists at exactly
one wall — the editor's wire — via an index built over the one string
that crosses it.

```text
editor.on_change(utf16 edit)                     // JS, utf16 world
└─ onion.analyze(source)                         // UTF-8 bytes cross into wasm ONCE
   ├─ lex(source)                  -> tokens     // never serialized back out
   ├─ cst::build(&tokens)          -> cst        // syntax highlighting walks this
   ├─ lint(source, &tokens, &cst)  -> LintReport //   offsets: source bytes
   ├─ toc(source, &tokens)         -> Toc        //   sids without strings
   └─ mask(source, &tokens, &cst, Filter::verse_text()) -> Mask
      │
      └─ sous.proofread(mask.text(), &mask)      // Rust→Rust: no boundary, no utf16
         ├─ finds "doubled word"   -> mask bytes 12..17
         ├─ mask.to_source(12), mask.to_source(17)
         │                         -> source bytes 33..38
         │                            // mask space DIES here — never escapes sous
         └─ Diagnostic { at: 33..38 }             // indistinguishable from lint's

// back at the wire, one diagnostic stream, all in source bytes:
├─ toc.locate(diag.at)             -> "MRK 6:3"   // the human label
└─ Utf16Index(source).to_utf16(diag.at)           // THE one translation layer
   └─ editor.decorate(utf16 range, "MRK 6:3", severity)

// the reverse trips, same two maps, other direction:
editor cursor (utf16) ─ Utf16Index(source).to_byte ─ toc.locate ─> status bar sid
show a source diagnostic INSIDE the proofread view:
   mask.from_source(diag.at)  -> Some(mask byte) | None ("not in this view", honestly)
```

Costs, for intuition: Utf16Index(source) is ~1.6% of the source and
O(1) per query; Mask::to_source is a binary search over ~thousands of
ranges (~100 ns); the whole analyze() pipeline is ~25–40 ns/token
(perf-notes.md). Nothing here is cached; every artifact rebuilds per
call at those prices.

## Composition (ruled direction 2026-08-21: colorless, one module)

The stack above requires onion and sous LINKED INTO ONE WASM BINARY.
Both stay plain library crates — independently usable, each free to
carry its own thin bindings crate for standalone JS use — and the
COMPOSED experience is a third thin crate depending on both. That
crate is what "galley" (braid's stateful ancestor) becomes: the one
call that runs analyze + proofread, the retained source buffer, the
shared Utf16Index, caching, coordination — STATE LIVES AT THE
BINDINGS CRATE, never in the engines. Bindings crates are a few
hundred lines; three of them (onion-wasm, sous-wasm, galley) cost
nearly nothing because the libraries are the substance. The rejected
shape: editor ferrying the mask between two separate wasm modules — a
text copy + encode per hop, justified only if the two must deploy
independently. (The engine needed NO porting for wasm: pure Rust, no
IO — the measurement probe compiled it to wasm32 unchanged; the
roadmap's "wasm analyze()" item is only the API surface.)
