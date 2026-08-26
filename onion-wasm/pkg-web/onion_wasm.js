/* @ts-self-types="./onion_wasm.d.ts" */

/**
 * One transaction of proposed splices: `[from, to]` pairs in UTF-16, one
 * concatenated ASCII insert blob, one byte length per edit.
 *
 * The same shape a fix crosses in — an editor session applies both the same
 * way, and `lens[i] == 0` is a pure deletion.
 */
export class Edits {
    static __wrap(ptr) {
        const obj = Object.create(Edits.prototype);
        obj.__wbg_ptr = ptr;
        EditsFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        EditsFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_edits_free(ptr, 0);
    }
    /**
     * @returns {Uint32Array}
     */
    get lens() {
        const ret = wasm.edits_lens(this.__wbg_ptr);
        var v1 = getArrayU32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * @returns {Uint32Array}
     */
    get spans() {
        const ret = wasm.edits_spans(this.__wbg_ptr);
        var v1 = getArrayU32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * @returns {string}
     */
    get text() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.edits_text(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) Edits.prototype[Symbol.dispose] = Edits.prototype.free;

/**
 * The formatter's switches, defaulted to [`FormatOptions::default`].
 *
 * A tagged struct rather than a dozen positional booleans: the `.d.ts` names
 * each switch, and adding one later does not renumber a call site.
 */
export class FormatOpts {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        FormatOptsFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_formatopts_free(ptr, 0);
    }
    constructor() {
        const ret = wasm.formatopts_new();
        this.__wbg_ptr = ret;
        FormatOptsFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Marker names, no backslash, comma-separated — `"s5"` for the
     * unfoldingWord chunk marker. Every occurrence is deleted outright.
     * @param {string} names
     */
    setRemoveMarkers(names) {
        const ptr0 = passStringToWasm0(names, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.formatopts_setRemoveMarkers(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * Lint codes (the `diagnostics.json` indices) whose existing fixes join
     * the format transaction. An index naming no code is ignored.
     * @param {Uint32Array} codes
     */
    setRepairs(codes) {
        const ptr0 = passArray32ToWasm0(codes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.formatopts_setRepairs(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @returns {boolean}
     */
    get block_marker_own_line() {
        const ret = wasm.__wbg_get_formatopts_block_marker_own_line(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get bridge_empty_verses() {
        const ret = wasm.__wbg_get_formatopts_bridge_empty_verses(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * 0 = keep a break on a character-marker boundary, 1 = join it into a
     * space (the aligned-corpus shape).
     * @returns {number}
     */
    get char_marker_breaks() {
        const ret = wasm.__wbg_get_formatopts_char_marker_breaks(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {boolean}
     */
    get collapse_blank_lines() {
        const ret = wasm.__wbg_get_formatopts_collapse_blank_lines(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get dedupe_verse_number() {
        const ret = wasm.__wbg_get_formatopts_dedupe_verse_number(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get delimiter_single() {
        const ret = wasm.__wbg_get_formatopts_delimiter_single(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get designator_ws_single() {
        const ret = wasm.__wbg_get_formatopts_designator_ws_single(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get marker_ws_at_line_start() {
        const ret = wasm.__wbg_get_formatopts_marker_ws_at_line_start(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * 0 = LF, 1 = CRLF.
     * @returns {number}
     */
    get newline() {
        const ret = wasm.__wbg_get_formatopts_newline(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {boolean}
     */
    get normalize_newlines() {
        const ret = wasm.__wbg_get_formatopts_normalize_newlines(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get trim_text_edges() {
        const ret = wasm.__wbg_get_formatopts_trim_text_edges(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * 0 = keep the line break in front of a `\v`, 1 = fold it into a space.
     * @returns {number}
     */
    get verse_breaks() {
        const ret = wasm.__wbg_get_formatopts_verse_breaks(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @param {boolean} arg0
     */
    set block_marker_own_line(arg0) {
        wasm.__wbg_set_formatopts_block_marker_own_line(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set bridge_empty_verses(arg0) {
        wasm.__wbg_set_formatopts_bridge_empty_verses(this.__wbg_ptr, arg0);
    }
    /**
     * 0 = keep a break on a character-marker boundary, 1 = join it into a
     * space (the aligned-corpus shape).
     * @param {number} arg0
     */
    set char_marker_breaks(arg0) {
        wasm.__wbg_set_formatopts_char_marker_breaks(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set collapse_blank_lines(arg0) {
        wasm.__wbg_set_formatopts_collapse_blank_lines(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set dedupe_verse_number(arg0) {
        wasm.__wbg_set_formatopts_dedupe_verse_number(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set delimiter_single(arg0) {
        wasm.__wbg_set_formatopts_delimiter_single(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set designator_ws_single(arg0) {
        wasm.__wbg_set_formatopts_designator_ws_single(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set marker_ws_at_line_start(arg0) {
        wasm.__wbg_set_formatopts_marker_ws_at_line_start(this.__wbg_ptr, arg0);
    }
    /**
     * 0 = LF, 1 = CRLF.
     * @param {number} arg0
     */
    set newline(arg0) {
        wasm.__wbg_set_formatopts_newline(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set normalize_newlines(arg0) {
        wasm.__wbg_set_formatopts_normalize_newlines(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set trim_text_edges(arg0) {
        wasm.__wbg_set_formatopts_trim_text_edges(this.__wbg_ptr, arg0);
    }
    /**
     * 0 = keep the line break in front of a `\v`, 1 = fold it into a space.
     * @param {number} arg0
     */
    set verse_breaks(arg0) {
        wasm.__wbg_set_formatopts_verse_breaks(this.__wbg_ptr, arg0);
    }
}
if (Symbol.dispose) FormatOpts.prototype[Symbol.dispose] = FormatOpts.prototype.free;

/**
 * The same merge as replay splices over the BASELINE — the hot-path shape, for
 * an editor that would rather apply a transaction than replace a document.
 *
 * `spans` is `[from, to]` per splice in BASELINE UTF-16; `inserts` is
 * `[from, to]` per splice in CURRENT UTF-16, the text to put there. An empty
 * insert is a deletion; `from == to` in `spans` is an insertion.
 */
export class Splices {
    static __wrap(ptr) {
        const obj = Object.create(Splices.prototype);
        obj.__wbg_ptr = ptr;
        SplicesFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        SplicesFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_splices_free(ptr, 0);
    }
    /**
     * @returns {Uint32Array}
     */
    get inserts() {
        const ret = wasm.splices_inserts(this.__wbg_ptr);
        var v1 = getArrayU32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * @returns {Uint32Array}
     */
    get spans() {
        const ret = wasm.splices_spans(this.__wbg_ptr);
        var v1 = getArrayU32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
}
if (Symbol.dispose) Splices.prototype[Symbol.dispose] = Splices.prototype.free;

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
 * @param {string} text
 * @param {number} wants
 * @param {number | null} [clip_from]
 * @param {number | null} [clip_to]
 * @returns {object}
 */
export function analyze(text, wants, clip_from, clip_to) {
    const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.analyze(ptr0, len0, wants, isLikeNone(clip_from) ? Number.MAX_SAFE_INTEGER : (clip_from) >>> 0, isLikeNone(clip_to) ? Number.MAX_SAFE_INTEGER : (clip_to) >>> 0);
    return ret;
}

/**
 * The first `\id`'s book code — `"GEN"`. EMPTY when the document declares
 * none (real in the wild: BSB Ecclesiastes); the `missing-id` diagnostic is
 * where that becomes a finding, not here.
 * @param {string} text
 * @returns {string}
 */
export function book(text) {
    let deferred2_0;
    let deferred2_1;
    try {
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.book(ptr0, len0);
        deferred2_0 = ret[0];
        deferred2_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
    }
}

/**
 * The diff skeleton as JSON. Spans are UTF-16 offsets into each side's own
 * document.
 * @param {string} baseline
 * @param {string} current
 * @returns {string}
 */
export function diff(baseline, current) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(baseline, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(current, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.diff(ptr0, len0, ptr1, len1);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * The formatted document. `format_edits` applied, in one call.
 * @param {string} text
 * @param {FormatOpts} opts
 * @returns {string}
 */
export function format(text, opts) {
    let deferred2_0;
    let deferred2_1;
    try {
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        _assertClass(opts, FormatOpts);
        const ret = wasm.format(ptr0, len0, opts.__wbg_ptr);
        deferred2_0 = ret[0];
        deferred2_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
    }
}

/**
 * The formatter's edit list, offsets converted to UTF-16 here.
 *
 * The index is built per call: an edit list is small and the conversion is
 * random-access (edits are sorted, but a stride index is ~0.1ms and this is a
 * user-triggered path, not a keystroke one).
 * @param {string} text
 * @param {FormatOpts} opts
 * @returns {Edits}
 */
export function formatEdits(text, opts) {
    const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    _assertClass(opts, FormatOpts);
    const ret = wasm.formatEdits(ptr0, len0, opts.__wbg_ptr);
    return Edits.__wrap(ret);
}

/**
 * The same transaction bounded to `from..to` (UTF-16, the offsets the editor
 * already holds — a chapter's span out of the `chapters` read).
 *
 * The range crosses the wall in UTF-16 and is translated here, on the same
 * index the edits go out through. The policy is the library's: an edit is kept
 * only if its whole span is inside, a multi-edit claim only if all of it is,
 * and a pure insertion sitting ON either edge is inside.
 * @param {string} text
 * @param {number} from
 * @param {number} to
 * @param {FormatOpts} opts
 * @returns {Edits}
 */
export function formatEditsIn(text, from, to, opts) {
    const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    _assertClass(opts, FormatOpts);
    const ret = wasm.formatEditsIn(ptr0, len0, from, to, opts.__wbg_ptr);
    return Edits.__wrap(ret);
}

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
 * @param {string} text
 * @param {number} utf16
 * @returns {string}
 */
export function locate(text, utf16) {
    let deferred2_0;
    let deferred2_1;
    try {
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.locate(ptr0, len0, utf16);
        deferred2_0 = ret[0];
        deferred2_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
    }
}

/**
 * The merged document. An unknown unit id REJECTS loudly — the caller must
 * re-diff, and there is no fuzzy stale-id fallback.
 * @param {string} baseline
 * @param {string} current
 * @param {string} decisions_json
 * @param {string} _default
 * @returns {string}
 */
export function merge(baseline, current, decisions_json, _default) {
    let deferred6_0;
    let deferred6_1;
    try {
        const ptr0 = passStringToWasm0(baseline, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(current, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(decisions_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(_default, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len3 = WASM_VECTOR_LEN;
        const ret = wasm.merge(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3);
        var ptr5 = ret[0];
        var len5 = ret[1];
        if (ret[3]) {
            ptr5 = 0; len5 = 0;
            throw takeFromExternrefTable0(ret[2]);
        }
        deferred6_0 = ptr5;
        deferred6_1 = len5;
        return getStringFromWasm0(ptr5, len5);
    } finally {
        wasm.__wbindgen_free(deferred6_0, deferred6_1, 1);
    }
}

/**
 * @param {string} baseline
 * @param {string} current
 * @param {string} decisions_json
 * @param {string} _default
 * @returns {Splices}
 */
export function mergeSplices(baseline, current, decisions_json, _default) {
    const ptr0 = passStringToWasm0(baseline, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(current, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passStringToWasm0(decisions_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len2 = WASM_VECTOR_LEN;
    const ptr3 = passStringToWasm0(_default, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len3 = WASM_VECTOR_LEN;
    const ret = wasm.mergeSplices(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return Splices.__wrap(ret[0]);
}

/**
 * A CodeMirror offset as a source byte offset. Rebuilds the 1.6% stride index
 * per call (~0.1ms) — honest and stateless at a handful of calls per user
 * interaction, which is what a cursor-to-sid lookup is.
 * @param {string} text
 * @param {number} utf16
 * @returns {number}
 */
export function toByte(text, utf16) {
    const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.toByte(ptr0, len0, utf16);
    return ret >>> 0;
}

/**
 * A source byte offset as a CodeMirror offset. Same deal.
 * @param {string} text
 * @param {number} byte
 * @returns {number}
 */
export function toUtf16(text, byte) {
    const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.toUtf16(ptr0, len0, byte);
    return ret >>> 0;
}

/**
 * `wants::ALL` — every read. Exported so a caller that wants everything does
 * not restate the bitmask.
 * @returns {number}
 */
export function wantsAll() {
    const ret = wasm.wantsAll();
    return ret >>> 0;
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_408e67f47ca7b58b: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg___wbindgen_debug_string_a57024b9c6e4a48b: function(arg0, arg1) {
            const ret = debugString(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_bb96b2010945f0bc: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_new_ebe3e0f6837f0879: function() {
            const ret = new Object();
            return ret;
        },
        __wbg_new_from_slice_8aed4f0384605526: function(arg0, arg1) {
            const ret = new Uint32Array(getArrayU32FromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_set_8155bb79a948541b: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = Reflect.set(arg0, arg1, arg2);
            return ret;
        }, arguments); },
        __wbindgen_cast_0000000000000001: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_cast_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./onion_wasm_bg.js": import0,
    };
}

const EditsFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_edits_free(ptr, 1));
const FormatOptsFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_formatopts_free(ptr, 1));
const SplicesFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_splices_free(ptr, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

function _assertClass(instance, klass) {
    if (!(instance instanceof klass)) {
        throw new Error(`expected instance of ${klass.name}`);
    }
}

function debugString(val) {
    // primitive types
    const type = typeof val;
    if (type == 'number' || type == 'boolean' || val == null) {
        return  `${val}`;
    }
    if (type == 'string') {
        return `"${val}"`;
    }
    if (type == 'symbol') {
        const description = val.description;
        if (description == null) {
            return 'Symbol';
        } else {
            return `Symbol(${description})`;
        }
    }
    if (type == 'function') {
        const name = val.name;
        if (typeof name == 'string' && name.length > 0) {
            return `Function(${name})`;
        } else {
            return 'Function';
        }
    }
    // objects
    if (Array.isArray(val)) {
        const length = val.length;
        let debug = '[';
        if (length > 0) {
            debug += debugString(val[0]);
        }
        for(let i = 1; i < length; i++) {
            debug += ', ' + debugString(val[i]);
        }
        debug += ']';
        return debug;
    }
    // Test for built-in
    const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
    let className;
    if (builtInMatches && builtInMatches.length > 1) {
        className = builtInMatches[1];
    } else {
        // Failed to match the standard '[object ClassName]'
        return toString.call(val);
    }
    if (className == 'Object') {
        // we're a user defined class or Object
        // JSON.stringify avoids problems with cycles, and is generally much
        // easier than looping through ownProperties of `val`.
        try {
            return 'Object(' + JSON.stringify(val) + ')';
        } catch (_) {
            return 'Object';
        }
    }
    // errors
    if (val instanceof Error) {
        return `${val.name}: ${val.message}\n${val.stack}`;
    }
    // TODO we could test for more things here, like `Set`s and `Map`s.
    return className;
}

function getArrayU32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint32ArrayMemory0 = null;
function getUint32ArrayMemory0() {
    if (cachedUint32ArrayMemory0 === null || cachedUint32ArrayMemory0.byteLength === 0) {
        cachedUint32ArrayMemory0 = new Uint32Array(wasm.memory.buffer);
    }
    return cachedUint32ArrayMemory0;
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray32ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 4, 4) >>> 0;
    getUint32ArrayMemory0().set(arg, ptr / 4);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('onion_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
