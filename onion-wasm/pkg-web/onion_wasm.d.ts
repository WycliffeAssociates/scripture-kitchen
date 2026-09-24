/* tslint:disable */
/* eslint-disable */

/** `parse`'s options, and `Galley.parse`/`parseText`'s. Every key defaults to false. */
export interface ParseOptions {
    /** Run the lint walk. The one expensive optional. */
    diagnostics?: boolean;
    /** Build the chapter and verse index. Cheap, and needs no tree. */
    toc?: boolean;
    /** Emit every offset as a UTF-16 code unit instead of a byte. */
    utf16?: boolean;
}

/** `attrs`'s options. */
export interface AttrsOptions {
    /** `from`/`to` and every word returned are UTF-16 code units; default bytes. */
    utf16?: boolean;
}

/** `setExtensions`'s options. */
export interface ExtensionOptions {
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
export function book(text: string): string;

/**
 * The diff skeleton as JSON. Spans are UTF-16 offsets into each side's own
 * document.
 *
 * `text_mode` is `"none"` | `"words"` | `"chars"`: the intra-verse runs a
 * `modified` unit is highlighted by, at UAX-29 word or grapheme grain.
 * `"none"` computes nothing — no CST, no mask — and yields the same JSON the
 * door returned before runs existed.
 */
export function diff(baseline: string, current: string, text_mode: string): string;

/**
 * A `markers.ext` file, read into the list [`set_extensions`] takes.
 *
 * ```js
 * const { markers, malformed } = JSON.parse(extensionsFromMarkersExt(text));
 * for (const { name, reason, line } of malformed) show(line, name, reason);
 * setExtensions(JSON.stringify(markers));
 * ```
 *
 * READS ONLY — nothing is installed here, because a host may want to show
 * what it found before acting on it, and because the file is one of several
 * ways a list arrives (a `custom.sty`, a UI that lets a user add a marker).
 * A bad entry costs only itself and lands in `malformed`; the file never
 * fails as a whole.
 */
export function extensionsFromMarkersExt(text: string): string;

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
export function locate(text: string, utf16: number): string;

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
export function mask(text: string, recipe: string): object;

/**
 * The merged document. An unknown unit id REJECTS loudly — the caller must
 * re-diff, and there is no fuzzy stale-id fallback.
 */
export function merge(baseline: string, current: string, decisions_json: string, _default: string): string;

export function mergeSplices(baseline: string, current: string, decisions_json: string, _default: string): Splices;

/**
 * THE read call. One document in, one buffer out.
 *
 * ```ts
 * interface ParseOptions {
 *   diagnostics?: boolean;   // run the lint walk; default false
 *   toc?: boolean;           // build the chapter and verse index; default false
 *   utf16?: boolean;         // every offset as a UTF-16 code unit; default bytes
 * }
 * parse(text: string, opts?: ParseOptions): Uint8Array;
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
export function parse(text: string, opts?: ParseOptions): Uint8Array;

/**
 * Installs a list of user markers process-wide, and returns what it could not
 * keep.
 *
 * ```js
 * setExtensions('[{"name":"zaln","category":"milestone"}]');  // → "[]"
 * setExtensions("[]");                                        // clears
 *
 * // Legacy markup a host cannot change: en_ulb's chunk marker, as a bare
 * // point that leaves its paragraph open.
 * setExtensions('[{"name":"s5","category":"standalone"}]', { relaxZPrefix: true });
 * ```
 *
 * Takes the LIST, never a file: a host with a `custom.sty`, or a UI that lets
 * a user add a marker, feeds this directly.
 *
 * Every registered marker then behaves as its `\category` — a `footnote`
 * takes a caller and a note scope, a `milestone` pairs `-s`/`-e` and takes
 * attributes — because the engine resolves it to the spec row that category
 * behaves as. An unregistered `\z` marker stays what it has always been.
 *
 * **This invalidates every derived product**, here and in a resident
 * `Galley`: the same bytes are a different document once the rows change, and
 * both caches key on content. Call it at composition, before the first
 * parse, rather than between edits.
 *
 * `relaxZPrefix` admits a name without the `z` that the spec does not
 * define; a name the spec does define (`s1`, `p`) is still a report.
 *
 * Throws on malformed JSON and on an `opts` that is not an object or whose
 * `relaxZPrefix` is not a boolean. A bad ENTRY — no name, a name that is not
 * `z`-initial, an unknown category word, a duplicate — is a report, not a
 * failure, so one bad line never costs a host the rest of its list.
 */
export function setExtensions(list: string, opts?: ExtensionOptions): string;

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
    readonly book: (a: number, b: number) => [number, number];
    readonly diff: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly edits_lens: (a: number) => [number, number];
    readonly edits_spans: (a: number) => [number, number];
    readonly edits_text: (a: number) => [number, number];
    readonly extensionsFromMarkersExt: (a: number, b: number) => [number, number];
    readonly format: (a: number, b: number, c: number) => [number, number];
    readonly formatEdits: (a: number, b: number, c: number) => number;
    readonly formatEditsIn: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly formatopts_new: () => number;
    readonly formatopts_setRemoveMarkers: (a: number, b: number, c: number) => void;
    readonly formatopts_setRepairs: (a: number, b: number, c: number) => void;
    readonly locate: (a: number, b: number, c: number) => [number, number];
    readonly mask: (a: number, b: number, c: number, d: number) => any;
    readonly merge: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number, number];
    readonly mergeSplices: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number];
    readonly parse: (a: number, b: number, c: number) => [number, number, number, number];
    readonly setExtensions: (a: number, b: number, c: number) => [number, number, number, number];
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
