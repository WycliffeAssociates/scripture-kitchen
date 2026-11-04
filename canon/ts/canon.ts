/**
 * canon for TypeScript: the book table and its lookups, with no wasm.
 *
 *     import { book, books, parseBook, usfmFileName } from "@wycliffeassociates/scripture-kitchen/canon";
 *
 *     book("MAT")?.canonicalIndex    // 40
 *     parseBook("1samuel")           // "1SA"
 *     books("protestant66").length   // 66
 *     usfmFileName("MAT")            // "41-MAT.usfm"
 *
 * The data is `tables.ts`, generated from `canon/data/books.tsv` by the same
 * run that writes the Rust tables. These lookups are hand-written to match
 * `canon/src/books.rs`, and both pass `canon/fixtures/books.json`
 * (`canon.test.mjs` here, `tests/fixtures.rs` there). Change behaviour in the
 * fixtures first.
 */
import { BOOKS, NAMES, type Book, type Division } from "./tables.ts";

export { BOOKS, type Book, type Division };

/** Which books a list holds; every list is in the spec's row order. */
export type Canon = "protestant66" | "withDeuterocanon" | "all";

const BY_CODE: ReadonlyMap<string, Book> = new Map(BOOKS.map((row) => [row.code, row]));

const HOLDS: Readonly<Record<Canon, (division: Division) => boolean>> = {
  protestant66: (d) => d === "ot" || d === "nt",
  withDeuterocanon: (d) => d === "ot" || d === "nt" || d === "deuterocanon",
  all: () => true,
};

/** The row for a code, exactly (`MAT`, not `mat`: `parseBook` folds). */
export const book = (code: string): Book | undefined => BY_CODE.get(code);

/** The books of one canon, in order. */
export const books = (canon: Canon): readonly Book[] =>
  BOOKS.filter((row) => HOLDS[canon](row.division));

/**
 * The code a word names, or `undefined`: a book code in any case first, then
 * an English name or alias after `fold`. Exact only; prefix matching is a
 * navigation box's policy. A code wins, so "Jon" is Jonah.
 */
export const parseBook = (text: string): string | undefined => {
  const trimmed = text.trim();
  if (trimmed.length === 3) {
    const upper = trimmed.replace(/[a-z]/g, (c) => c.toUpperCase());
    if (BY_CODE.has(upper)) return upper;
  }
  return NAMES.get(fold(trimmed));
};

/** `41-MAT.usfm`, or `undefined` for a code outside the table. */
export const usfmFileName = (code: string): string | undefined => {
  const row = BY_CODE.get(code);
  return row === undefined ? undefined : `${row.usfmNumber}-${row.code}.usfm`;
};

/** The code in lowercase, as Writer project directories spell it (`mat`). */
export const slug = (code: string): string => code.replace(/[A-Z]/g, (c) => c.toLowerCase());

/**
 * A name as `parseBook` compares it: trimmed, lowercased, whitespace runs
 * collapsed to one space, and a leading digit separated from the word after
 * it. Must equal Rust's `canon::fold` on every fixture.
 */
export const fold = (text: string): string => {
  const out = text.trim().split(/\s+/u).filter((word) => word !== "").join(" ").toLowerCase();
  const first = out.charAt(0);
  const next = out.charAt(1);
  return /^[0-9]$/.test(first) && next !== "" && next !== " " && !/^[0-9]$/.test(next)
    ? `${first} ${out.slice(1)}`
    : out;
};
