/* @ts-self-types="./onion_wasm.d.ts" */
import * as wasm from "./onion_wasm_bg.wasm";
import { __wbg_set_wasm } from "./onion_wasm_bg.js";

__wbg_set_wasm(wasm);
wasm.__wbindgen_start();
export {
    Edits, FormatOpts, Splices, book, diff, format, formatEdits, formatEditsIn, locate, mask, merge, mergeSplices, parse, toByte, toUtf16
} from "./onion_wasm_bg.js";
