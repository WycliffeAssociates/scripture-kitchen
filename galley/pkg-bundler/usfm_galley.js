/* @ts-self-types="./usfm_galley.d.ts" */
import * as wasm from "./usfm_galley_bg.wasm";
import { __wbg_set_wasm } from "./usfm_galley_bg.js";

__wbg_set_wasm(wasm);
wasm.__wbindgen_start();
export {
    Edits, Fingerprint, FormatOpts, Galley, SousSettings, Splices, attrResolve, attrs, book, diff, extensionsFromMarkersExt, format, formatEdits, formatEditsIn, locate, mask, merge, mergeSplices, parse, setExtensions, toByte, toUtf16
} from "./usfm_galley_bg.js";
