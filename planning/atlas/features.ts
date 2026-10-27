/**
 * The /features spec sheet's hand-written rows. Counts that a checked-in file
 * can give (lint codes, Sous message ids, wire layouts) are not here: the page
 * reads them at build. `pnpm atlas check` fails on a `concept` that is no
 * concept page, a `cite` that no longer resolves, or an `item` that is no plan.
 *
 *   { name: "USJ", note: "the CST as USJ", concept: "exports", door: "usj(…)", cite: "onion/src/usj.rs#usj", tag: "Rust" }
 *     → USJ — the CST as USJ  [usj(…)]→GitHub lines  Rust     name links /atlas/exports
 */
export interface FeatureItem {
  readonly name: string;
  readonly note: string;
  /** A concept slug: the name links to /atlas/<slug>. */
  readonly concept?: string;
  /** The call as a host writes it. Shown as code, linked to `cite`. */
  readonly door?: string;
  /** A `[[code:…]]` target, resolved by src/lib/code-cite.ts. */
  readonly cite?: string;
  /** A site path the name links to instead of the concept page. */
  readonly href?: string;
  /** A planning item slug, linked as the plan. */
  readonly item?: string;
  /** Where it is reachable when that is not galley: "Rust", "CLI", "not built". */
  readonly tag?: string;
}

export interface FeatureGroup {
  readonly id: string;
  readonly title: string;
  readonly items: readonly FeatureItem[];
}

/** Notes for the Sous message families, keyed by catalog id prefix (galley/sous-messages.en.json). */
export const SOUS_NOTES: Readonly<Record<string, { name: string; note: string; concept?: string }>> = {
  hygiene: { name: "Hygiene", note: "invisible, stray or lost characters", concept: "sous-core" },
  presence: { name: "Presence", note: "verse missing, extra or empty against the source", concept: "source" },
  length: { name: "Length", note: "verse long or short for its source, banded per project", concept: "source" },
  sourceCopy: { name: "Source copy", note: "runs of source words left untranslated; off by default", concept: "source" },
  "convention.exactNeighbor": { name: "Glyph neighbour", note: "a glyph beside one the project rarely puts it next to", concept: "sous-chef" },
  "convention.pooledNeighbor": { name: "Pooled neighbour", note: "a glyph beside a quote, bracket or dash class it rarely touches", concept: "sous-chef" },
  "convention.runShape": { name: "Run shape", note: "a group of marks rare as a whole, or reordered", concept: "sous-chef" },
  "convention.placement": { name: "Placement", note: "a glyph on the uncommon side of a word or digit", concept: "sous-chef" },
  "convention.rarity": { name: "Rarity", note: "a glyph the project almost never uses, lookalikes named", concept: "sous-chef" },
  "convention.casing": { name: "Casing", note: "a word cased unlike its usual form here", concept: "sous-chef" },
  "convention.wordLength": { name: "Word length", note: "a word much longer than most in the project", concept: "words" },
  "convention.doubled": { name: "Doubling", note: "a word written twice in a row", concept: "words" },
  "convention.letterRun": { name: "Letter run", note: "one letter repeated past the project's longest", concept: "words" },
  "convention.sentenceStart": { name: "Sentence start", note: "lowercase after a mark the project capitalizes after", concept: "sous-chef" },
  "convention.bookRate": { name: "Book rate", note: "a pairing far commoner in one book than the others", concept: "sous-chef" },
};

