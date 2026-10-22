/* tslint:disable */
/* eslint-disable */

/** A `\category` word, as the USFM spec spells it. */
export type CustomMarkerCategory =
| "header" | "title" | "introduction" | "sectionpara" | "versepara" | "list" | "otherpara"
| "crossreference" | "footnote"
| "char" | "introchar" | "listchar" | "footnotechar" | "crossreferencechar"
| "milestone" | "attribute" | "cell" | "standalone" | "internal";

/** One user `\z` marker. */
export interface CustomMarker {
    name: string;
    category: CustomMarkerCategory;
    /** The `markers.ext` description; shown, never lexed by. */
    description?: string;
    /** The attribute names `markers.ext` lists; shown, never lexed by. */
    attributes?: readonly string[];
}

/** An entry a list could not keep. `line` only where a file gave one. */
export interface MalformedCustomMarker {
    name?: string;
    reason: string;
    line?: number;
}

declare const customMarkersBrand: unique symbol;

/** A judged list, from `customMarkers` or `customMarkersFromMarkersExt`. */
export type CustomMarkers = {
    /** The entries kept; every door lexes by exactly these. */
    readonly markers: readonly CustomMarker[];
    readonly relaxZPrefix: boolean;
    /** The entries refused, and why. */
    readonly malformed: readonly MalformedCustomMarker[];
} & { readonly [customMarkersBrand]: "CustomMarkers" };



/** `parse`'s options, and `Galley.parse`/`parseText`'s. Every key defaults to false. */
export interface ParseOptions {
    /** Run the lint walk. The one expensive optional. */
    diagnostics?: boolean;
    /** Build the chapter and verse index. Cheap, and needs no tree. */
    toc?: boolean;
    /** Emit every offset as a UTF-16 code unit instead of a byte. */
    utf16?: boolean;
}

/** The list a door that takes text lexes `\z` markers by. */
export interface LexOptions {
    /** Default: the list `setCustomMarkers` installed. */
    customMarkers?: CustomMarkers;
}

/** `parse(text, opts)`, and galley's `parseText`. */
export interface ParseTextOptions extends ParseOptions, LexOptions {}

/** `diff`'s options. */
export interface DiffOptions extends LexOptions {
    /** The runs a changed unit is highlighted by: UAX-29 words, graphemes, or none. */
    text: "none" | "words" | "chars";
    /** Send unchanged units too, in their places; default false. */
    unchanged?: boolean;
}

/** `attrs`'s options. */
export interface AttrsOptions {
    /** `from`/`to` and every word returned are UTF-16 code units; default bytes. */
    utf16?: boolean;
}

/** galley's `resolve(id, address, opts)`. */
export interface ResolveOptions {
    /** Offsets as UTF-16 code units; default bytes. */
    utf16?: boolean;
}

/** `resolve(text, address, opts)`, and galley's `resolveOf`. */
export interface ResolveTextOptions extends ResolveOptions, LexOptions {}

/** A place in scripture, Sefer's `Address`. A `book` key is read past: the book is the caller's to match. */
export type Address =
| { kind: "book" }
| { kind: "intro" }
| { kind: "chapters"; from: number; to: number }
| { kind: "verses"; from: AddressPoint; to: AddressPoint };

export interface AddressPoint {
    chapter: number;
    verse: number;
    /** `a` in `3a`; absent for the whole verse. */
    segment?: string;
}

/** What `resolve` answers, JSON-encoded. Offsets are half-open. */
export type Resolution =
| { found: { from: number; to: number; opening: number; covers: Address; coarser: boolean } }
| { missing: "intro" | "chapter" | "verse"; within?: { from: number; to: number } }
| { ambiguous: { from: number; to: number; opening: number }[] };

/** `customMarkers`'s options. */
export interface CustomMarkerOptions {
    /** Admit a legacy name without the `z` that the spec does not define. */
    relaxZPrefix?: boolean;
}



/**
 * One transaction of proposed splices: `[from, to]` pairs, one concatenated
 * ASCII insert blob, one length per edit.
 *
 * The same shape a fix crosses in — an editor session applies both the same
 * way, and `lens[i] == 0` is a pure deletion. `spans` and `lens` are always
 * in the SAME unit, because `lens` slices `text` at offsets `spans` place:
 * the formatter's doors here answer in UTF-16 throughout, and a producer in
 * another crate names its own unit (`galley`'s overlay answers bytes unless
 * asked for UTF-16).
 *
 * **At most one edit per position.** A pure insert landing where the edit
 * before it ends is folded into that edit, so an applier's order among
 * same-position edits can never matter:
 *
 * ```text
 * engine    [66,66) "\n\q2"  [66,66) "\n\q3"  [66,66) "\n\q4"
 * crosses   [66,66) "\n\q2\n\q3\n\q4"
 * ```
 *
 * Applied either way the text is the same; only the entry count differs.
 */
