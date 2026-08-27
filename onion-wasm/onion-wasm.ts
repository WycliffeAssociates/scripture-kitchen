/**
 * onion-wasm.ts — the decoders over onion-wasm's flat arrays.
 *
 * THIS FILE IS THE SCHEMA. The strides, the bit layouts and the sentinel live
 * here and nowhere else on the JS side; it ships in the same npm package as the
 * `.wasm` it decodes, so the two can never drift. Nothing here has a
 * dependency, and nothing here re-derives an engine fact — every decoder is a
 * rename of numbers the engine already decided.
 *
 * Usage:
 *
 *   import init, { analyze } from "./pkg/onion_wasm.js";
 *   import { analysis, wants } from "./onion-wasm.js";
 *
 *   await init();
 *   const a = analysis(analyze(text, wants({ chapters: true, diagnostics: true })));
 *   for (const chapter of a.chapters()) …
 *   for (const finding of a.diagnostics()) …
 *
 * `analyze` returns a PLAIN OBJECT of typed arrays — nothing wasm-side outlives
 * the call, and there is nothing to free.
 *
 * Every offset is a CodeMirror UTF-16 offset into the SAME text that went in,
 * which must be LF-normalized — see onion-wasm's module doc. Display strings
 * come from `doc.sliceString(span.from, span.to)`: no marker name, chapter
 * label or message text ever crosses the wall.
 */

// ---------------------------------------------------------------------------
// The schema
// ---------------------------------------------------------------------------

/** One bit per read. An unset bit computes nothing and returns nothing. */
export const WANTS = {
  CHAPTERS: 1 << 0,
  BLOCKS: 1 << 1,
  /** Note extents AND their interiors — `notes()` and `noteParts()`. */
  NOTE_EXTENTS: 1 << 2,
  TOKEN_SPANS: 1 << 3,
  TEXT_RUNS: 1 << 4,
  VERSE_ANCHORS: 1 << 5,
  /** Findings and their fixes — one is useless without the other. */
  DIAGNOSTICS: 1 << 6,
  /** One row per MARKED LINE: the read an editor's line rules read. */
  LINES: 1 << 7,
} as const;

export const WANTS_ALL =
  WANTS.CHAPTERS |
  WANTS.BLOCKS |
  WANTS.NOTE_EXTENTS |
  WANTS.TOKEN_SPANS |
  WANTS.TEXT_RUNS |
  WANTS.VERSE_ANCHORS |
  WANTS.DIAGNOSTICS |
  WANTS.LINES;

/**
 * The reads, by NAME — one optional boolean per decoder method on
 * `AnalysisView`, so what you ask for is spelled the way you read it back.
 *
 * `notes` is the one key that covers two decoders (`notes()` and
 * `noteParts()`): the inside of a note is meaningless without the extent it
 * belongs to, so the engine gates them on one bit.
 */
export interface Wants {
  chapters?: boolean;
  blocks?: boolean;
  lines?: boolean;
  /** `notes()` AND `noteParts()`. */
  notes?: boolean;
  tokens?: boolean;
  textRuns?: boolean;
  verseAnchors?: boolean;
  /** `diagnostics()` and the fixes they offer. */
  diagnostics?: boolean;
}

/**
 * A `Wants` object → the u32 the wasm `analyze` takes.
 *
 *   analyze(text, wants({ chapters: true, verseAnchors: true }))
 *
 * The bitmask stays the WIRE form; this lives entirely in TS on purpose. A
 * misspelled key in an object crossing the wall would be silently ignored and
 * the read would come back empty — here it is a compile error, because an
 * object literal cannot carry a property `Wants` does not declare. `WANTS` is
 * still exported for anyone composing bits dynamically.
 */
export function wants(w: Wants): number {
  return (
    (w.chapters ? WANTS.CHAPTERS : 0) |
    (w.blocks ? WANTS.BLOCKS : 0) |
    (w.lines ? WANTS.LINES : 0) |
    (w.notes ? WANTS.NOTE_EXTENTS : 0) |
    (w.tokens ? WANTS.TOKEN_SPANS : 0) |
    (w.textRuns ? WANTS.TEXT_RUNS : 0) |
    (w.verseAnchors ? WANTS.VERSE_ANCHORS : 0) |
    (w.diagnostics ? WANTS.DIAGNOSTICS : 0)
  );
}

