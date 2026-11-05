/**
 * docx: USFM in Word and LibreOffice. A book exports as a .docx whose named
 * styles are its markers ("USFM p", "USFM q1", "USFM v", real footnotes), and
 * any .docx or .odt imports back. TypeScript over one wasm module; every rule
 * lives in the Rust (`docx/src`), once.
 *
 *     const docx = await import("@wycliffeassociates/scripture-kitchen/docx");
 *     await docx.init();                                     // or initSync({ module: bytes })
 *
 *     const out = docx.exportBook(usfm);                     // { name: "PSA.docx", docx, book, unsupported }
 *     const all = docx.exportBooks([gen, exo], { rtl: true }); // one .docx per book, by file name
 *     const dotx = docx.template();                          // every style and a starter book
 *
 *     const reading = docx.readDocument(bytes);              // once per file
 *     const back = reading.render({ book: "TIT" });          // rerun on every settings change
 *     back.books[0].usfm;  back.books[0].comments;           // threads as @sefer/document-import's
 *     back.report.exact;  back.report.guesses;               // nothing guessed? and every guess, with its reason
 *
 * A paragraph in one of our `USFM …` styles is read as that marker, with no
 * guessing. Any other paragraph is read by `@sefer/document-import`'s rules,
 * the same settings and the same report: verse numbers by how the document
 * writes them, poetry by indent, headings by look. One file may mix the two.
 * Tracked changes are taken as accepted and counted.
 *
 * A large book takes tens of milliseconds each way; call it from a Worker
 * where that would stall a frame.
 */
import wasmInit, * as raw from "./pkg-web/docx.js";

/** Start the module: from its URL (the default), a `Response`, or its bytes. */
export const init = (input?: Parameters<typeof wasmInit>[0]) => wasmInit(input);
/** Start the module from bytes already in hand (`{ module: bytes }`). */
export const initSync = raw.initSync;

export type BookCode = string;

export interface ExportOptions {
  /** Right-to-left paragraphs and runs, for Arabic or Hebrew script. */
  rtl?: boolean;
  /** The document's default font. */
  font?: string;
}

/** Something the document could not carry: an attribute, a figure, `\ca`. */
export interface Unsupported {
  /** The verse or chapter it follows (`GEN 1:3`), or the book code. */
  at: string;
  what: string;
}

export interface Exported {
  /** `GEN.docx`; `book.docx` when the USFM has no `\id`. */
  name: string;
  docx: Uint8Array;
  book?: BookCode;
  /** Empty when the round trip gives back the same book. */
  unsupported: Unsupported[];
}

export function exportBook(usfm: string, options: ExportOptions = {}): Exported {
  const out = raw.exportBook(usfm, options.rtl ?? false, options.font) as { docx: Uint8Array; json: string };
  const meta = JSON.parse(out.json) as { book: BookCode | null; unsupported: Unsupported[] };
  const book = meta.book ?? undefined;
  return { name: `${book ?? "book"}.docx`, docx: out.docx, book, unsupported: meta.unsupported };
}

/** One .docx per book, by file name. Zipping them for a download is the caller's. */
export function exportBooks(
  usfms: string[],
  options: ExportOptions = {},
): { files: Map<string, Uint8Array>; unsupported: Map<string, Unsupported[]> } {
  const files = new Map<string, Uint8Array>();
  const unsupported = new Map<string, Unsupported[]>();
  for (const usfm of usfms) {
    const out = exportBook(usfm, options);
    files.set(out.name, out.docx);
    if (out.unsupported.length > 0) unsupported.set(out.name, out.unsupported);
  }
  return { files, unsupported };
}

/** The exact USFM a book was exported from, when the file says. */
export interface Source {
  /** Git's object id for the USFM's bytes: findable in any repository holding them. */
  gitBlobSha1: string;
  /** onion's dish-header hash of the same bytes, 16 hex digits. */
  xxh3_64: string;
}

/** A position in one verse's plain text (no markers), in UTF-16 code units. */
export interface VersePoint {
  /** `chapter:verse`, the verse as `\v` spells it. */
  ref: string;
  offset: number;
}

export interface Comment {
  id: string;
  author: string;
  /** Null when Word stripped personal information from the file. */
  date: string | null;
  text: string;
  parentId: string | null;
  resolved: boolean;
}

/** As `@sefer/document-import` returns it. */
export interface CommentThread {
  ref: string | null;
  anchorText: string;
  start: VersePoint | null;
  end: VersePoint | null;
  /** Root first, then replies in document order. */
  comments: Comment[];
}

export interface ImportedBook {
  code: BookCode | null;
  usfm: string;
  source: Source | null;
  /** Threads anchored in this book; the first book also takes any anchored nowhere. */
  comments: CommentThread[];
}