export class Edits {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    readonly lens: Uint32Array;
    readonly spans: Uint32Array;
    readonly text: string;
}

/**
 * The formatter's switches, defaulted to [`FormatOptions::default`].
 *
 * A tagged struct rather than a dozen positional booleans: the `.d.ts` names
 * each switch, and adding one later does not renumber a call site.
 */
export class FormatOpts {
    free(): void;
    [Symbol.dispose](): void;
    constructor();
    /**
     * The list the text lexes `\z` markers by, instead of the one
     * `setCustomMarkers` installed. Throws where a door refuses its
     * `customMarkers` option.
     */
    setCustomMarkers(markers: CustomMarkers): void;
    /**
     * Marker names, no backslash, comma-separated — `"s5"` for the
     * unfoldingWord chunk marker. Every occurrence is deleted outright.
     */
    setRemoveMarkers(names: string): void;
    /**
     * Lint codes (the `diagnostics.json` indices) whose existing fixes join
     * the format transaction. An index naming no code is ignored.
     */
    setRepairs(codes: Uint32Array): void;
    block_marker_own_line: boolean;
    bridge_empty_verses: boolean;
    /**
     * 0 = keep a break on a character-marker boundary, 1 = join it into a
     * space (the aligned-corpus shape).
     */
    char_marker_breaks: number;
    collapse_blank_lines: boolean;
    dedupe_verse_number: boolean;
    delimiter_single: boolean;
    designator_ws_single: boolean;
    marker_ws_at_line_start: boolean;
    /**
     * 0 = LF, 1 = CRLF.
     */
    newline: number;
    normalize_newlines: boolean;
    trim_text_edges: boolean;
    /**
     * 0 = keep the line break in front of a `\v`, 1 = fold it into a space.
     */
    verse_breaks: number;
}

/**
 * The same merge as replay splices over the BASELINE — the hot-path shape, for
 * an editor that would rather apply a transaction than replace a document.
 *
 * `spans` is `[from, to]` per splice in BASELINE UTF-16; `inserts` is
 * `[from, to]` per splice in CURRENT UTF-16, the text to put there. An empty
 * insert is a deletion; `from == to` in `spans` is an insertion.
 */
export class Splices {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    readonly inserts: Uint32Array;
    readonly spans: Uint32Array;
}

/**
 * One attribute name against one marker row: the `AttrResolution` code.
 *
 * An empty name is the bare default-value form, which resolves through the
 * row's own default — so it is a legitimate argument, not a mistake.
 */
export function attrResolve(name: string, marker: number): number;

/**
 * The k/v view of one `AttrList` token, flat.
 *
 * `[from, to)` is the list token's span in the CALLER's space — UTF-16 under
 * `{ utf16: true }`, bytes otherwise — and every word comes back in that same
 * space.
 *
 * ```text
 * |lemma="grace" x-y="z"   ->  [nameFrom nameTo valueFrom valueTo] x 2, NONE, NONE
 * |grace                   ->  one quadruple, its name span EMPTY at the value
 * |lemma="grace            ->  no quadruple, then UnterminatedQuote and the quote's offset
 * ```
 *
 * Four words per attribute, then two: the `MalformedAttr` code and its
 * offset, both `NONE` when the list parsed clean. A malformed tail is always
 * the last event, so it can only be the last pair of words.
 */
export function attrs(text: string, from: number, to: number, opts?: AttrsOptions): Uint32Array;

/**
 * The first `\id`'s book code — `"GEN"`. EMPTY when the document declares
 * none (real in the wild: BSB Ecclesiastes); the `missing-id` diagnostic is
 * where that becomes a finding, not here.
 */
export function book(text: string, opts?: LexOptions): string;