// There is deliberately no COMMIT set here. Which reads an editor needs per
// accepted change is that editor's opinion — a CodeMirror probe's answer is
// not a library fact — so the app composes its own from `wants`.

/** u32s per entry, per read. */
export const STRIDE = {
  CHAPTERS: 7,
  BLOCKS: 4,
  LINES: 4,
  NOTE_EXTENTS: 3,
  NOTE_PARTS: 4,
  TOKEN_SPANS: 3,
  TEXT_RUNS: 2,
  VERSE_ANCHORS: 5,
  DIAGNOSTICS: 7,
  FIXES: 2,
  FIX_EDITS: 2,
} as const;

/** "No such offset / no such index." */
export const NONE = 0xffffffff;

/** The coarse rendering class — the low three bits of a class word. */
export const CLASS = {
  OTHER: 0,
  PARA: 1,
  CHAR: 2,
  NOTE: 3,
  MILESTONE: 4,
  /** `\c`, `\v` and their alternates. `chapters()`/`verseAnchors()` tell them apart. */
  CHAPTER_VERSE: 5,
  SIDEBAR: 6,
  TABLE: 7,
} as const;

/**
 * The flag bits above the coarse class.
 *
 * Bits 0..7 are a byte's worth and have not moved; `META` is bit 8, so a class
 * is a 16-bit WORD. It rides in a whole u32 slot everywhere it appears.
 */
export const FLAG = {
  /** A title or section paragraph — `\mt`, `\ms`, `\s`, `\r`, `\d`, `\sp`, `\cd`, `\cl`. */
  HEADING: 1 << 3,
  /** Identification, introductions, peripherals, `\id`/`\usfm`. */
  FRONT: 1 << 4,
  /** Poetry and lists: the block classes an editor indents. */
  POETRY: 1 << 5,
  /** A closing spelling — `\x*`, `\*`, a `-e` milestone. */
  CLOSER: 1 << 6,
  /** The marker is in no table row (`\zaln`, `\s5`, a typo). Assume no shape. */
  UNKNOWN: 1 << 7,
  /**
   * MACHINE metadata, always alongside `FRONT`: `\id \usfm \ide \rem \sts
   * \h \toc1-3`. FRONT alone is introduction prose the reader sees
   * (`\ip \iot \is \imt`), so an editor dims the first and lays out the
   * second without a marker-name list.
   */
  META: 1 << 8,
} as const;

/** The class word occupies the low 16 bits of a packed field. */
export const CLASS_MASK = 0xffff;

/**
 * Flags packed into the `chapter`/`number` field of the two designator reads.
 *
 * A designator is POSITIONAL — the scanner returns whatever follows `\v` — so
 * once a number is deleted the next word becomes the designator. `NUMBER_SHAPED`
 * is the engine's verdict that the slot really is a number; CLEAR is the
 * conservative answer, and nothing should be styled as a verse number without it.
 */
export const ANCHOR = {
  NUMBER_SHAPED: 0x80000000,
  NUMBER_MASK: 0x7fffffff,
} as const;

/** What one `noteParts()` row describes. */
export const NOTE_PART = {
  /** The caller after `\f` — `+`, `-`, `?`. Delimiter trimmed. */
  CALLER: 0,
  /** Reader text inside `\fr`/`\xo` — the origin reference. */
  ORIGIN: 1,
  /** Reader text anywhere else in the note — the wording an apparatus edits. */
  BODY: 2,
  /**
   * Every marker token inside the note but its own opener: the chrome to
   * freeze. The marker plus ONE delimiter code unit — `\ft   note` freezes
   * `\ft ` and the two Pad spaces left over come back as BODY.
   */
  MARKUP: 3,
} as const;

/** The `\usfm` ladder, indexed by `AnalysisView.usfmVersion`. */
export const USFM_VERSIONS = ["3.0", "3.2", "4.0"] as const;

