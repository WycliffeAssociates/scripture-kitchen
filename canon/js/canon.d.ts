import { BOOKS, NAME_LOCALES, type Book, type Division } from "./tables.ts";
export { BOOKS, NAME_LOCALES, type Book, type Division };
export { SCHEME_NAMES, chapters, excluded, isSchemeName, lastVerse, verseCounts, type SchemeName, } from "./versification.ts";
/** Which books a list holds; every list is in the spec's row order. */
export type Canon = "protestant66" | "withDeuterocanon" | "all";
/** The row for a code, exactly (`MAT`, not `mat`: `parseBook` folds). */
export declare const book: (code: string) => Book | undefined;
/** The books of one canon, in order. */
export declare const books: (canon: Canon) => readonly Book[];
/** Which of a book's names: `\toc1`, `\toc2` or `\toc3`. */
export type NameForm = "long" | "short" | "abbrev";
/** One gateway language's reviewed names: `[long, short, abbrev]`, "" for none. */
export interface NameTable {
    readonly locale: string;
    readonly books: Readonly<Record<string, readonly [string, string, string]>>;
}
/** A name, with the locale and form that answered. */
export interface Named {
    readonly text: string;
    readonly locale: string;
    readonly form: NameForm;
}
/**
 * The names module a BCP 47 tag means, of those canon holds (`NAME_LOCALES`),
 * or `undefined`: the same tag in any case; else a held locale of the same
 * language whose script, if it names one, the tag names too. A
 * macrolanguage reads as the language canon holds for it. Rust's
 * `canon::locale_for`; `fixtures/names.json` holds both to one rule.
 *
 *     localeFor("es")        // "es-419", then import(`…/names/es-419`)
 *     localeFor("ar-EG")     // "arb"
 *     localeFor("ur")        // undefined: ur-deva is Devanagari
 */
export declare const localeFor: (tag: string) => string | undefined;
/**
 * The code a word names, or `undefined`: a book code in any case first, then
 * each given locale's names (any form) in order, then an English name or
 * alias, all after `fold`. Exact only; prefix matching is a navigation box's
 * policy. A code wins, so "Jon" is Jonah, even in Tok Pisin.
 */
export declare const parseBook: (text: string, tables?: readonly NameTable[]) => string | undefined;
/**
 * What a book is called: each table in order, then English (en_ulb's own
 * names, then the book table's, which also covers the books en_ulb lacks).
 * Within a table, "short" falls back to "long"; "abbrev" stands in for
 * nothing. Rust's `canon::name` is this with locale tags for tables.
 */
export declare const name: (code: string, form: NameForm, tables?: readonly NameTable[]) => Named | undefined;
/** `41-MAT.usfm`, or `undefined` for a code outside the table. */
export declare const usfmFileName: (code: string) => string | undefined;
/** The code in lowercase, as Writer project directories spell it (`mat`). */
export declare const slug: (code: string) => string;
/**
 * A name as `parseBook` compares it: NFKD with the combining accents
 * (U+0300..U+036F) dropped, trimmed, lowercased, whitespace runs collapsed to
 * one space, and a leading digit separated from the word after it, so
 * "Éxodo" is "exodo". Sefer's `names.ts` folds the same way. Must equal
 * Rust's `canon::fold` on every fixture.
 */
export declare const fold: (text: string) => string;