/**
 * A list of user markers, judged once and held as plain data.
 *
 * ```js
 * const mine = customMarkers([{ name: "zmyf", category: "footnote" }]);
 * mine.malformed;                                            // → []
 * galley.update("books/MRK.usfm", text, { customMarkers: mine });
 * parse(refText, { customMarkers: mine });
 *
 * customMarkers([{ name: "s5", category: "standalone" }], { relaxZPrefix: true });
 * ```
 *
 * `markers` holds the entries kept and `malformed` the ones refused: no name,
 * a name that is not `z`-initial, an unknown category word, a duplicate. A
 * bad entry costs only itself. Installs nothing.
 *
 * Throws when `list` is not an array of `{ name, category }` strings (with
 * an optional string `description` and string-array `attributes`), or
 * `opts` is not `CustomMarkerOptions`.
 */
export function customMarkers(list: readonly CustomMarker[], opts?: CustomMarkerOptions): CustomMarkers;

/**
 * A `markers.ext` file, read into the same object [`custom_markers`] makes,
 * with `relaxZPrefix` false.
 *
 * ```js
 * const mine = customMarkersFromMarkersExt(text);
 * for (const { name, reason, line } of mine.malformed) show(line, name, reason);
 * galley.update("books/MRK.usfm", text, { customMarkers: mine });
 * ```
 *
 * A bad entry costs only itself and lands in `malformed` with its line; the
 * file never fails as a whole. Installs nothing.
 */
export function customMarkersFromMarkersExt(text: string): CustomMarkers;

/**
 * The diff skeleton as JSON: the changed units of two documents, in the
 * current document's order. Spans are UTF-16 offsets into each side's own document.
 *
 * ```ts
 * interface DiffOptions {
 *   text: "none" | "words" | "chars";   // required
 *   unchanged?: boolean;                // send unchanged units too; default false
 * }
 * diff(baseline: string, current: string, opts: DiffOptions): string;
 * ```
 *
 * `text` is the intra-verse runs a `modified`, `added` or `deleted` unit is
 * highlighted by, at UAX-29 word or grapheme grain; `"none"` computes nothing
 * — no CST, no mask. It is required: which of the three a screen draws is the
 * caller's to say. An unknown key, a wrong type or an unknown mode throws by
 * name.
 *
 * `unchanged: true` adds every unchanged unit in its place, for a view that
 * draws the whole book from the skeleton. Nothing else changes: the ids, the
 * spans and the runs are the same in both.
 */
export function diff(baseline: string, current: string, opts: DiffOptions): string;

/**
 * The formatted document. `format_edits` applied, in one call.
 */
export function format(text: string, opts: FormatOpts): string;

/**
 * The formatter's edit list, offsets converted to UTF-16 here.
 *
 * The index is built per call: an edit list is small and the conversion is
 * random-access (edits are sorted, but a stride index is ~0.1ms and this is a
 * user-triggered path, not a keystroke one).
 */
export function formatEdits(text: string, opts: FormatOpts): Edits;

/**
 * The same transaction bounded to `from..to` (UTF-16, the offsets the editor
 * already holds — a chapter's span out of the `chapters` read).
 *
 * The range crosses the wall in UTF-16 and is translated here, on the same
 * index the edits go out through. The policy is the library's: an edit is kept
 * only if its whole span is inside, a multi-edit claim only if all of it is,
 * and a pure insertion sitting ON either edge is inside.
 */
export function formatEditsIn(text: string, from: number, to: number, opts: FormatOpts): Edits;

/**
 * The reference at a CodeMirror offset — `"MRK 6:3"`, `"MRK 6:1-3"` for a
 * bridge, `"MRK 6"` ahead of a chapter's first verse, `"MRK"` in front matter,
 * `"###"` when the book declares no `\id`.
 *
 * TOTAL: an offset past the end names the last chapter rather than nothing,
 * because every caller of this is labelling a position it already has.
 *
 * One of the two exports on the sketch's method list that the reads
 * could not supply: the sid RENDERING is a format with rules (bridges, absent
 * chapters, an unknown book), and a JS re-implementation of it in every
 * consumer is the drift the no-strings-cross rule exists to prevent.
 */
export function locate(text: string, utf16: number, opts?: LexOptions): string;

/**
 * One mask recipe's text, and the map back to the source it was cut from.
 *
 * Not a section of a dish: a mask is a different question with its own
 * parameter, and most callers never want one. Not hot and not large either,
 * so it takes the string like any other call rather than the buffer.
 *
 * `ranges` are the kept SOURCE spans — sorted, disjoint, maximal — and
 * `starts[i]` is the prefix sum, so `ranges[i]`'s bytes sit at `starts[i]..`
 * in `text`. That pair is the map: a finding at a masked offset maps back by
 * a binary search on `starts`.
 *
 * Offsets stay in UTF-8 bytes. The mask is onion-to-sous and never reaches an
 * editor, which is the only consumer that counts in UTF-16.
 */
