/* tslint:disable */
/* eslint-disable */

/**
 * One transaction of proposed splices: `[from, to]` pairs in UTF-16, one
 * concatenated ASCII insert blob, one byte length per edit.
 *
 * The same shape a fix crosses in — an editor session applies both the same
 * way, and `lens[i] == 0` is a pure deletion.
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
 * The one read call. `wants` is the bitmask in `onion-wasm.ts`; an unset bit
 * computes nothing and returns an empty array.
 *
 * `clipFrom`/`clipTo` are UTF-16 offsets and bound ONLY the token-granularity
 * reads (`tokenSpans`, `textRuns`) to a viewport — chapters, blocks and
 * diagnostics stay whole-book, because a finding's evidence is regularly
 * outside the viewport that shows it. Pass `undefined` for both to skip.
 *
 * `text` must be LF-normalized (see the module doc); a debug build asserts it.
 */
export function analyze(text: string, wants: number, clip_from?: number | null, clip_to?: number | null): object;

/**
 * The first `\id`'s book code — `"GEN"`. EMPTY when the document declares
 * none (real in the wild: BSB Ecclesiastes); the `missing-id` diagnostic is
 * where that becomes a finding, not here.
 */
export function book(text: string): string;

/**
 * The diff skeleton as JSON. Spans are UTF-16 offsets into each side's own
 * document.
 */
export function diff(baseline: string, current: string): string;

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
 * The merged document. An unknown unit id REJECTS loudly — the caller must
 * re-diff, and there is no fuzzy stale-id fallback.
 */
export function merge(baseline: string, current: string, decisions_json: string, _default: string): string;

export function mergeSplices(baseline: string, current: string, decisions_json: string, _default: string): Splices;

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
 * `wants::ALL` — every read. Exported so a caller that wants everything does
 * not restate the bitmask.
 */
export function wantsAll(): number;

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
    readonly analyze: (a: number, b: number, c: number, d: number, e: number) => any;
    readonly book: (a: number, b: number) => [number, number];
    readonly diff: (a: number, b: number, c: number, d: number) => [number, number];
    readonly edits_lens: (a: number) => [number, number];
    readonly edits_spans: (a: number) => [number, number];
    readonly edits_text: (a: number) => [number, number];
    readonly format: (a: number, b: number, c: number) => [number, number];
    readonly formatEdits: (a: number, b: number, c: number) => number;
    readonly formatopts_new: () => number;
    readonly formatopts_setRemoveMarkers: (a: number, b: number, c: number) => void;
    readonly formatopts_setRepairs: (a: number, b: number, c: number) => void;
    readonly locate: (a: number, b: number, c: number) => [number, number];
    readonly merge: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number, number];
    readonly mergeSplices: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number];
    readonly splices_inserts: (a: number) => [number, number];
    readonly splices_spans: (a: number) => [number, number];
    readonly toByte: (a: number, b: number, c: number) => number;
    readonly toUtf16: (a: number, b: number, c: number) => number;
    readonly wantsAll: () => number;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
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
