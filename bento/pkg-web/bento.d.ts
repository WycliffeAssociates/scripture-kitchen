/* tslint:disable */
/* eslint-disable */

export function check(name: string, bytes: Uint8Array, repo_name?: string | null, files?: Map<any, any> | null): string;

export function createBible(project: string, book_bytes: Array<any>, formats: string[]): Map<any, any>;

export function edit(existing: Map<any, any>, change: string, add_bytes: Array<any>, checksum_bytes: Array<any>): object;

export function find(names: string[]): string | undefined;

export function fingerprint(bytes: Uint8Array): string;

export function read(name: string, bytes: Uint8Array): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly check: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number];
    readonly createBible: (a: number, b: number, c: any, d: number, e: number) => [number, number, number];
    readonly edit: (a: any, b: number, c: number, d: any, e: any) => [number, number, number];
    readonly find: (a: number, b: number) => [number, number];
    readonly fingerprint: (a: number, b: number) => [number, number];
    readonly read: (a: number, b: number, c: number, d: number) => [number, number];
    readonly wasm_bindgen_a705d1ccab00a4e4___convert__closures_____invoke___js_sys_b4878f215895654f___Function_fn_wasm_bindgen_a705d1ccab00a4e4___JsValue_____wasm_bindgen_a705d1ccab00a4e4___sys__Undefined___js_sys_b4878f215895654f___Function_fn_wasm_bindgen_a705d1ccab00a4e4___JsValue_____wasm_bindgen_a705d1ccab00a4e4___sys__Undefined_______true_: (a: number, b: number, c: any, d: any) => void;
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