export function mask(text: string, recipe: string, opts?: LexOptions): object;

/**
 * The merged document. An unknown unit id REJECTS loudly — the caller must
 * re-diff, and there is no fuzzy stale-id fallback.
 */
export function merge(baseline: string, current: string, decisions_json: string, _default: string, opts?: LexOptions): string;

export function mergeSplices(baseline: string, current: string, decisions_json: string, _default: string, opts?: LexOptions): Splices;

/**
 * THE read call. One document in, one buffer out.
 *
 * ```ts
 * interface ParseTextOptions {
 *   diagnostics?: boolean;          // run the lint walk; default false
 *   toc?: boolean;                  // build the chapter and verse index; default false
 *   utf16?: boolean;                // every offset as a UTF-16 code unit; default bytes
 *   customMarkers?: CustomMarkers;  // the `\z` list to lex by; default the installed one
 * }
 * parse(text: string, opts?: ParseTextOptions): Uint8Array;
 * ```
 *
 * The buffer is a plated parse — tokens, the tree, and whatever `opts` asked
 * for besides — read by `reader.ts`, which is generated from the same schema
 * as the writer. Nothing is retained wasm-side: the `Uint8Array` is JS's, the
 * collector reclaims it, and there is no `free`.
 *
 * A misspelled key is a compile error through the declared `ParseOptions`,
 * and a THROW at the wall for a caller the compiler never saw: an unknown key
 * is refused by name rather than read as `false` ([`options`]).
 *
 * `text` must be LF-normalized (see the module doc); a debug build asserts it.
 */
export function parse(text: string, opts?: ParseTextOptions): Uint8Array;

/**
 * Where an address is in one text, as JSON: found, missing or ambiguous,
 * never a silent nearest. The rules are `onion/src/resolve.md`, the cases
 * `testData/resolve/cases.json`, whose `expect` is this door's answer.
 *
 * ```js
 * resolve(text, { kind: "verses", from: { chapter: 1, verse: 2 }, to: { chapter: 1, verse: 2 } })
 *   // → '{"found":{"from":8,"to":19,"opening":5,"covers":{…},"coarser":false}}'
 * resolve(text, { kind: "chapters", from: 3, to: 3 }, { utf16: true })
 *   // → '{"missing":"chapter"}'
 * ```
 *
 * Offsets are bytes unless `utf16` asks otherwise. galley's
 * `Galley.resolveOf` answers the same over text, and `Galley.resolve` over a
 * registered book.
 */
export function resolve(text: string, address: Address, opts?: ResolveTextOptions): string;

/**
 * Installs a list process-wide: the one every door lexes by when it is given
 * no `customMarkers`.
 *
 * ```js
 * setCustomMarkers(customMarkersFromMarkersExt(text));
 * setCustomMarkers(customMarkers([]));                       // clears
 * ```
 *
 * Every registered marker then behaves as its `\category` — a `footnote`
 * takes a caller and a note scope, a `milestone` pairs `-s`/`-e` and takes
 * attributes — because the engine resolves it to the spec row that category
 * behaves as. An unregistered `\z` marker stays what it has always been.
 *
 * A resident `Galley` re-derives the books registered without a list; a book
 * registered with one keeps its own.
 *
 * Throws where a door refuses its `customMarkers` option.
 */
export function setCustomMarkers(markers: CustomMarkers): void;

/**
 * A CodeMirror offset as a source byte offset. Rebuilds the 1.6% stride index
 * per call (~0.1ms) — honest and stateless at a handful of calls per user
 * interaction, which is what a cursor-to-sid lookup is.
 */
export function toByte(text: string, utf16: number): number;

/**
 * A source byte offset as a CodeMirror offset. Same deal.
 */
export function toUtf16(text: string, byte: number): number;

/**
 * XXH3-64, seed 0, over the bytes — the hash every dish header stamps as
 * `sourceHash`, for whatever a host wants to key: a file fetched over the
 * network, a chapter slice cut at a TOC row.
 *
 * ```js
 * xxh3(bytes)                          // → 0x…n, a bigint (u64)
 * xxh3Text(text) === xxh3(new TextEncoder().encode(text))
 * xxh3Text(text) === parse(text, …).sourceHash
 * ```
 */