/** Groups before Lint. */
export const BEFORE: readonly FeatureGroup[] = [
  {
    id: "import",
    title: "Import",
    items: [
      { name: "USFM", note: "only input; any bytes parse, nothing is repaired", concept: "cst", door: "parse(text, opts)", cite: "onion-wasm/src/lib.rs#parse" },
      { name: "USFM version", note: "\\usfm 3.0 / 3.2 / 4.0 sets deprecation severity and attribute rules", concept: "lint", cite: "onion/src/lint.rs#parse_version" },
      { name: "Custom markers", note: "\\z markers from markers.ext or a host list, per book", concept: "extensions", door: "customMarkersFromMarkersExt(text)", cite: "onion-wasm/src/lib.rs#custom_markers_from_markers_ext" },
      { name: "vref source", note: "BOOK C:V⇥text as a declared source for Sous", concept: "source", door: "sous --source x.vref", cite: "sous-chef/cli/src/args.rs#Args", tag: "CLI" },
      { name: "USJ → USFM", note: "sketched, not scheduled", concept: "exports", item: "2026-08-20-usj-import-sketch-usj-usfm-follow-on-to-usj-export-not-sched", tag: "not built" },
      { name: "USX → USFM", note: "no plan", concept: "exports", tag: "not built" },
    ],
  },
  {
    id: "export",
    title: "Export",
    items: [
      { name: "USJ", note: "lossy by design; corpus-checked against the reference", concept: "exports", door: "usj(src, tokens, cst)", cite: "onion/src/usj.rs#usj", tag: "Rust" },
      { name: "USX", note: "same fold, XML writer", concept: "exports", door: "usx(src, tokens, cst)", cite: "onion/src/usx.rs#usx", tag: "Rust" },
      { name: "HTML", note: "a view; no oracle, no round trip", concept: "exports", door: "html(src, tokens, cst)", cite: "onion/src/html.rs#html", tag: "Rust" },
      { name: "vref", note: "one line per verse slot, ebible shape; bridges give <range>", concept: "vref", door: "vref::joined(toc, mask, src, trim)", cite: "onion/src/vref.rs#joined", tag: "Rust" },
      { name: "Masked vref", note: "every vref takes a mask: verse text, or notes folded in", concept: "vref", door: "vref::lines(toc, mask, src, trim)", cite: "onion/src/vref.rs#lines", tag: "Rust" },
      { name: "Sous report", note: "self-contained HTML of every pattern and its sites", concept: "sous-cli", door: "sous --report out.html", cite: "sous-chef/cli/src/args.rs#Args", tag: "CLI" },
    ],
  },
  {
    id: "masks",
    title: "Projections (masks)",
    items: [
      { name: "Verse text", note: "text inside verses, no headings, notes or markup", concept: "mask", door: "galley.verseText(id)", cite: "onion/src/mask.rs#Filter::verse_text" },
      { name: "All text", note: "every text run, markup dropped", concept: "mask", door: "mask(id, { recipe: \"text\" })", cite: "onion/src/mask.rs#Filter::text" },
      { name: "Structure", note: "markers kept, prose dropped", concept: "mask", door: "galley.structureTextOf(text)", cite: "onion/src/mask.rs#Filter::structure" },
      { name: "Custom filter", note: "Keep / Unwrap / Remove per marker or kind", concept: "mask", door: "Filter", cite: "onion/src/mask.rs#Action", tag: "Rust" },
      { name: "Mask map", note: "projected offset ↔ source offset", concept: "mask-map", door: "galley.mask(id, opts)", cite: "galley/src/wasm.rs#Galley::mask" },
    ],
  },
  {
    id: "indexes",
    title: "Indexes & lookups",
    items: [
      { name: "TOC", note: "chapters, verses, what each verse covers", concept: "toc", door: "galley.toc(id)", cite: "galley/src/wasm.rs#Galley::toc" },
      { name: "Census", note: "every book's TOC without a parse", concept: "census", door: "galley.tocAll({ scope })", cite: "galley/src/wasm.rs#Galley::toc_all" },
      { name: "Resolve", note: "address → found, missing or ambiguous place", concept: "resolve", door: "galley.resolve(id, address)", cite: "galley/src/wasm.rs#Galley::resolve" },
      { name: "Locate", note: "offset → sid (\"GEN 1:3\")", concept: "toc", door: "locate(text, utf16)", cite: "onion-wasm/src/lib.rs#locate" },
      { name: "Designators", note: "verse and chapter numbers, bridges, segments", concept: "designator", cite: "onion/src/designator.rs#verse", tag: "Rust" },
      { name: "UTF-16 offsets", note: "byte ↔ editor offset; utf16 option on every door", concept: "utf16", door: "toUtf16 / toByte", cite: "onion-wasm/src/lib.rs#to_utf16" },
      { name: "Marker table", note: "every marker's category, context, attributes", concept: "marker-table", cite: "onion/src/tables/rows.rs" },
      { name: "Attributes", note: "attribute list → name/value spans; name → marker row", concept: "attributes", door: "attrs / attrResolve", cite: "onion-wasm/src/lib.rs#attrs" },
      { name: "Book code", note: "the first \\id's code", concept: "toc", door: "book(text)", cite: "onion-wasm/src/lib.rs#book" },
      { name: "Change check", note: "per-chunk checksum; which chunks changed", concept: "chunk", door: "galley.changedSinceUpdate(id, text)", cite: "galley/src/wasm.rs#Galley::changed_since_update" },
    ],
  },
];

