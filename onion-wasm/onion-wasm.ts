/**
 * onion-wasm.ts — the WRITE path's decoders.
 *
 * The read path moved to `reader.ts`, which is GENERATED from
 * `onion/src/wire/schema.rs` alongside the writer that produces the bytes it
 * parses. What used to live here — the strides, the bit layouts, the sentinel,
 * the class and flag tables, `AnalysisView` — is gone, not relocated: a
 * consumer reads a token through a generated accessor now and never learns a
 * layout, so there is nothing left to keep in step by hand.
 *
 * What remains is the format transaction, which is not a read: it hands out a
 * wasm HANDLE the caller must free, and its wire is its own.
 *
 *   import init, { formatEdits } from "./pkg/onion_wasm.js";
 *   import { consumeEdits } from "./onion-wasm.js";
 *
 *   await init();
 *   const edits = consumeEdits(formatEdits(text, opts));
 *
 * Offsets here are UTF-16 code units, unconditionally — unlike `parse`, where
 * the space is a per-call choice. The format transaction exists only to be
 * applied to an editor's document, so it has only one caller's addressing to
 * serve.
 */

/** u32s per entry in an edit's span plane. */
export const EDIT_STRIDE = 2;

/** One edit. ASCII inserts; empty is a deletion, `from === to` an insertion. */
export interface FixEdit {
  from: number;
  to: number;
  insert: string;
}

/**
 * A wasm `Edits` handle — what `formatEdits` and `formatEditsIn` return.
 * `[from, to]` per edit in UTF-16, one concatenated ASCII insert blob, one byte
 * length per edit.
 *
 * `formatEditsIn(text, from, to, opts)` is the SCOPED transaction — the same
 * whole-book analysis, filtered to a UTF-16 window. Do not filter an edit list
 * in JS instead: the engine drops a boundary-STRADDLING edit whole rather than
 * cutting it, keeps a multi-edit claim only if all of it is inside, and counts
 * a pure insertion sitting ON either edge as inside. A JS `filter` cannot see
 * the claim groups, so it will happily keep half of a `bridge-empty-verses`
 * pair — and half an edit corrupts.
 */
export interface RawEdits {
  readonly spans: Uint32Array;
  readonly lens: Uint32Array;
  readonly text: string;
}

export type OwnedEdits = RawEdits & { free(): void };

/**
 * The edit list, decoded.
 *
 * Eager, unlike the reads: a format transaction is hundreds of edits at the
 * outside, the caller holds it while a human looks at a preview, and the wasm
 * handle behind it must not stay alive that long.
 */
export function editList(raw: RawEdits): FixEdit[] {
  const out: FixEdit[] = [];
  let at = 0;
  for (let edit = 0; edit < raw.lens.length; edit++) {
    const len = raw.lens[edit];
    out.push({
      from: raw.spans[edit * EDIT_STRIDE],
      to: raw.spans[edit * EDIT_STRIDE + 1],
      insert: raw.text.slice(at, at + len),
    });
    at += len;
  }
  return out;
}

/** THE PAVED PATH: decode every edit out, then free the wasm allocation. */
export function consumeEdits(raw: OwnedEdits): FixEdit[] {
  try {
    return editList(raw);
  } finally {
    raw.free();
  }
}
