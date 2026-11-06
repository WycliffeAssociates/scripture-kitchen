/* @ts-self-types="./usfm_galley.d.ts" */
import * as wasm from "./usfm_galley_bg.wasm";
import { __wbg_set_wasm } from "./usfm_galley_bg.js";

__wbg_set_wasm(wasm);
wasm.__wbindgen_start();
export {
    Citations, Edits, Fingerprint, FormatOpts, Galley, SousSettings, Splices, attrResolve, attrs, book, customMarkers, customMarkersFromMarkersExt, diff, format, formatEdits, formatEditsIn, formatReference, locate, mask, merge, mergeSplices, parse, parseReference, resolve, setCustomMarkers, showReference, toByte, toUtf16, xxh3, xxh3Text
} from "./usfm_galley_bg.js";
