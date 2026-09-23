/* @ts-self-types="./onion_wasm.d.ts" */
import * as wasm from "./onion_wasm_bg.wasm";
import { __wbg_set_wasm } from "./onion_wasm_bg.js";

__wbg_set_wasm(wasm);
wasm.__wbindgen_start();
export {
    Edits, FormatOpts, Splices, attrResolve, attrs, book, diff, extensionsFromMarkersExt, format, formatEdits, formatEditsIn, locate, mask, merge, mergeSplices, parse, setExtensions, toByte, toUtf16, xxh3, xxh3Text
} from "./onion_wasm_bg.js";