/** The token shapes, as `TokenKind::to_bits` packs them. */
export const TOKEN = {
  MARKER: 0,
  CLOSING_MARKER: 1,
  MILESTONE: 2,
  MILESTONE_TERMINATOR: 3,
  NEWLINE: 4,
  OPT_BREAK: 5,
  ATTR_LIST: 6,
  TEXT: 7,
  DESIGNATOR: 8,
  NOTE_CALLER: 9,
  BOOK_CODE: 10,
  /**
   * The reducible surplus of a delimiter run: every horizontal-whitespace
   * code unit past the ONE a chrome token keeps. Visible, editable bytes —
   * never hidden (no paint stands in for them), never content (text views
   * drop the kind whole; lint flags it, format deletes it).
   */
  PAD: 11,
} as const;

/**
 * Bit 4 of a token's kind byte. Its meaning is PER SHAPE: `\+` nesting on the
 * two marker shapes, the `-e` half on a milestone. No shape carries both.
 */
export const TOKEN_SPELLING_BIT = 1 << 4;

/** Where a token's kind byte sits in its packed field, above the class word. */
export const TOKEN_KIND_SHIFT = 16;

export const NOTE_FAMILY = {
  FOOTNOTE: 0,
  /** `\fe` */
  ENDNOTE: 1,
  /** `\ef` */
  EXTENDED_FOOTNOTE: 2,
  CROSS_REFERENCE: 3,
  /** `\ex` */
  EXTENDED_CROSS_REFERENCE: 4,
  OTHER: 5,
} as const;

// ---------------------------------------------------------------------------
// What a decoder yields
// ---------------------------------------------------------------------------

export interface Span {
  from: number;
  to: number;
}

export interface Chapter extends Span {
  /** The designator's number; 0 for front matter or a malformed `\c 12b`. */
  number: number;
  /** Is the label a NUMBER, or whatever happened to follow the marker? */
  numberShaped: boolean;
  /** The raw label's span (`12b`). Empty when the `\c` has no designator. */
  label: Span;
  /**
   * Where the `\c` marker starts — `null` on the front-matter row, which no
   * marker opens. `markerFrom..contentFrom` is the chrome an editor hides.
   */
  markerFrom: number | null;
  /**
   * Where the caret goes: past the designator's delimiter. `null` on row 0.
   * `label.to..contentFrom` is that delimiter — ONE code unit, or empty.
   */
  contentFrom: number | null;
}

/**
 * One MARKED line: a line whose first non-whitespace token is an opening
 * marker. Lines with no marker get no row at all.
 */
export interface Line extends Span {
  /** The packed class word — read it with `coarse`/`has`. */
  cls: number;
  /**
   * Past the marker AND its designator, plus ONE delimiter code unit:
   * `from..contentFrom` is chrome, and anything after it is the author's
   * leading whitespace — visible and editable, never hidden.
   */
  contentFrom: number;
}

export interface NotePart extends Span {
  /** Indexes the `notes()` read. */
  note: number;
  /** One of `NOTE_PART`. */
  kind: number;
}

export interface Block extends Span {
  /** The packed class word — read it with `coarse`/`has`. */
  cls: number;
  /**
   * The opening marker's name plus ONE delimiter code unit:
   * `from..contentFrom` is chrome — the marker TOKEN's own span, no re-split.
   * `\p    text` hides `\p ` and leaves three spaces of visible Pad.
   */
  contentFrom: number;
}

/**
 * One note's extent — the whole `\f … \f*`, closer included. An UNCLOSED note
 * recovers at its line end, and its extent stops IN FRONT OF the recovery
 * newline: no line break is ever inside an extent, so hiding or replacing one
 * whole never swallows document structure.
 */
export interface Note extends Span {
  /** One of `NOTE_FAMILY`. */
  family: number;
}

/**
 * One token, for the source pane's syntax styling.
 *
 * The spans TILE the document, and the ONE-delimiter rule is the lexer's own:
 * a chrome token (an opening marker, a milestone, a designator, a note
 * caller, a book code) carries its name plus at most ONE delimiter code unit,
 * and a delimiter run's surplus is a `TOKEN.PAD` span — visible, editable,
 * reducible. Hidden is therefore definable from this read alone: the
 * chrome-kind spans, minus anchor slots. `\p    text` styles `\p ` as chrome
 * and leaves three spaces of Pad.
 */
