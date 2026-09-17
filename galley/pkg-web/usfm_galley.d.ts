/* tslint:disable */
/* eslint-disable */

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
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    readonly lens: Uint32Array;
    readonly spans: Uint32Array;
    readonly text: string;
}

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
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Ranges of `current`'s text whose chunk this fingerprint never saw, as
     * flat `from, to` pairs.
     */
    changedChunks(current: Fingerprint): Uint32Array;
    /**
     * Chunks in the text this fingerprint was taken from.
     */
    chunkCount(): number;
    /**
     * Whether the two byte strings differ at all.
     */
    differsFrom(current: Fingerprint): boolean;
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
 * The corpus, resident: one [`Expediter`], one Pantry, one snapshot out.
 *
 * One per project, not per document. Books go in whole by caller id and come
 * back as one complete publication; the Pantry inside owns the chunk cache,
 * so the onion methods read the same warm chunks the analysis does.
 */
export class Galley {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Ranges of `text` whose chunk this book's last update never saw — the
     * chunks a host would have to re-derive, in `text`'s own byte offsets,
     * as `from, to` pairs.
     *
     * `undefined` when the id is not registered, which is the question a
     * host asks before deciding to register it.
     */
    changedSinceUpdate(id: string, text: string): Uint32Array | undefined;
    /**
     * A copy of the settings the next [`publish`](Self::publish) judges with.
     */
    config(): SousSettings;
    /**
     * Cached chunk units. Zero for a one-chapter book: galley does not cache
     * what it cannot reuse.
     */
    entryCount(): number;
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
     */
    find(id: string, needle: string, opts: any): Uint8Array;
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
     */
    findAll(needle: string, opts: any): Uint8Array;
    /**
     * Chunk starts plus one checksum each, and no text — the ~1 KB baseline
     * user land keeps beside a file on disk.
     */
    fingerprint(text: string): Fingerprint;
    /**
     * Books rescanned for sites rather than replaying cached rows.
     */
    lastLocated(): number;
    /**
     * Chapters mapped by the last [`publish`](Self::publish).
     */
    lastMapped(): number;
    /**
     * Targets re-paired against their declared source.
     */
    lastPaired(): number;
    /**
     * Of those, the ones that kept an observation and re-walked only part.
     */
    lastRemapped(): number;
    /**
     * Declared sources the source-copy lane would have read and could not,
     * because they were registered while `settings.source_copy` was off and so
     * kept no word lane.
     *
     * Nonzero after turning the lane on means "re-send those references'
     * text", not "nothing was found".
     */
    lastWordlessReferences(): number;
    /**
     * One registered book's diagnostics, off its retained text.
     *
     * Onion has no lint door of its own: a lint report crosses the wall as
     * the `diagnostics` section of a parse buffer, so this is
     * [`parse`](Self::parse) with that section alone asked for, read by the
     * same `reader.ts`.
     */
    lint(id: string): Uint8Array;
    /**
     * Chunk units computed rather than reused, cumulative.
     */
    misses(): number;
    /**
     * `budgetBytes` bounds resident products; omit it for 16 MB.
     */
    constructor(budget_bytes?: number | null);
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
     */
    overlay(target_id: string, source_id: string, opts: any): Edits;
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
     */
    overlayReport(target_id: string, source_id: string, opts: any): string;
    /**
     * The same transaction applied — the target's own bytes under the
     * source's structure. [`overlay`](Self::overlay) is what an editor wants;
     * this is for a caller that only needs the string.
     */
    overlayText(target_id: string, source_id: string, opts: any): string;
    /**
     * One registered book, plated — the same buffer `onion_wasm::parse`
     * returns for that text, with the lex, the tree and the lint walk reused
     * for every chunk whose bytes did not change.
     *
     * Read it with the same `reader.ts` the stateless door's output uses:
     * nothing here is a new rendering, only a cheaper route to the same
     * bytes. A book that retains no text refuses.
     */
    parse(id: string, diagnostics: boolean, toc: boolean, utf16: boolean): Uint8Array;
    /**
     * [`parse`](Self::parse) over text the host holds and has not registered
     * — a preview pane, a file not yet in the project.
     *
     * The chunk cache keys on content, so an unregistered copy of a
     * registered book still hits; what it costs over the id door is the
     * string crossing the wall.
     */
    parseText(text: string, diagnostics: boolean, toc: boolean, utf16: boolean): Uint8Array;
    /**
     * One complete corpus publication over every target, in canonical book
     * order, in raw-book UTF-16 — the buffer `FindingsSnapshot.open` reads.
     *
     * A snapshot replaces the previous one whole; row positions are valid
     * only inside the buffer they came from.
     */
    publish(): Uint8Array;
    /**
     * Drop a book, its text, and its cached rows. `false` when the id was
     * never registered.
     */
    remove(id: string): boolean;
    /**
     * Resident bytes across the whole handle: the Pantry's texts and
     * products, and the Expediter's own cached rows.
     */
    residentBytes(): number;
    /**
     * Replaces them. No chapter is remapped and no book refolded — judging
     * reads the config, mapping does not — so a knob flip costs a re-judge.
     */
    setConfig(settings: SousSettings): void;
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
     */
    skeleton(id: string, opts: any, utf16?: boolean | null): string;
    /**
     * A TARGET block's address, answered in the source — the mirror of
     * [`targetNodeFor`](Self::target_node_for), and the same three answers.
     */
    sourceNodeFor(target_id: string, source_id: string, address: any, opts: any, utf16?: boolean | null): string;
    /**
     * The structure recipe's text, the verse-text mask's sibling. No book
     * retains a structure projection, so this door takes text only.
     */
    structureTextOf(text: string): string;
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
     */
    targetNodeFor(target_id: string, source_id: string, address: any, opts: any, utf16?: boolean | null): string;
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
     */
    toc(id: string, utf16?: boolean | null): Uint8Array;
    /**
     * The same over every registered book in `scope`, in canonical book order
     * — the project-wide census, and the call that takes one parse per book
     * off a project's open.
     *
     * The scope is wider than `findAll`'s on purpose: a reference that kept no
     * text still kept its `Toc`, so it is listed. The one thing it cannot
     * answer is `utf16`, because the table that rebases offsets travels with
     * the text.
     */
    tocAll(scope?: string | null, utf16?: boolean | null): Uint8Array;
    /**
     * Register or replace one whole book under the caller's `id`, as a
     * target: it keeps its text, and it publishes findings.
     *
     * Returns the `\id` line's canonical book code — `"MRK"` — which is what
     * orders the publication. Idempotent: the same text costs a checksum.
     *
     * `text` must be LF-normalized, the contract every door here documents.
     */
    update(id: string, text: string): string;
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
     */
    updateReference(id: string, text: string, keep_text?: boolean | null): string;
    /**
     * One registered book's verse text, off the projection it already
     * retains — no mask is cut and no text crosses in.
     *
     * A reference registered without its text retains no projection and
     * refuses.
     */
    verseText(id: string): string;
    /**
     * The verse text of loose text. See [`parse_text`](Self::parse_text).
     *
     * TODO: this DISCARDS the mask. `Mask` carries `ranges`/`starts` — the map
     * from a masked offset back to the source — and sous needs it to report a
     * finding against the unmasked document. Returning the text alone means
     * whatever consumes this cannot get back.
     */
    verseTextOf(text: string): string;
}

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
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    casing: boolean;
    doubled: boolean;
    doubles_productive_bp: number;
    exact_neighbor: boolean;
    lengths_enabled: boolean;
    letter_runs: boolean;
    min_verses: number;
    placement: boolean;
    pooled_neighbor: boolean;
    presence: boolean;
    rarity: boolean;
    run_shape: boolean;
    sentence_start_upper_bp: number;
    sentence_start: boolean;
    source_copy_min_run: number;
    source_copy: boolean;
    support_floor: number;
    terminal_upper_share_bp: number;
    word_length_sigma: number;
    word_length: boolean;
    word_support_floor: number;
    z_long: number;
    z_short: number;
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
 * One attribute name against one marker row: the `AttrResolution` code.
 *
 * An empty name is the bare default-value form, which resolves through the
 * row's own default — so it is a legitimate argument, not a mistake.
 */