export type Keep = "keep" | "unwrap";
export type VerseMode = "superscript" | "marked" | "lineStart" | "perLine" | "any";

/** How a paragraph in no style of ours is read. `@sefer/document-import`'s settings, but for `versification`. */
export interface Settings {
  /** Book code for the `\id` line when the document has none of its own. */
  book?: string;
  /** How verse numbers are written; "auto" (the default) detects it. */
  verses?: VerseMode | "auto";
  /** Words that label a chapter, as in "Chapter 3". A label without a number is the next chapter. */
  chapterWords?: string[];
  /** A canon versification scheme ("eng", "org", "ulb"): a number past the chapter's last verse is never a verse. Needs a book code. */
  versification?: string;
  /** Indented, italic, Quote-styled or short-lined paragraphs: \q1–\q3, or \p. */
  poetry?: Keep;
  /** Heading paragraphs: \s1, \rem, or left out. */
  headings?: "keep" | "rem" | "drop";
  /** Text before the first verse: \rem, \mt, or left out. */
  front?: "rem" | "mt" | "drop";
  notes?: "keep" | "drop";
  italic?: Keep;
  bold?: Keep;
  /** Numbered and bulleted paragraphs: \li1, or \p. */
  lists?: Keep;
  /** Paragraph styles named after a USFM marker ("q2", "s1 - Heading"): that marker, or ignored. */
  styleMarkers?: Keep;
  comments?: "keep" | "drop";
  /** Renames guessed paragraph markers last, e.g. { q2: "q1", li1: "p" }. */
  markers?: Record<string, string>;
}

export interface ChapterSummary {
  chapter: number;
  verses: number;
  last: number;
  missing: number[];
}

export interface Found {
  poetry: number;
  headings: number;
  front: number;
  notes: number;
  italic: number;
  bold: number;
  lists: number;
  tables: number;
  styleMarkers: number;
  comments: number;
  trackedChanges: number;
  lineBreaks: number;
  blankParagraphs: number;
}

export interface Skip {
  ref: string | null;
  number: string;
  reason: "sequence" | "versification" | "thousands";
}

export interface Guess {
  ref: string | null;
  marker: string;
  reason: string;
  excerpt: string;
}

/** `@sefer/document-import`'s report, with what the exact path adds. */
export interface ImportReport {
  /** "usfm" when the document already held USFM markers, copied through as text. */
  source: "document" | "usfm";
  verses: VerseMode;
  detected: VerseMode;
  /** How many numbers of each kind the guessed paragraphs hold (our own verse numbers count as superscript). */
  candidates: Record<"superscript" | "alone" | "marked" | "lineStart" | "inline", number>;
  chapters: ChapterSummary[];
  /** What the document holds, whatever the settings do with it: the switches a host offers. */
  found: Found;
  /** Numbers written like verse numbers that were left as text, and why. */
  skipped: Skip[];
  /** Every guess, with the reason. */
  guesses: Guess[];
  /** Nothing was guessed: every paragraph and run was in one of our styles. */
  exact: boolean;
  /** Paragraphs in no style of ours, by style name: read by the best-effort rules. */
  guessedParagraphs: Record<string, number>;
  /** Character styles that are not ours, inside our paragraphs. Their text was kept, unwrapped. */
  unknownCharacterStyles: Record<string, number>;
  /** Notes whose paragraph style is not ours; each became \f or \fe. */
  unknownNoteStyles: Record<string, number>;
  /** Insertions, deletions and moves still tracked; all taken as accepted. */
  trackedChanges: number;
  /** Note references whose note was not in the file. */
  missingNotes: number;
}

export interface Imported {
  /** One per `USFM id` paragraph; text before the first is a book of its own. */
  books: ImportedBook[];
  report: ImportReport;
  /** Every custom document property, as [name, value]. */
  properties: [string, string][];
}

export interface DocumentReading {
  render(settings?: Settings): Imported;
  /** Frees the reading's memory in the module; it cannot render after. */
  free(): void;
}

/** Reads a .docx or .odt once. Throws when the bytes are neither. */
export function readDocument(bytes: Uint8Array): DocumentReading {
  const reading = raw.readDocument(bytes);
  return {
    render: (settings: Settings = {}) => JSON.parse(reading.render(JSON.stringify(settings))) as Imported,
    free: () => reading.free(),
  };
}

/** Reads and renders once, with these settings. */
export function importDocument(bytes: Uint8Array, settings: Settings = {}): Imported {
  const reading = raw.readDocument(bytes);
  try {
    return JSON.parse(reading.render(JSON.stringify(settings))) as Imported;
  } finally {
    reading.free();
  }
}

/** The template (.dotx): every style and a starter book, for writing USFM in Word from scratch. */
export function template(): Uint8Array {
  return raw.template();
}