export interface TokenSpan extends Span {
  /** The packed class word. */
  cls: number;
  /** One of `TOKEN`, with `TOKEN_SPELLING_BIT` already stripped. */
  kind: number;
  /** `\+` nesting on a marker, the `-e` half on a milestone. */
  spelled: boolean;
}

/**
 * One `\v`, and the four offsets an editor hides or lands a caret on.
 *
 * `from`/`to` are the NUMBER's span. An absent designator reports it EMPTY at
 * `contentFrom` — the propped-open slot, which is where a retyped number lands.
 */
export interface VerseAnchor extends Span {
  /** The enclosing chapter's number. */
  chapter: number;
  /** Is the slot a NUMBER? Clear means "do not style this as a verse number". */
  numberShaped: boolean;
  /**
   * Where the `\v` marker starts. The leading CHROME is the marker token —
   * `\v` plus at most one delimiter code unit; whitespace surplus between it
   * and the number is a visible Pad token, so `markerFrom..from` is all
   * chrome only when no Pad sits inside it.
   */
  markerFrom: number;
  /**
   * Past the designator's delimiter — `to..contentFrom` is the trailing
   * chrome, and it is exactly ONE code unit, or empty when the line ends right
   * after the number. Never the whole whitespace run: a space the author types
   * at `contentFrom` stays theirs, visible, on the next analysis.
   */
  contentFrom: number;
}

export interface Diagnostic extends Span {
  /** Indexes `diagnostics.json` — name, severity, category, template, fix label. */
  code: number;
  /** The other party (an opener, an owner, a first occurrence), or null. */
  second: Span | null;
  /** The code's `aux` integer; what it MEANS is the side-table's `aux` column. */
  aux: number;
  /** The offered repair, or null. */
  fix: Fix | null;
}

export interface FixEdit extends Span {
  /** ASCII. Empty is a pure deletion; `from === to` is a pure insertion. */
  insert: string;
}

export interface Fix {
  edits: FixEdit[];
}

// ---------------------------------------------------------------------------
// Bit helpers
// ---------------------------------------------------------------------------

/** The coarse class of a class word — one of `CLASS`. */
export const coarse = (cls: number): number => cls & 0b111;

/** Is a flag set? `has(block.cls, FLAG.HEADING)`. */
export const has = (cls: number, flag: number): boolean => (cls & flag) !== 0;

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/**
 * What `analyze` returns: a plain object of typed arrays, built eagerly inside
 * the binary. Every key is always present; a read whose `wants` bit was clear
 * is an empty array.
 */
export interface RawAnalysis {
  readonly lenUtf16: number;
  /** The `\usfm` ladder index, or `NONE`. Decode with `declaredVersion`. */
  readonly usfmVersion: number;
  readonly chapters: Uint32Array;
  readonly blocks: Uint32Array;
  readonly lines: Uint32Array;
  readonly noteExtents: Uint32Array;
  readonly noteParts: Uint32Array;
  readonly tokenSpans: Uint32Array;
  readonly textRuns: Uint32Array;
  readonly verseAnchors: Uint32Array;
  readonly diagnostics: Uint32Array;
  readonly fixes: Uint32Array;
  readonly fixEdits: Uint32Array;
  readonly fixLens: Uint32Array;
  readonly fixText: string;
}

/**
 * Lazy decoders over one analysis.
 *
 * The arrays are already copies out of wasm memory. The decoders allocate
 * nothing until iterated, and yield plain objects one at a time —
 * `tokenSpans` on a large book is thousands of rows and must never be
 * materialised as an array unless the caller asks for one.
 */
export class AnalysisView {
  readonly lenUtf16: number;
  /** The `\usfm` ladder index, or `NONE`. See `declaredVersion`. */
  readonly usfmVersion: number;
  private readonly _chapters: Uint32Array;
  private readonly _blocks: Uint32Array;
  private readonly _lines: Uint32Array;
  private readonly _notes: Uint32Array;
  private readonly _noteParts: Uint32Array;
  private readonly _tokens: Uint32Array;
  private readonly _runs: Uint32Array;
  private readonly _verses: Uint32Array;
  private readonly _diagnostics: Uint32Array;
  private readonly _fixes: Uint32Array;
  private readonly _fixEdits: Uint32Array;
  private readonly _fixLens: Uint32Array;
  private readonly _fixText: string;
  /** Lazy prefix sums over `_fixLens` — where each edit's text starts. */
  private _textStarts: Uint32Array | null = null;
  /** Materialised once, on the first `blockAt` — see that method. */
  private _blockIndex: Block[] | null = null;

