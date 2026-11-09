/**
 * Versification schemes for TypeScript: how many chapters a book has and how
 * many verses each chapter has, under one numbering tradition. No wasm.
 *
 *     chapters("eng", "MAL")          // 4  (Malachi 4 is org's 3:19-24)
 *     chapters("org", "MAL")          // 3
 *     lastVerse("org", "PSA", 3)      // 9  (org numbers the title as verse 1)
 *     lastVerse("ulb", "ACT", 19)     // 40 (eng says 41)
 *     chapters("ulb", "TOB")          // undefined: ulb has no Tobit
 *
 * The data is `scheme-tables.ts`, generated from `canon/data/schemes/*.json`
 * by the run that writes the Rust tables. These lookups match
 * `canon/src/scheme.rs`, and both pass `canon/fixtures/schemes.json`.
 */
import { EXCLUDED, SCHEME_NAMES, VERSE_COUNTS } from "./scheme-tables.js";
export { SCHEME_NAMES };
/** Whether a string names a built-in scheme, exactly (`"org"`, not `"ORG"`). */
export const isSchemeName = (name) => SCHEME_NAMES.includes(name);
/**
 * The last verse of every chapter, chapter 1 first, or `undefined` when the
 * scheme does not number the book.
 */
export const verseCounts = (scheme, book) => VERSE_COUNTS[scheme].get(book);
/** How many chapters the book has, or `undefined` when the scheme lacks it. */
export const chapters = (scheme, book) => verseCounts(scheme, book)?.length;
/**
 * The last verse of a chapter, or `undefined` for a book the scheme does not
 * number, chapter 0, or a chapter past the book's last.
 */
export const lastVerse = (scheme, book, chapter) => Number.isInteger(chapter) && chapter >= 1 ? verseCounts(scheme, book)?.[chapter - 1] : undefined;
/**
 * Whether the scheme omits this verse on purpose. No built-in scheme excludes
 * a verse today; the question is here for declared schemes.
 */
export const excluded = (scheme, book, chapter, verse) => EXCLUDED[scheme].has(`${book} ${chapter}:${verse}`);
