/* tslint:disable */
/* eslint-disable */

export class HistoryWalk {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * The history, once the walk wants nothing more. `source` is `owner/repo`.
     */
    finish(source: string): WriterHistory;
    /**
     * Hand over objects: their ids, a kind each (0 commit, 1 tree, 2 blob),
     * their lengths, and their bytes one after another.
     */
    give(ids: string[], kinds: Uint8Array, lengths: Uint32Array, bytes: Uint8Array): void;
    constructor(head: string, shallow: string[]);
    /**
     * The object ids to read next; empty when the walk is done.
     */
    wants(): string[];
}

/**
 * One book project's saves, held in the module.
 */
export class WriterHistory {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * JSON `{name, email, at, offset}` of the first save, after clock fixes.
     */
    first(now: number): string | undefined;
    /**
     * JSON `{name, email, at, offset}` of the last save, after clock fixes.
     */
    last(now: number): string | undefined;
    /**
     * `[at, verses, at, verses, …]`: verses present after each save.
     */
    progress(now: number): Float64Array;
    /**
     * JSON: `{book, language: {id, name?, direction}, resource, sources, translators, name?}`.
     */
    project(): string;
    title(): string;
    /**
     * The book at its last save, as replay renders it.
     */
    usfm(): string;
    readonly saves: number;
    /**
     * `owner/repo`: where `Writer-Source` says a save came from.
     */
    readonly source: string;
}

/**
 * [`replay`], then a closing commit holding the project's manifests.
 * `project` is bento's `NewBibleProject` without books; `closing` is
 * `{author, committer?, message}`. Uses the histories up.
 */
export function consolidate(histories: WriterHistory[], now: number, deltas: boolean, project: string, closing: string, formats: string[]): object;

/**
 * Every history's saves as one line of commits in one pack, or `undefined`
 * when no save changed any book. Uses the histories up.
 */
export function replay(histories: WriterHistory[], now: number, deltas: boolean): object | undefined;

/**
 * One Writer book's files, with no history, → its USFM: the template
 * replay renders each save with, over the files as they are. `paths[i]`'s
 * bytes are the next `lengths[i]` of `bytes`, as `give` packs them; files
 * that aren't the book's text, or aren't UTF-8, are left out. `book` is the
 * code `\id` gets; `title` stands in when `front/title.txt` says nothing
 * (default: canon's English name). Needs no git.
 */
export function stitch(paths: string[], lengths: Uint32Array, bytes: Uint8Array, book: string, title?: string | null): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_historywalk_free: (a: number, b: number) => void;
    readonly __wbg_writerhistory_free: (a: number, b: number) => void;
    readonly consolidate: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => [number, number, number];
    readonly historywalk_finish: (a: number, b: number, c: number) => [number, number, number];
    readonly historywalk_give: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => [number, number];
    readonly historywalk_new: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly historywalk_wants: (a: number) => [number, number];
    readonly replay: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly stitch: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => [number, number, number, number];
    readonly writerhistory_first: (a: number, b: number) => [number, number];
    readonly writerhistory_last: (a: number, b: number) => [number, number];
    readonly writerhistory_progress: (a: number, b: number) => [number, number];
    readonly writerhistory_project: (a: number) => [number, number];
    readonly writerhistory_saves: (a: number) => number;
    readonly writerhistory_source: (a: number) => [number, number];
    readonly writerhistory_title: (a: number) => [number, number];
    readonly writerhistory_usfm: (a: number) => [number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __externref_drop_slice: (a: number, b: number) => void;
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
