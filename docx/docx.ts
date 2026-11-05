/**
 * docx: USFM in Word and LibreOffice. A book exports as a .docx whose named
 * styles are its markers ("USFM p", "USFM q1", "USFM v", real footnotes), and
 * any .docx imports back. TypeScript over one wasm module; every rule lives in
 * the Rust (`docx/src`), once.
 *
 *     const docx = await import("@wycliffeassociates/scripture-kitchen/docx");
 *     await docx.init();                                     // or initSync({ module: bytes })
 *
 *     const out = docx.exportBook(usfm);                     // { name: "PSA.docx", docx, book, unsupported }
 *     const all = docx.exportBooks([gen, exo], { rtl: true }); // one .docx per book, by file name
 *     const back = docx.importDocx(bytes);                   // { books, report, properties }
 *     back.books[0].usfm;  back.books[0].comments;           // threads as @sefer/document-import's
 *     back.report.unknownParagraphStyles;                    // what was not in our styles
 *
 * A paragraph or run in a `USFM …` style is read as that marker, with no
 * guessing. A paragraph in any other style becomes `\p`, a run in any other
 * style is unwrapped, and each is counted in the report. Tracked changes are
 * taken as accepted and counted.
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

export interface ImportReport {
  /** Paragraph styles that are not ours, by name, with how many paragraphs used each. Each became `\p`. */
  unknownParagraphStyles: Record<string, number>;
  /** Character styles that are not ours. Their text was kept, unwrapped. */
  unknownCharacterStyles: Record<string, number>;
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

/** Throws when the bytes are not a .docx (not a zip, or no document part). */
export function importDocx(bytes: Uint8Array): Imported {
  return JSON.parse(raw.importDocx(bytes)) as Imported;
}

/** Nothing was guessed: every paragraph and run was in one of our styles. */
export function isExact(report: ImportReport): boolean {
  return (
    Object.keys(report.unknownParagraphStyles).length === 0 &&
    Object.keys(report.unknownCharacterStyles).length === 0 &&
    report.missingNotes === 0
  );
}