export function attrResolve(name: string, marker: number): number;

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
 */
export function attrs(text: string, from: number, to: number, utf16: number): Uint32Array;

/**
 * The first `\id`'s book code — `"GEN"`. EMPTY when the document declares
 * none (real in the wild: BSB Ecclesiastes); the `missing-id` diagnostic is
 * where that becomes a finding, not here.
 */
export function book(text: string): string;

/**
 * The diff skeleton as JSON. Spans are UTF-16 offsets into each side's own
 * document.
 *
 * `text_mode` is `"none"` | `"words"` | `"chars"`: the intra-verse runs a
 * `modified` unit is highlighted by, at UAX-29 word or grapheme grain.
 * `"none"` computes nothing — no CST, no mask — and yields the same JSON the
 * door returned before runs existed.
 */
export function diff(baseline: string, current: string, text_mode: string): string;

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
 * The same transaction bounded to `from..to` (UTF-16, the offsets the editor
 * already holds — a chapter's span out of the `chapters` read).
 *
 * The range crosses the wall in UTF-16 and is translated here, on the same
 * index the edits go out through. The policy is the library's: an edit is kept
 * only if its whole span is inside, a multi-edit claim only if all of it is,
 * and a pure insertion sitting ON either edge is inside.
 */
export function formatEditsIn(text: string, from: number, to: number, opts: FormatOpts): Edits;

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
 */
export function mask(text: string, recipe: string): object;

/**
 * The merged document. An unknown unit id REJECTS loudly — the caller must
 * re-diff, and there is no fuzzy stale-id fallback.
 */
export function merge(baseline: string, current: string, decisions_json: string, _default: string): string;

export function mergeSplices(baseline: string, current: string, decisions_json: string, _default: string): Splices;

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
 */
export function parse(text: string, diagnostics: boolean, toc: boolean, utf16: boolean): Uint8Array;

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

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_fingerprint_free: (a: number, b: number) => void;
    readonly __wbg_galley_free: (a: number, b: number) => void;
    readonly __wbg_get_soussettings_casing: (a: number) => number;
    readonly __wbg_get_soussettings_doubled: (a: number) => number;
    readonly __wbg_get_soussettings_doubles_productive_bp: (a: number) => number;
    readonly __wbg_get_soussettings_exact_neighbor: (a: number) => number;
    readonly __wbg_get_soussettings_lengths_enabled: (a: number) => number;
    readonly __wbg_get_soussettings_letter_runs: (a: number) => number;
    readonly __wbg_get_soussettings_min_verses: (a: number) => number;
    readonly __wbg_get_soussettings_placement: (a: number) => number;
    readonly __wbg_get_soussettings_pooled_neighbor: (a: number) => number;
    readonly __wbg_get_soussettings_presence: (a: number) => number;
    readonly __wbg_get_soussettings_rarity: (a: number) => number;
    readonly __wbg_get_soussettings_run_shape: (a: number) => number;
    readonly __wbg_get_soussettings_sentence_start: (a: number) => number;
    readonly __wbg_get_soussettings_sentence_start_upper_bp: (a: number) => number;
    readonly __wbg_get_soussettings_source_copy: (a: number) => number;
    readonly __wbg_get_soussettings_source_copy_min_run: (a: number) => number;
    readonly __wbg_get_soussettings_support_floor: (a: number) => number;
    readonly __wbg_get_soussettings_terminal_upper_share_bp: (a: number) => number;
    readonly __wbg_get_soussettings_word_length: (a: number) => number;
    readonly __wbg_get_soussettings_word_length_sigma: (a: number) => number;
    readonly __wbg_get_soussettings_word_support_floor: (a: number) => number;
    readonly __wbg_get_soussettings_z_long: (a: number) => number;
    readonly __wbg_get_soussettings_z_short: (a: number) => number;
    readonly __wbg_set_soussettings_casing: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_doubled: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_doubles_productive_bp: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_exact_neighbor: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_lengths_enabled: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_letter_runs: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_min_verses: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_placement: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_pooled_neighbor: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_presence: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_rarity: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_run_shape: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_sentence_start: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_sentence_start_upper_bp: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_source_copy: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_source_copy_min_run: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_support_floor: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_terminal_upper_share_bp: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_word_length: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_word_length_sigma: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_word_support_floor: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_z_long: (a: number, b: number) => void;
    readonly __wbg_set_soussettings_z_short: (a: number, b: number) => void;
    readonly __wbg_soussettings_free: (a: number, b: number) => void;
    readonly fingerprint_changedChunks: (a: number, b: number) => [number, number];
    readonly fingerprint_chunkCount: (a: number) => number;
    readonly fingerprint_differsFrom: (a: number, b: number) => number;
    readonly galley_changedSinceUpdate: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly galley_config: (a: number) => number;
    readonly galley_entryCount: (a: number) => number;
    readonly galley_find: (a: number, b: number, c: number, d: number, e: number, f: any) => [number, number, number, number];
    readonly galley_findAll: (a: number, b: number, c: number, d: any) => [number, number, number, number];
    readonly galley_fingerprint: (a: number, b: number, c: number) => number;
    readonly galley_lastLocated: (a: number) => number;
    readonly galley_lastMapped: (a: number) => number;
    readonly galley_lastPaired: (a: number) => number;
    readonly galley_lastRemapped: (a: number) => number;
    readonly galley_lastWordlessReferences: (a: number) => number;
    readonly galley_lint: (a: number, b: number, c: number) => [number, number, number, number];
    readonly galley_misses: (a: number) => number;
    readonly galley_new: (a: number, b: number) => number;
    readonly galley_overlay: (a: number, b: number, c: number, d: number, e: number, f: any) => [number, number, number];
    readonly galley_overlayReport: (a: number, b: number, c: number, d: number, e: number, f: any) => [number, number, number, number];
    readonly galley_overlayText: (a: number, b: number, c: number, d: number, e: number, f: any) => [number, number, number, number];
    readonly galley_parse: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly galley_parseText: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly galley_publish: (a: number) => [number, number, number, number];
    readonly galley_remove: (a: number, b: number, c: number) => number;
    readonly galley_residentBytes: (a: number) => number;
    readonly galley_setConfig: (a: number, b: number) => void;
    readonly galley_skeleton: (a: number, b: number, c: number, d: any, e: number) => [number, number, number, number];
    readonly galley_sourceNodeFor: (a: number, b: number, c: number, d: number, e: number, f: any, g: any, h: number) => [number, number, number, number];
    readonly galley_structureTextOf: (a: number, b: number, c: number) => [number, number];
    readonly galley_targetNodeFor: (a: number, b: number, c: number, d: number, e: number, f: any, g: any, h: number) => [number, number, number, number];
    readonly galley_toc: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly galley_tocAll: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly galley_update: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly galley_updateReference: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly galley_verseText: (a: number, b: number, c: number) => [number, number, number, number];
    readonly galley_verseTextOf: (a: number, b: number, c: number) => [number, number];
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
    readonly attrResolve: (a: number, b: number, c: number) => number;
    readonly attrs: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly book: (a: number, b: number) => [number, number];
    readonly diff: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly edits_lens: (a: number) => [number, number];
    readonly edits_spans: (a: number) => [number, number];
    readonly edits_text: (a: number) => [number, number];
    readonly format: (a: number, b: number, c: number) => [number, number];
    readonly formatEdits: (a: number, b: number, c: number) => number;
    readonly formatEditsIn: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly formatopts_new: () => number;
    readonly formatopts_setRemoveMarkers: (a: number, b: number, c: number) => void;
    readonly formatopts_setRepairs: (a: number, b: number, c: number) => void;
    readonly locate: (a: number, b: number, c: number) => [number, number];
    readonly mask: (a: number, b: number, c: number, d: number) => any;
    readonly merge: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number, number];
    readonly mergeSplices: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number];
    readonly parse: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly splices_inserts: (a: number) => [number, number];
    readonly splices_spans: (a: number) => [number, number];
    readonly toByte: (a: number, b: number, c: number) => number;
    readonly toUtf16: (a: number, b: number, c: number) => number;
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
