/* @ts-self-types="./usfm_galley.d.ts" */

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
 * One book's chunk starts and their checksums — no text, ~1 KB, opaque.
 *
 * Dirty and rework are two questions, and this answers both separately:
 * [`differs_from`](Self::differs_from) is positional, so a chapter that only
 * moved is dirty; [`changed_chunks`](Self::changed_chunks) is set membership
 * over the checksums, so that same chapter needs no rework.
 *
 * A handle, so JS frees it: `print.free()` when the baseline is dropped.
 */
export class Fingerprint {
    static __wrap(ptr) {
        const obj = Object.create(Fingerprint.prototype);
        obj.__wbg_ptr = ptr;
        FingerprintFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        FingerprintFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_fingerprint_free(ptr, 0);
    }
    /**
     * Ranges of `current`'s text whose chunk this fingerprint never saw, as
     * flat `from, to` pairs.
     * @param {Fingerprint} current
     * @returns {Uint32Array}
     */
    changedChunks(current) {
        _assertClass(current, Fingerprint);
        const ret = wasm.fingerprint_changedChunks(this.__wbg_ptr, current.__wbg_ptr);
        var v1 = getArrayU32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Chunks in the text this fingerprint was taken from.
     * @returns {number}
     */
    chunkCount() {
        const ret = wasm.fingerprint_chunkCount(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Whether the two byte strings differ at all.
     * @param {Fingerprint} current
     * @returns {boolean}
     */
    differsFrom(current) {
        _assertClass(current, Fingerprint);
        const ret = wasm.fingerprint_differsFrom(this.__wbg_ptr, current.__wbg_ptr);
        return ret !== 0;
    }
}
if (Symbol.dispose) Fingerprint.prototype[Symbol.dispose] = Fingerprint.prototype.free;

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
 * The corpus, resident: one [`Expediter`], one Pantry, one snapshot out.
 *
 * One per project, not per document. Books go in whole by caller id and come
 * back as one complete publication; the Pantry inside owns the chunk cache,
 * so the onion methods read the same warm chunks the analysis does.
 */
export class Galley {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        GalleyFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_galley_free(ptr, 0);
    }
    /**
     * Ranges of `text` whose chunk this book's last update never saw — the
     * chunks a host would have to re-derive, in `text`'s own byte offsets,
     * as `from, to` pairs.
     *
     * `undefined` when the id is not registered, which is the question a
     * host asks before deciding to register it.
     * @param {string} id
     * @param {string} text
     * @returns {Uint32Array | undefined}
     */
    changedSinceUpdate(id, text) {
        const ptr0 = passStringToWasm0(id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.galley_changedSinceUpdate(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        let v3;
        if (ret[0] !== 0) {
            v3 = getArrayU32FromWasm0(ret[0], ret[1]).slice();
            wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        }
        return v3;
    }
    /**
     * A copy of the settings the next [`publish`](Self::publish) judges with.
     * @returns {SousSettings}
     */
    config() {
        const ret = wasm.galley_config(this.__wbg_ptr);
        return SousSettings.__wrap(ret);
    }
    /**
     * Cached chunk units. Zero for a one-chapter book: galley does not cache
     * what it cannot reuse.
     * @returns {number}
     */
    entryCount() {
        const ret = wasm.galley_entryCount(this.__wbg_ptr);
        return ret;
    }
    /**
     * Every hit of `needle` in ONE registered book's verse-text projection,
     * as the find buffer ([`crate::find::wire`] and `wasm.md` state the
     * layout: magic and version, then little-endian `u32`, UTF-16 offsets,
     * both coordinate spaces per hit).
     *
     * ```ts
     * interface FindOptions {
     *   caseSensitive?: boolean;                        // default false
     *   wholeWord?: boolean;                            // default false
     *   limit?: number;                                 // default 0: no bound
     *   scope?: "targets" | "references" | "all";       // findAll only; default "targets"
     * }
     * find(id: string, needle: string, opts?: FindOptions): Uint8Array;
     * findAll(needle: string, opts?: FindOptions): Uint8Array;
     * ```
     *
     * The search runs over the PROJECTION — what a reader sees — so a needle
     * inside a footnote is not found, and a needle that spans one comes back
     * as one source range per contiguous piece. That is the whole reason the
     * buffer carries a piece count per hit.
     *
     * Literal only: `needle` is never a pattern. `wholeWord` is the words
     * rule galley restates in `find.md`; `caseSensitive` off is the simple
     * lowercase fold, not a collator. `limit` bounds hits across the whole
     * call, and `0` (the default) means no bound. `scope` names ONE book, so
     * it belongs to `findAll` only — present here it throws. Any registered
     * book that retains text and a projection may be searched — a target, or
     * a reference registered with `keepText`. One that retains neither errors
     * by name, because answering "no hits" would say it was clean.
     * @param {string} id
     * @param {string} needle
     * @param {any} opts
     * @returns {Uint8Array}
     */
    find(id, needle, opts) {
        const ptr0 = passStringToWasm0(id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(needle, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.galley_find(this.__wbg_ptr, ptr0, len0, ptr1, len1, opts);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v3;
    }
    /**
     * The same over every searchable book in `opts.scope`, in canonical book
     * order — the project-wide find. See [`find`](Self::find) for
     * `FindOptions`.
     *
     * `scope` is `"targets"` (the default when omitted), `"references"`, or
     * `"all"`, which searches the targets and then the references. A
     * reference registered without `keepText` is in no scope: it retains
     * nothing to search, so it is not listed either.
     *
     * The buffer's `bookIndex` indexes its own id table, which names every
     * book searched whether or not it matched, so a consumer never has to ask
     * a second question to learn which book a hit is in.
     * @param {string} needle
     * @param {any} opts
     * @returns {Uint8Array}
     */
    findAll(needle, opts) {
        const ptr0 = passStringToWasm0(needle, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.galley_findAll(this.__wbg_ptr, ptr0, len0, opts);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * Chunk starts plus one checksum each, and no text — the ~1 KB baseline
     * user land keeps beside a file on disk.
     * @param {string} text
     * @returns {Fingerprint}
     */
    fingerprint(text) {
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.galley_fingerprint(this.__wbg_ptr, ptr0, len0);
        return Fingerprint.__wrap(ret);
    }
    /**
     * Books rescanned for sites rather than replaying cached rows.
     * @returns {number}
     */
    lastLocated() {
        const ret = wasm.galley_lastLocated(this.__wbg_ptr);
        return ret;
    }
    /**
     * Chapters mapped by the last [`publish`](Self::publish).
     * @returns {number}
     */
    lastMapped() {
        const ret = wasm.galley_lastMapped(this.__wbg_ptr);
        return ret;
    }
    /**
     * Targets re-paired against their declared source.
     * @returns {number}
     */
    lastPaired() {
        const ret = wasm.galley_lastPaired(this.__wbg_ptr);
        return ret;
    }
    /**
     * Of those, the ones that kept an observation and re-walked only part.
     * @returns {number}
     */
    lastRemapped() {
        const ret = wasm.galley_lastRemapped(this.__wbg_ptr);
        return ret;
    }
    /**
     * Declared sources the source-copy lane would have read and could not,
     * because they were registered while `settings.source_copy` was off and so
     * kept no word lane.
     *
     * Nonzero after turning the lane on means "re-send those references'
     * text", not "nothing was found".
     * @returns {number}
     */
    lastWordlessReferences() {
        const ret = wasm.galley_lastWordlessReferences(this.__wbg_ptr);
        return ret;
    }
    /**
     * One registered book's diagnostics, off its retained text.
     *
     * Onion has no lint door of its own: a lint report crosses the wall as
     * the `diagnostics` section of a parse buffer, so this is
     * [`parse`](Self::parse) with that section alone asked for, read by the
     * same `reader.ts`.
     * @param {string} id
     * @returns {Uint8Array}
     */
    lint(id) {
        const ptr0 = passStringToWasm0(id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.galley_lint(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * Chunk units computed rather than reused, cumulative.
     * @returns {number}
     */
    misses() {
        const ret = wasm.galley_misses(this.__wbg_ptr);
        return ret;
    }
    /**
     * `budgetBytes` bounds resident products; omit it for 16 MB.
     * @param {number | null} [budget_bytes]
     */
    constructor(budget_bytes) {
        const ret = wasm.galley_new(!isLikeNone(budget_bytes), isLikeNone(budget_bytes) ? 0 : budget_bytes);
        this.__wbg_ptr = ret;
        GalleyFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * The edits that make `targetId`'s skeleton `sourceId`'s, exactly.
     *
     * ```ts
     * interface OverlayOptions {
     *   markers?: string[];                              // default: onion's paragraph+poetry block set, no titles
     *   scope?: { chapter: number } | { sid: string };   // default: the whole book
     *   utf16?: boolean;                                 // default false: byte offsets; true: UTF-16 units, like parse/find
     * }
     * ```
     *
     * A source block the target lacks is INSERTED — before the verse's `\v`
     * when it is leading, EMPTY after the verse's text when it is inside,
     * because where a verse's text splits is unknowable across languages and
     * the translator pastes each line into place. A target block the source
     * lacks is REMOVED and its text joins the block before it. Footnotes and
     * cross-references never cross; their locations are the target's own.
     *
     * The transaction is ascending and non-overlapping, so a host applies it
     * through the document as ONE undo step — it is `onion-wasm`'s own
     * `Edits`, the class `formatEdits` answers with, so an editor applies an
     * overlay exactly as it applies a fix. Its spans are BYTES here unless
     * `utf16` asks otherwise; `formatEdits`'s are always UTF-16.
     * @param {string} target_id
     * @param {string} source_id
     * @param {any} opts
     * @returns {Edits}
     */
    overlay(target_id, source_id, opts) {
        const ptr0 = passStringToWasm0(target_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(source_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.galley_overlay(this.__wbg_ptr, ptr0, len0, ptr1, len1, opts);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Edits.__wrap(ret[0]);
    }
    /**
     * What the overlay did, and what it declined to do, as JSON.
     *
     * ```ts
     * interface BlockAddress { sid: string; where: "leading" | "inside"; ordinal: number;
     *                           marker: string }   // the spelling that position held
     * interface OverlayReport {
     *   inserted:  { address: BlockAddress; marker: string; at: number; empty: boolean }[];  // empty = Inside block awaiting text
     *   removed:   { address: BlockAddress; marker: string; from: number; to: number }[];    // target blocks the source lacks
     *   collapsed: { sid: string; marker: string; count: number }[];                         // source empty-block runs folded to one
     *   unpaired:  { sid: string; side: "target" | "source"; reason: "absent" | "bridge" | "ambiguous" }[];
     * }
     * ```
     *
     * An overlay is a SUGGESTION applied on request, never a finding.
     * @param {string} target_id
     * @param {string} source_id
     * @param {any} opts
     * @returns {string}
     */
    overlayReport(target_id, source_id, opts) {
        let deferred4_0;
        let deferred4_1;
        try {
            const ptr0 = passStringToWasm0(target_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(source_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            const ret = wasm.galley_overlayReport(this.__wbg_ptr, ptr0, len0, ptr1, len1, opts);
            var ptr3 = ret[0];
            var len3 = ret[1];
            if (ret[3]) {
                ptr3 = 0; len3 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * The same transaction applied — the target's own bytes under the
     * source's structure. [`overlay`](Self::overlay) is what an editor wants;
     * this is for a caller that only needs the string.
     * @param {string} target_id
     * @param {string} source_id
     * @param {any} opts
     * @returns {string}
     */
    overlayText(target_id, source_id, opts) {
        let deferred4_0;
        let deferred4_1;
        try {
            const ptr0 = passStringToWasm0(target_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(source_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            const ret = wasm.galley_overlayText(this.__wbg_ptr, ptr0, len0, ptr1, len1, opts);
            var ptr3 = ret[0];
            var len3 = ret[1];
            if (ret[3]) {
                ptr3 = 0; len3 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * One registered book, plated — the same buffer `onion_wasm::parse`
     * returns for that text, with the lex, the tree and the lint walk reused
     * for every chunk whose bytes did not change.
     *
     * Read it with the same `reader.ts` the stateless door's output uses:
     * nothing here is a new rendering, only a cheaper route to the same
     * bytes. A book that retains no text refuses.
     * @param {string} id
     * @param {boolean} diagnostics
     * @param {boolean} toc
     * @param {boolean} utf16
     * @returns {Uint8Array}
     */
    parse(id, diagnostics, toc, utf16) {
        const ptr0 = passStringToWasm0(id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.galley_parse(this.__wbg_ptr, ptr0, len0, diagnostics, toc, utf16);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * [`parse`](Self::parse) over text the host holds and has not registered
     * — a preview pane, a file not yet in the project.
     *
     * The chunk cache keys on content, so an unregistered copy of a
     * registered book still hits; what it costs over the id door is the
     * string crossing the wall.
     * @param {string} text
     * @param {boolean} diagnostics
     * @param {boolean} toc
     * @param {boolean} utf16
     * @returns {Uint8Array}
     */
    parseText(text, diagnostics, toc, utf16) {
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.galley_parseText(this.__wbg_ptr, ptr0, len0, diagnostics, toc, utf16);
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * One complete corpus publication over every target, in canonical book
     * order, in raw-book UTF-16 — the buffer `FindingsSnapshot.open` reads.
     *
     * A snapshot replaces the previous one whole; row positions are valid
     * only inside the buffer they came from.
     * @returns {Uint8Array}
     */
    publish() {
        const ret = wasm.galley_publish(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Drop a book, its text, and its cached rows. `false` when the id was
     * never registered.
     * @param {string} id
     * @returns {boolean}
     */
    remove(id) {
        const ptr0 = passStringToWasm0(id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.galley_remove(this.__wbg_ptr, ptr0, len0);
        return ret !== 0;
    }
    /**
     * Resident bytes across the whole handle: the Pantry's texts and
     * products, and the Expediter's own cached rows.
     * @returns {number}
     */
    residentBytes() {
        const ret = wasm.galley_residentBytes(this.__wbg_ptr);
        return ret;
    }
    /**
     * Replaces them. No chapter is remapped and no book refolded — judging
     * reads the config, mapping does not — so a knob flip costs a re-judge.
     * @param {SousSettings} settings
     */
    setConfig(settings) {
        _assertClass(settings, SousSettings);
        var ptr0 = settings.__destroy_into_raw();
        wasm.galley_setConfig(this.__wbg_ptr, ptr0);
    }
    /**
     * One registered book's block structure, as JSON — either side, and the
     * whole truth for drawing.
     *
     * ```ts
     * interface Skeleton {
     *   verses: { sid: string; from: number; to: number; textFrom: number; textTo: number }[];
     *                  // the \v marker span, and the verse's own text span
     *   blocks: SkeletonRow[];
     * }
     * interface SkeletonRow {
     *   sid: string; where: "leading" | "inside"; ordinal: number;   // the address
     *   marker: string;                                              // "q1"
     *   from: number; to: number;                                    // the marker node's span
     *   empty: boolean;                       // onion's empty paragraph; a source folds these away
     * }
     * ```
     *
     * `opts` is an [`OverlayOptions`](Self::overlay) — only `markers` is read
     * here — and `utf16` asks for UTF-16 offsets instead of bytes. A block is
     * LEADING when it sits immediately before its verse's `\v`, INSIDE when
     * the verse's own text is above it; ordinals count from one per address.
     * @param {string} id
     * @param {any} opts
     * @param {boolean | null} [utf16]
     * @returns {string}
     */
    skeleton(id, opts, utf16) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.galley_skeleton(this.__wbg_ptr, ptr0, len0, opts, isLikeNone(utf16) ? 0xFFFFFF : utf16 ? 1 : 0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * A TARGET block's address, answered in the source — the mirror of
     * [`targetNodeFor`](Self::target_node_for), and the same three answers.
     * @param {string} target_id
     * @param {string} source_id
     * @param {any} address
     * @param {any} opts
     * @param {boolean | null} [utf16]
     * @returns {string}
     */
    sourceNodeFor(target_id, source_id, address, opts, utf16) {
        let deferred4_0;
        let deferred4_1;
        try {
            const ptr0 = passStringToWasm0(target_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(source_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            const ret = wasm.galley_sourceNodeFor(this.__wbg_ptr, ptr0, len0, ptr1, len1, address, opts, isLikeNone(utf16) ? 0xFFFFFF : utf16 ? 1 : 0);
            var ptr3 = ret[0];
            var len3 = ret[1];
            if (ret[3]) {
                ptr3 = 0; len3 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * The structure recipe's text, the verse-text mask's sibling. No book
     * retains a structure projection, so this door takes text only.
     * @param {string} text
     * @returns {string}
     */
    structureTextOf(text) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.galley_structureTextOf(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * A SOURCE block's address, answered in the target: where it is, or
     * where the overlay would put it.
     *
     * ```ts
     * type Equivalent =
     *   | { found: SkeletonRow }                                            // same address on the other side
     *   | { absent: true; insertAt: number; where: "leading" | "inside" }   // where overlay would put it
     *   | { unpaired: true; reason: "absent" | "bridge" | "ambiguous" };    // the verse itself has no pair
     * ```
     *
     * `address.marker` is REQUIRED and is checked against the side the
     * address was taken from: if that position still exists but now spells
     * something else, the call THROWS ("… names q2 but the node there is q1
     * — the address is stale") rather than answering about another node. The
     * position is still the key; the name is only the check.
     * @param {string} target_id
     * @param {string} source_id
     * @param {any} address
     * @param {any} opts
     * @param {boolean | null} [utf16]
     * @returns {string}
     */
    targetNodeFor(target_id, source_id, address, opts, utf16) {
        let deferred4_0;
        let deferred4_1;
        try {
            const ptr0 = passStringToWasm0(target_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(source_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            const ret = wasm.galley_targetNodeFor(this.__wbg_ptr, ptr0, len0, ptr1, len1, address, opts, isLikeNone(utf16) ? 0xFFFFFF : utf16 ? 1 : 0);
            var ptr3 = ret[0];
            var len3 = ret[1];
            if (ret[3]) {
                ptr3 = 0; len3 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * One registered book's census — its chapter rows and verse anchors, off
     * the `Toc` that `update` built and the Pantry pins.
     *
     * Nothing is derived here: no chunk is resolved, no text is read, no wire
     * is plated. `utf16` rebases every offset through the book's own retained
     * table; the default is bytes.
     *
     * Read it with `usfm-galley/toc-reader`. The layout is generated from the
     * same declaration the writer is, so no consumer learns one.
     * @param {string} id
     * @param {boolean | null} [utf16]
     * @returns {Uint8Array}
     */
    toc(id, utf16) {
        const ptr0 = passStringToWasm0(id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.galley_toc(this.__wbg_ptr, ptr0, len0, isLikeNone(utf16) ? 0xFFFFFF : utf16 ? 1 : 0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * The same over every registered book in `scope`, in canonical book order
     * — the project-wide census, and the call that takes one parse per book
     * off a project's open.
     *
     * The scope is wider than `findAll`'s on purpose: a reference that kept no
     * text still kept its `Toc`, so it is listed. The one thing it cannot
     * answer is `utf16`, because the table that rebases offsets travels with
     * the text.
     * @param {string | null} [scope]
     * @param {boolean | null} [utf16]
     * @returns {Uint8Array}
     */
    tocAll(scope, utf16) {
        var ptr0 = isLikeNone(scope) ? 0 : passStringToWasm0(scope, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len0 = WASM_VECTOR_LEN;
        const ret = wasm.galley_tocAll(this.__wbg_ptr, ptr0, len0, isLikeNone(utf16) ? 0xFFFFFF : utf16 ? 1 : 0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * Register or replace one whole book under the caller's `id`, as a
     * target: it keeps its text, and it publishes findings.
     *
     * Returns the `\id` line's canonical book code — `"MRK"` — which is what
     * orders the publication. Idempotent: the same text costs a checksum.
     *
     * `text` must be LF-normalized, the contract every door here documents.
     * @param {string} id
     * @param {string} text
     * @returns {string}
     */
    update(id, text) {
        let deferred4_0;
        let deferred4_1;
        try {
            const ptr0 = passStringToWasm0(id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            const ret = wasm.galley_update(this.__wbg_ptr, ptr0, len0, ptr1, len1);
            var ptr3 = ret[0];
            var len3 = ret[1];
            if (ret[3]) {
                ptr3 = 0; len3 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * The same, as a declared source: one projected grapheme length per
     * verse and no text at all, so a reference costs a fraction of a target.
     *
     * A reference publishes no findings of its own; it is the denominator
     * the length lane compares a target's verses against.
     *
     * `keepText` — omitted is `false` — makes it keep the text and the
     * projection a target keeps too, which is what [`find`](Self::find) and
     * `findAll`'s `"references"` scope read. It costs what a target costs
     * minus the resident analysis; a source nobody searches should stay off
     * it.
     * @param {string} id
     * @param {string} text
     * @param {boolean | null} [keep_text]
     * @returns {string}
     */
    updateReference(id, text, keep_text) {
        let deferred4_0;
        let deferred4_1;
        try {
            const ptr0 = passStringToWasm0(id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            const ret = wasm.galley_updateReference(this.__wbg_ptr, ptr0, len0, ptr1, len1, isLikeNone(keep_text) ? 0xFFFFFF : keep_text ? 1 : 0);
            var ptr3 = ret[0];
            var len3 = ret[1];
            if (ret[3]) {
                ptr3 = 0; len3 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * One registered book's verse text, off the projection it already
     * retains — no mask is cut and no text crosses in.
     *
     * A reference registered without its text retains no projection and
     * refuses.
     * @param {string} id
     * @returns {string}
     */
    verseText(id) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.galley_verseText(this.__wbg_ptr, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * The verse text of loose text. See [`parse_text`](Self::parse_text).
     *
     * TODO: this DISCARDS the mask. `Mask` carries `ranges`/`starts` — the map
     * from a masked offset back to the source — and sous needs it to report a
     * finding against the unmasked document. Returning the text alone means
     * whatever consumes this cannot get back.
     * @param {string} text
     * @returns {string}
     */
    verseTextOf(text) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.galley_verseTextOf(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
}
if (Symbol.dispose) Galley.prototype[Symbol.dispose] = Galley.prototype.free;

/**
 * The judging settings that cross the wall: every plain scalar of
 * [`JudgingConfig`], flat, so bindgen writes the getters and setters and JS
 * assigns `settings.casing = false`.
 *
 * Not on the wall: `bands` and `word_bands` (a `Staircase` is a validated
 * ladder, not a plain field), `letters` and `doubles` (tri-state policies),
 * and the roster bounds. They have no plain-field shape and no consumer has
 * asked for them; everything not a knob keeps the current config's value
 * through [`apply`](SousSettings::apply), so widening this later breaks nothing.
 */
export class SousSettings {
    static __wrap(ptr) {
        const obj = Object.create(SousSettings.prototype);
        obj.__wbg_ptr = ptr;
        SousSettingsFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        SousSettingsFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_soussettings_free(ptr, 0);
    }
    /**
     * @returns {boolean}
     */
    get casing() {
        const ret = wasm.__wbg_get_soussettings_casing(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get doubled() {
        const ret = wasm.__wbg_get_soussettings_doubled(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {number}
     */
    get doubles_productive_bp() {
        const ret = wasm.__wbg_get_soussettings_doubles_productive_bp(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {boolean}
     */
    get exact_neighbor() {
        const ret = wasm.__wbg_get_soussettings_exact_neighbor(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get lengths_enabled() {
        const ret = wasm.__wbg_get_soussettings_lengths_enabled(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get letter_runs() {
        const ret = wasm.__wbg_get_soussettings_letter_runs(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {number}
     */
    get min_verses() {
        const ret = wasm.__wbg_get_soussettings_min_verses(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {boolean}
     */
    get placement() {
        const ret = wasm.__wbg_get_soussettings_placement(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get pooled_neighbor() {
        const ret = wasm.__wbg_get_soussettings_pooled_neighbor(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get presence() {
        const ret = wasm.__wbg_get_soussettings_presence(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get rarity() {
        const ret = wasm.__wbg_get_soussettings_rarity(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {boolean}
     */
    get run_shape() {
        const ret = wasm.__wbg_get_soussettings_run_shape(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {number}
     */
    get sentence_start_upper_bp() {
        const ret = wasm.__wbg_get_soussettings_sentence_start_upper_bp(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {boolean}
     */
    get sentence_start() {
        const ret = wasm.__wbg_get_soussettings_sentence_start(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {number}
     */
    get source_copy_min_run() {
        const ret = wasm.__wbg_get_soussettings_source_copy_min_run(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {boolean}
     */
    get source_copy() {
        const ret = wasm.__wbg_get_soussettings_source_copy(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {number}
     */
    get support_floor() {
        const ret = wasm.__wbg_get_soussettings_support_floor(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    get terminal_upper_share_bp() {
        const ret = wasm.__wbg_get_soussettings_terminal_upper_share_bp(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    get word_length_sigma() {
        const ret = wasm.__wbg_get_soussettings_word_length_sigma(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {boolean}
     */
    get word_length() {
        const ret = wasm.__wbg_get_soussettings_word_length(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {number}
     */
    get word_support_floor() {
        const ret = wasm.__wbg_get_soussettings_word_support_floor(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    get z_long() {
        const ret = wasm.__wbg_get_soussettings_z_long(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    get z_short() {
        const ret = wasm.__wbg_get_soussettings_z_short(this.__wbg_ptr);
        return ret;
    }
    /**
     * @param {boolean} arg0
     */
    set casing(arg0) {
        wasm.__wbg_set_soussettings_casing(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set doubled(arg0) {
        wasm.__wbg_set_soussettings_doubled(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set doubles_productive_bp(arg0) {
        wasm.__wbg_set_soussettings_doubles_productive_bp(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set exact_neighbor(arg0) {
        wasm.__wbg_set_soussettings_exact_neighbor(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set lengths_enabled(arg0) {
        wasm.__wbg_set_soussettings_lengths_enabled(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set letter_runs(arg0) {
        wasm.__wbg_set_soussettings_letter_runs(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set min_verses(arg0) {
        wasm.__wbg_set_soussettings_min_verses(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set placement(arg0) {
        wasm.__wbg_set_soussettings_placement(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set pooled_neighbor(arg0) {
        wasm.__wbg_set_soussettings_pooled_neighbor(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set presence(arg0) {
        wasm.__wbg_set_soussettings_presence(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set rarity(arg0) {
        wasm.__wbg_set_soussettings_rarity(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set run_shape(arg0) {
        wasm.__wbg_set_soussettings_run_shape(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set sentence_start_upper_bp(arg0) {
        wasm.__wbg_set_soussettings_sentence_start_upper_bp(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set sentence_start(arg0) {
        wasm.__wbg_set_soussettings_sentence_start(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set source_copy_min_run(arg0) {
        wasm.__wbg_set_soussettings_source_copy_min_run(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set source_copy(arg0) {
        wasm.__wbg_set_soussettings_source_copy(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set support_floor(arg0) {
        wasm.__wbg_set_soussettings_support_floor(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set terminal_upper_share_bp(arg0) {
        wasm.__wbg_set_soussettings_terminal_upper_share_bp(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set word_length_sigma(arg0) {
        wasm.__wbg_set_soussettings_word_length_sigma(this.__wbg_ptr, arg0);
    }
    /**
     * @param {boolean} arg0
     */
    set word_length(arg0) {
        wasm.__wbg_set_soussettings_word_length(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set word_support_floor(arg0) {
        wasm.__wbg_set_soussettings_word_support_floor(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set z_long(arg0) {
        wasm.__wbg_set_soussettings_z_long(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set z_short(arg0) {
        wasm.__wbg_set_soussettings_z_short(this.__wbg_ptr, arg0);
    }
}
if (Symbol.dispose) SousSettings.prototype[Symbol.dispose] = SousSettings.prototype.free;

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
 * One attribute name against one marker row: the `AttrResolution` code.
 *
 * An empty name is the bare default-value form, which resolves through the
 * row's own default — so it is a legitimate argument, not a mistake.
 * @param {string} name
 * @param {number} marker
 * @returns {number}
 */
export function attrResolve(name, marker) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.attrResolve(ptr0, len0, marker);
    return ret >>> 0;
}

/**
 * The k/v view of one `AttrList` token, flat.
 *
 * `[from, to)` is the list token's span in the CALLER's space — UTF-16 when
 * `utf16` is non-zero, bytes otherwise, as `locate` reads it — and every word
 * comes back in that same space.
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
 * @param {string} text
 * @param {number} from
 * @param {number} to
 * @param {number} utf16
 * @returns {Uint32Array}
 */
export function attrs(text, from, to, utf16) {
    const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.attrs(ptr0, len0, from, to, utf16);
    var v2 = getArrayU32FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
    return v2;
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
 *
 * `text_mode` is `"none"` | `"words"` | `"chars"`: the intra-verse runs a
 * `modified` unit is highlighted by, at UAX-29 word or grapheme grain.
 * `"none"` computes nothing — no CST, no mask — and yields the same JSON the
 * door returned before runs existed.
 * @param {string} baseline
 * @param {string} current
 * @param {string} text_mode
 * @returns {string}
 */
export function diff(baseline, current, text_mode) {
    let deferred5_0;
    let deferred5_1;
    try {
        const ptr0 = passStringToWasm0(baseline, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(current, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(text_mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.diff(ptr0, len0, ptr1, len1, ptr2, len2);
        var ptr4 = ret[0];
        var len4 = ret[1];
        if (ret[3]) {
            ptr4 = 0; len4 = 0;
            throw takeFromExternrefTable0(ret[2]);
        }
        deferred5_0 = ptr4;
        deferred5_1 = len4;
        return getStringFromWasm0(ptr4, len4);
    } finally {
        wasm.__wbindgen_free(deferred5_0, deferred5_1, 1);
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
 * @param {string} text
 * @param {string} recipe
 * @returns {object}
 */
export function mask(text, recipe) {
    const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(recipe, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.mask(ptr0, len0, ptr1, len1);
    return ret;
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
 * THE read call. One document in, one buffer out.
 *
 * The buffer is a plated parse — tokens, the tree, and whatever `opts` asked
 * for besides — read by `reader.ts`, which is generated from the same schema
 * as the writer. Nothing is retained wasm-side: the `Uint8Array` is JS's, the
 * collector reclaims it, and there is no `free`.
 *
 * The three booleans are positional because an object crossing the wall would
 * be `Reflect::get` per key with a misspelling silently reading as `false`.
 * `reader.ts` wraps this as `parse(text, { diagnostics, toc, utf16 })`, where
 * a misspelled key is a compile error instead.
 *
 * `text` must be LF-normalized (see the module doc); a debug build asserts it.
 * @param {string} text
 * @param {boolean} diagnostics
 * @param {boolean} toc
 * @param {boolean} utf16
 * @returns {Uint8Array}
 */
export function parse(text, diagnostics, toc, utf16) {
    const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.parse(ptr0, len0, diagnostics, toc, utf16);
    var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v2;
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
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_408e67f47ca7b58b: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg___wbindgen_boolean_get_c9c83ebd41b34df3: function(arg0) {
            const v = arg0;
            const ret = typeof(v) === 'boolean' ? v : undefined;
            return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
        },
        __wbg___wbindgen_debug_string_a57024b9c6e4a48b: function(arg0, arg1) {
            const ret = debugString(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_is_null_7d13f41e1a2d5140: function(arg0) {
            const ret = arg0 === null;
            return ret;
        },
        __wbg___wbindgen_is_object_a2790eb24c211ea0: function(arg0) {
            const val = arg0;
            const ret = typeof(val) === 'object' && val !== null;
            return ret;
        },
        __wbg___wbindgen_is_undefined_6cff064c44e0d823: function(arg0) {
            const ret = arg0 === undefined;
            return ret;
        },
        __wbg___wbindgen_number_get_136b9679cab35cfb: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'number' ? obj : undefined;
            getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_string_get_d154f1e671052120: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_bb96b2010945f0bc: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_from_74f3d90e0ff11240: function(arg0) {
            const ret = Array.from(arg0);
            return ret;
        },
        __wbg_get_971a0c45d172643f: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_get_unchecked_e20b893aeafc3fca: function(arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return ret;
        },
        __wbg_length_ecfa2c63d3d0d82c: function(arg0) {
            const ret = arg0.length;
            return ret;
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
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
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
        "./usfm_galley_bg.js": import0,
    };
}

const EditsFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_edits_free(ptr, 1));
const FingerprintFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_fingerprint_free(ptr, 1));
const FormatOptsFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_formatopts_free(ptr, 1));
const GalleyFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_galley_free(ptr, 1));
const SousSettingsFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_soussettings_free(ptr, 1));
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

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
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
        module_or_path = new URL('usfm_galley_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