/** Groups after Lint, before Sous. */
export const AFTER: readonly FeatureGroup[] = [
  {
    id: "format",
    title: "Format",
    items: [
      { name: "Edits", note: "a transaction of UTF-16 edits, not a new string", concept: "format", door: "formatEdits(text, opts)", cite: "onion-wasm/src/lib.rs#format_edits" },
      { name: "Range", note: "only the edits inside a selection", concept: "format", door: "formatEditsIn(text, from, to, opts)", cite: "onion-wasm/src/lib.rs#format_edits_in" },
      { name: "Whole text", note: "the edits applied", concept: "format", door: "format(text, opts)", cite: "onion-wasm/src/lib.rs#format" },
      { name: "Whitespace canon", note: "7 rules, on by default: block marker own line, blank lines, newlines, edges, delimiters", concept: "format", cite: "onion/src/format.rs#FormatOptions" },
      { name: "Line shape", note: "verse breaks keep/remove, char-marker breaks keep/join, LF/CRLF", concept: "format", cite: "onion/src/format.rs#VerseBreaks" },
      { name: "Content rows", note: "dedupe verse number, bridge empty verses; off by default", concept: "format", cite: "onion/src/format.rs#FormatOptions" },
      { name: "Remove markers", note: "delete every \\s5 (or any named marker)", concept: "format", door: "opts.setRemoveMarkers(names)", cite: "onion-wasm/src/lib.rs#FormatOpts::set_remove_markers" },
      { name: "Repairs", note: "chosen lint fixes join the transaction", concept: "format", door: "opts.setRepairs(codes)", cite: "onion-wasm/src/lib.rs#FormatOpts::set_repairs" },
    ],
  },
  {
    id: "diff",
    title: "Diff & merge",
    items: [
      { name: "Diff skeleton", note: "units = front matter, chapter open, verse; text runs none/words/chars", concept: "diff", door: "diff(baseline, current, opts)", cite: "onion-wasm/src/lib.rs#diff" },
      { name: "Merge", note: "pick a side per unit; a stale id throws", concept: "diff", door: "merge(baseline, current, decisions, default)", cite: "onion-wasm/src/lib.rs#merge" },
      { name: "Splice edits", note: "the merge as edits an editor applies", concept: "edit", door: "mergeSplices(…)", cite: "onion-wasm/src/lib.rs#merge_splices" },
      { name: "Overlay", note: "a source's paragraph/poetry breaks onto its target, as edits", concept: "overlay", door: "galley.overlay(id, sourceId, opts)", cite: "galley/src/wasm.rs#Galley::overlay" },
      { name: "Block skeleton", note: "both sides' blocks, paired, with node lookups", concept: "overlay", door: "galley.skeleton / targetNodeFor / sourceNodeFor", cite: "galley/src/wasm.rs#Galley::skeleton" },
    ],
  },
  {
    id: "find",
    title: "Find",
    items: [
      { name: "One book", note: "literal, over verse text; hits span markup", concept: "find", door: "galley.find(id, needle, opts)", cite: "galley/src/wasm.rs#Galley::find" },
      { name: "Project", note: "scope: targets, references or all", concept: "find", door: "galley.findAll(needle, opts)", cite: "galley/src/wasm.rs#Galley::find_all" },
      { name: "Options", note: "caseSensitive, wholeWord, limit", concept: "find", door: "FindOptions", cite: "galley/src/find.rs#Find::whole_word" },
      { name: "Regex", note: "literal only until a consumer asks", concept: "find", tag: "not built" },
    ],
  },
];

/** The Sous group's rows that the message catalog does not list. */
export const SOUS_EXTRA: readonly FeatureItem[] = [
  { name: "Run", note: "judge every registered book; one findings snapshot", concept: "expediter", door: "galley.publish()", cite: "galley/src/wasm.rs#Galley::publish" },
  { name: "Settings", note: "per-check knobs; a change costs a re-judge", concept: "expediter", door: "galley.setConfig(settings)", cite: "galley/src/wasm.rs#Galley::set_config" },
  { name: "Typos", note: "rare word one edit from a common one; on demand", concept: "words", door: "sous --typos", cite: "sous-chef/core/src/typos.rs", tag: "CLI" },
];

export const HOSTS: FeatureGroup = {
  id: "hosts",
  title: "Hosts",
  items: [
    { name: "usfm-galley", note: "every door plus the resident handle; what Sefer vendors", concept: "galley-package", door: "import { Galley } from \"usfm-galley\"", cite: "galley/src/wasm/onion.rs" },
    { name: "Builds", note: "bundler (.) and web (./web); readers as .ts subpaths", concept: "galley-package", cite: "galley/build.sh" },
    { name: "onion-wasm", note: "the stateless doors alone, same builds", concept: "onion-wasm", cite: "onion-wasm/src/lib.rs" },
    { name: "sous", note: "Sous over a book or folder from a shell", concept: "sous-cli", door: "sous --findings --publish out.sous book.usfm", cite: "sous-chef/cli/src/args.rs#Args", tag: "CLI" },
    { name: "Rust crates", note: "onion, mise, galley, sous-core", concept: "onion", cite: "Cargo.toml" },
  ],
};