export function xxh3(bytes: Uint8Array): bigint;

/**
 * [`xxh3`] over a string's UTF-8, without a `TextEncoder` round trip on the
 * JS side.
 */
export function xxh3Text(text: string): bigint;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_edits_free: (a: number, b: number) => void;
    readonly __wbg_formatopts_free: (a: number, b: number) => void;
    readonly __wbg_get_formatopts_block_marker_own_line: (a: number) => number;
    readonly __wbg_get_formatopts_bridge_empty_verses: (a: number) => number;
    readonly __wbg_get_formatopts_char_marker_breaks: (a: number) => number;
    readonly __wbg_get_formatopts_collapse_blank_lines: (a: number) => number;
    readonly __wbg_get_formatopts_dedupe_verse_number: (a: number) => number;
    readonly __wbg_get_formatopts_delimiter_single: (a: number) => number;
    readonly __wbg_get_formatopts_designator_ws_single: (a: number) => number;
    readonly __wbg_get_formatopts_marker_ws_at_line_start: (a: number) => number;
    readonly __wbg_get_formatopts_newline: (a: number) => number;
    readonly __wbg_get_formatopts_normalize_newlines: (a: number) => number;
    readonly __wbg_get_formatopts_trim_text_edges: (a: number) => number;
    readonly __wbg_get_formatopts_verse_breaks: (a: number) => number;
    readonly __wbg_set_formatopts_block_marker_own_line: (a: number, b: number) => void;
    readonly __wbg_set_formatopts_bridge_empty_verses: (a: number, b: number) => void;
    readonly __wbg_set_formatopts_char_marker_breaks: (a: number, b: number) => void;
    readonly __wbg_set_formatopts_collapse_blank_lines: (a: number, b: number) => void;
    readonly __wbg_set_formatopts_dedupe_verse_number: (a: number, b: number) => void;
    readonly __wbg_set_formatopts_delimiter_single: (a: number, b: number) => void;
    readonly __wbg_set_formatopts_designator_ws_single: (a: number, b: number) => void;
    readonly __wbg_set_formatopts_marker_ws_at_line_start: (a: number, b: number) => void;
    readonly __wbg_set_formatopts_newline: (a: number, b: number) => void;
    readonly __wbg_set_formatopts_normalize_newlines: (a: number, b: number) => void;
    readonly __wbg_set_formatopts_trim_text_edges: (a: number, b: number) => void;
    readonly __wbg_set_formatopts_verse_breaks: (a: number, b: number) => void;
    readonly __wbg_splices_free: (a: number, b: number) => void;
    readonly attrResolve: (a: number, b: number, c: number) => number;
    readonly attrs: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly book: (a: number, b: number, c: number) => [number, number, number, number];
    readonly customMarkers: (a: any, b: number) => [number, number, number];
    readonly customMarkersFromMarkersExt: (a: number, b: number) => any;
    readonly diff: (a: number, b: number, c: number, d: number, e: any) => [number, number, number, number];
    readonly edits_lens: (a: number) => [number, number];
    readonly edits_spans: (a: number) => [number, number];
    readonly edits_text: (a: number) => [number, number];
    readonly format: (a: number, b: number, c: number) => [number, number];
    readonly formatEdits: (a: number, b: number, c: number) => number;
    readonly formatEditsIn: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly formatopts_new: () => number;
    readonly formatopts_setCustomMarkers: (a: number, b: any) => [number, number];
    readonly formatopts_setRemoveMarkers: (a: number, b: number, c: number) => void;
    readonly formatopts_setRepairs: (a: number, b: number, c: number) => void;
    readonly locate: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly mask: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly merge: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => [number, number, number, number];
    readonly mergeSplices: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => [number, number, number];
    readonly parse: (a: number, b: number, c: number) => [number, number, number, number];
    readonly resolve: (a: number, b: number, c: any, d: number) => [number, number, number, number];
    readonly setCustomMarkers: (a: any) => [number, number];
    readonly splices_inserts: (a: number) => [number, number];
    readonly splices_spans: (a: number) => [number, number];
    readonly toByte: (a: number, b: number, c: number) => number;
    readonly toUtf16: (a: number, b: number, c: number) => number;
    readonly xxh3: (a: number, b: number) => bigint;
    readonly xxh3Text: (a: number, b: number) => bigint;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