  constructor(raw: RawAnalysis) {
    this.lenUtf16 = raw.lenUtf16;
    this.usfmVersion = raw.usfmVersion;
    this._chapters = raw.chapters;
    this._blocks = raw.blocks;
    this._lines = raw.lines;
    this._notes = raw.noteExtents;
    this._noteParts = raw.noteParts;
    this._tokens = raw.tokenSpans;
    this._runs = raw.textRuns;
    this._verses = raw.verseAnchors;
    this._diagnostics = raw.diagnostics;
    this._fixes = raw.fixes;
    this._fixEdits = raw.fixEdits;
    this._fixLens = raw.fixLens;
    this._fixText = raw.fixText;
  }

  *chapters(): IterableIterator<Chapter> {
    const a = this._chapters;
    for (let at = 0; at < a.length; at += STRIDE.CHAPTERS) {
      const marker = a[at + 1];
      yield {
        number: a[at] & ANCHOR.NUMBER_MASK,
        numberShaped: (a[at] & ANCHOR.NUMBER_SHAPED) !== 0,
        markerFrom: marker === NONE ? null : marker,
        label: { from: a[at + 2], to: a[at + 3] },
        contentFrom: a[at + 4] === NONE ? null : a[at + 4],
        from: a[at + 5],
        to: a[at + 6],
      };
    }
  }

  /** One row per marked line, in document order. */
  *lines(): IterableIterator<Line> {
    const a = this._lines;
    for (let at = 0; at < a.length; at += STRIDE.LINES) {
      yield { cls: a[at], from: a[at + 1], contentFrom: a[at + 2], to: a[at + 3] };
    }
  }

  *blocks(): IterableIterator<Block> {
    const a = this._blocks;
    for (let at = 0; at < a.length; at += STRIDE.BLOCKS) {
      yield { cls: a[at], from: a[at + 1], contentFrom: a[at + 2], to: a[at + 3] };
    }
  }

  *notes(): IterableIterator<Note> {
    const a = this._notes;
    for (let at = 0; at < a.length; at += STRIDE.NOTE_EXTENTS) {
      yield { family: a[at], from: a[at + 1], to: a[at + 2] };
    }
  }

  *tokens(): IterableIterator<TokenSpan> {
    const a = this._tokens;
    for (let at = 0; at < a.length; at += STRIDE.TOKEN_SPANS) {
      const packed = a[at];
      const kindBits = (packed >>> TOKEN_KIND_SHIFT) & 0xff;
      yield {
        cls: packed & CLASS_MASK,
        kind: kindBits & ~TOKEN_SPELLING_BIT,
        spelled: (kindBits & TOKEN_SPELLING_BIT) !== 0,
        from: a[at + 1],
        to: a[at + 2],
      };
    }
  }

  /**
   * Every token, WITHOUT allocating one object per row.
   *
   * `tokens()` yields a `TokenSpan` each time, which is right for a handful of
   * rows and wrong for a book: a 113KB book is ~6,000 tokens, and materialising
   * them costs more than the whole wasm call. A consumer scanning the read for
   * a couple of shapes (aligned words, milestone pips) takes this instead, and
   * pays nothing per token it does not want.
   *
   * `cls` is the packed class word; `kindBits` still carries
   * `TOKEN_SPELLING_BIT`. Return `false` to stop early.
   */
  forEachToken(fn: (kindBits: number, cls: number, from: number, to: number) => void | boolean): void {
    const a = this._tokens;
    for (let at = 0; at < a.length; at += STRIDE.TOKEN_SPANS) {
      const packed = a[at];
      if (
        fn((packed >>> TOKEN_KIND_SHIFT) & 0xff, packed & CLASS_MASK, a[at + 1], a[at + 2]) === false
      )
        return;
    }
  }

  *textRuns(): IterableIterator<Span> {
    const a = this._runs;
    for (let at = 0; at < a.length; at += STRIDE.TEXT_RUNS) {
      yield { from: a[at], to: a[at + 1] };
    }
  }

