/* @ts-self-types="./onion_wasm.d.ts" */
import * as wasm from "./onion_wasm_bg.wasm";
import { __wbg_set_wasm } from "./onion_wasm_bg.js";

__wbg_set_wasm(wasm);
wasm.__wbindgen_start();
export {
    Edits, FormatOpts, Splices, analyze, book, diff, format, formatEdits, locate, merge, mergeSplices, toByte, toUtf16, wantsAll
} from "./onion_wasm_bg.js";