  *verseAnchors(): IterableIterator<VerseAnchor> {
    const a = this._verses;
    for (let at = 0; at < a.length; at += STRIDE.VERSE_ANCHORS) {
      yield {
        chapter: a[at] & ANCHOR.NUMBER_MASK,
        numberShaped: (a[at] & ANCHOR.NUMBER_SHAPED) !== 0,
        markerFrom: a[at + 1],
        from: a[at + 2],
        to: a[at + 3],
        contentFrom: a[at + 4],
      };
    }
  }

  /** Every note's interior, in document order. `partsOf` filters to one note. */
  *noteParts(): IterableIterator<NotePart> {
    const a = this._noteParts;
    for (let at = 0; at < a.length; at += STRIDE.NOTE_PARTS) {
      yield { note: a[at], kind: a[at + 1], from: a[at + 2], to: a[at + 3] };
    }
  }

  /** One note's parts — the rows are grouped, so this stops at the group's end. */
  *partsOf(note: number): IterableIterator<NotePart> {
    for (const part of this.noteParts()) {
      if (part.note > note) return;
      if (part.note === note) yield part;
    }
  }

  *diagnostics(): IterableIterator<Diagnostic> {
    const a = this._diagnostics;
    for (let at = 0; at < a.length; at += STRIDE.DIAGNOSTICS) {
      const second = a[at + 3];
      const fix = a[at + 6];
      yield {
        code: a[at],
        from: a[at + 1],
        to: a[at + 2],
        second: second === NONE ? null : { from: second, to: a[at + 4] },
        aux: a[at + 5],
        fix: fix === NONE ? null : this.fix(fix),
      };
    }
  }

  /**
   * The blob is concatenated in edit order, so edit `n`'s text starts at the
   * sum of the lengths before it. Computed once, on the first fix asked for —
   * most documents ask for none.
   */
  private textStarts(): Uint32Array {
    if (this._textStarts === null) {
      const starts = new Uint32Array(this._fixLens.length + 1);
      for (let edit = 0; edit < this._fixLens.length; edit++) {
        starts[edit + 1] = starts[edit] + this._fixLens[edit];
      }
      this._textStarts = starts;
    }
    return this._textStarts;
  }

  /** One offered repair, by the index a diagnostic carries. */
  fix(index: number): Fix {
    const at = index * STRIDE.FIXES;
    const starts = this.textStarts();
    const edits: FixEdit[] = [];
    for (let edit = this._fixes[at]; edit < this._fixes[at + 1]; edit++) {
      edits.push({
        from: this._fixEdits[edit * STRIDE.FIX_EDITS],
        to: this._fixEdits[edit * STRIDE.FIX_EDITS + 1],
        insert: this._fixText.slice(starts[edit], starts[edit + 1]),
      });
    }
    return { edits };
  }

  /**
   * The block containing a position, or null — the `blockAt` a CM probe wants.
   *
   * A caret move asks this on every keystroke, so the rows are materialised
   * ONCE (on the first call) and every later call is a binary search. Blocks
   * nest — a `\p` inside an `\esb` — so the INNERMOST containing block wins:
   * the last one that starts at or before `pos` and still contains it.
   */
  blockAt(pos: number): Block | null {
    if (this._blockIndex === null) this._blockIndex = [...this.blocks()];
    const rows = this._blockIndex;
    // The last row whose `from <= pos`; blocks are emitted in `from` order.
    let lo = 0;
    let hi = rows.length - 1;
    let at = -1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (rows[mid].from <= pos) {
        at = mid;
        lo = mid + 1;
      } else hi = mid - 1;
    }
    // Walk back over rows that start before `pos` but end before it too — a
    // sibling that closed already, where the enclosing sidebar has not.
    for (; at >= 0; at--) if (pos <= rows[at].to) return rows[at];
    return null;
  }
}

/**
 * The decoders over one `analyze` result.
 *
 *   const a = analysis(analyze(text, wants));
 *
 * The cast is here and nowhere else: the binary types this return as `Object`
 * (the shape's one home is `RawAnalysis`, above, so it is not restated in the
 * generated `.d.ts`), and every call site gets a typed view without writing
 * one itself. Nothing is retained wasm-side, so there is nothing to free.
 */
export const analysis = (raw: unknown): AnalysisView =>
  new AnalysisView(raw as RawAnalysis);

/**
 * The declared `\usfm` version as the string `severityAt` wants, or `null`.
 *
 * The engine reports it, so nothing re-reads the document's first bytes.
 */
export const declaredVersion = (a: { usfmVersion: number }): string | null =>
  a.usfmVersion === NONE ? null : (USFM_VERSIONS[a.usfmVersion] ?? null);

// ---------------------------------------------------------------------------
// The write path: format edits
// ---------------------------------------------------------------------------

/**
 * A wasm `Edits` handle — what `formatEdits` and `formatEditsIn` return. Same
 * wire as a fix: `[from, to]` per edit in UTF-16, one concatenated ASCII insert
 * blob, one byte length per edit.
 *
 * `formatEditsIn(text, from, to, opts)` is the SCOPED transaction — the same
 * whole-book analysis, filtered to a UTF-16 window (a chapter's span out of the
 * `chapters` read). Do not filter an edit list in JS instead: the engine drops a
 * boundary-STRADDLING edit whole rather than cutting it, keeps a multi-edit
 * claim only if all of it is inside, and counts a pure insertion sitting ON
 * either edge as inside. A JS `filter` cannot see the claim groups, so it will
 * happily keep half of a `bridge-empty-verses` pair — half an edit corrupts.
 */
export interface RawEdits {
  readonly spans: Uint32Array;
  readonly lens: Uint32Array;
  readonly text: string;
}

export type OwnedEdits = RawEdits & { free(): void };

/**
 * The edit list, decoded. Same `{from, to, insert}` a fix yields, so an editor
 * applies both through one code path.
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
      from: raw.spans[edit * STRIDE.FIX_EDITS],
      to: raw.spans[edit * STRIDE.FIX_EDITS + 1],
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

// ---------------------------------------------------------------------------
// The diagnostics side-table
// ---------------------------------------------------------------------------

/** One entry of `diagnostics.json`, which is codegen'd from the lint rows. */
export interface DiagnosticCode {
  code: number;
  /** The durable identity, kebab-case. */
  name: string;
  category: "structure" | "ordering" | "attributes" | "payload" | "form" | "version";
  /**
   * CodeMirror's own ladder, plus `form` (the formatter's channel, which never
   * appears in a diagnostics read). `null` is the GATE: the code says nothing
   * until the document declares a `\usfm` version at or above its first rung.
   */
  severity: "error" | "warning" | "info" | "hint" | "form" | null;
  /** `[version, severity]`, ascending. The last rung at or below the declared version wins. */
  escalation: [string, string][];
  /** What the finding's `aux` integer means for this code. */
  aux:
    | "none"
    | "expectedNumber"
    | "numberingCap"
    | "count"
    | "version"
    | "malformedShape";
  /** `{anchor}`/`{second}` stand for the marker text at those spans. */
  template: string;
  /** The fix is also a formatting action. */
  formatter: boolean;
  fixLabel: string | null;
}

export interface DiagnosticCatalog {
  schema: string;
  codes: DiagnosticCode[];
}

/**
 * The message, rendered from the template and the document's own bytes.
 *
 * `slice` is the editor's `doc.sliceString`. Nothing crosses the wasm wall to
 * make this string.
 */
export function message(
  catalog: DiagnosticCatalog,
  finding: Diagnostic,
  slice: (from: number, to: number) => string,
): string {
  const template = catalog.codes[finding.code].template;
  return template
    .replace("{anchor}", slice(finding.from, finding.to))
    .replace(
      "{second}",
      finding.second ? slice(finding.second.from, finding.second.to) : "",
    );
}

/**
 * The severity a code carries in a document declaring `version` (`null` when it
 * declares none) — the `severity_at` ladder, JS side.
 */
export function severityAt(
  entry: DiagnosticCode,
  version: string | null,
): DiagnosticCode["severity"] {
  let severity = entry.severity;
  if (version === null) return severity;
  for (const [rung, at] of entry.escalation) {
    if (rung <= version) severity = at as DiagnosticCode["severity"];
  }
  return severity;
}
