/**
 * canon for TypeScript: the book table, versification schemes' counts, and
 * their lookups, with no wasm. The schemes are in `versification.ts`.
 *
 *     import { book, books, parseBook, usfmFileName } from "@wycliffeassociates/scripture-kitchen/canon";
 *
 *     book("MAT")?.canonicalIndex    // 40
 *     parseBook("1samuel")           // "1SA"
 *     books("protestant66").length   // 66
 *     usfmFileName("MAT")            // "41-MAT.usfm"
 *
 * Book names in a gateway language are one module per locale, so a bundle
 * carries only the languages it imports (`NAME_LOCALES` lists them):
 *
 *     import fr from "@wycliffeassociates/scripture-kitchen/names/fr";
 *
 *     name("MAT", "long", [fr])      // { text: "Matthieu", locale: "fr", form: "long" }
 *     parseBook("matthieu", [fr])    // "MAT"
 *
 * The data is `tables.ts`, generated from `canon/data/books.tsv` by the same
 * run that writes the Rust tables. These lookups are hand-written to match
 * `canon/src/books.rs`, and both pass `canon/fixtures/books.json`
 * (`canon.test.mjs` here, `tests/fixtures.rs` there). Change behaviour in the
 * fixtures first.
 */
import english from "./names/en.js";
import { BOOKS, NAMES, NAME_LOCALES } from "./tables.js";
export { BOOKS, NAME_LOCALES };
export { SCHEME_NAMES, chapters, excluded, isSchemeName, lastVerse, verseCounts, } from "./versification.js";
const BY_CODE = new Map(BOOKS.map((row) => [row.code, row]));
const HOLDS = {
    protestant66: (d) => d === "ot" || d === "nt",
    withDeuterocanon: (d) => d === "ot" || d === "nt" || d === "deuterocanon",
    all: () => true,
};
/** The row for a code, exactly (`MAT`, not `mat`: `parseBook` folds). */
export const book = (code) => BY_CODE.get(code);
/** The books of one canon, in order. */
export const books = (canon) => BOOKS.filter((row) => HOLDS[canon](row.division));
const FORMS = ["long", "short", "abbrev"];
/** Each table's names folded, to the code: built once per table. */
const INDEXES = new WeakMap();
const indexOf = (table) => {
    let index = INDEXES.get(table);
    if (index === undefined) {
        const built = new Map();
        for (const [code, names] of Object.entries(table.books)) {
            for (const n of names) {
                if (n !== "")
                    built.set(fold(n), code);
            }
        }
        index = built;
        INDEXES.set(table, index);
    }
    return index;
};
/** A tag's language, macrolanguage aliases folded, and its script, lowercased. */
const subtags = (tag) => {
    const [first = "", ...rest] = tag.split(/[-_]/);
    const language = first.toLowerCase();
    const alias = { ar: "arb", mg: "plt", fil: "tl" };
    const script = rest.find((part) => /^[A-Za-z]{4}$/.test(part))?.toLowerCase();
    return [alias[language] ?? language, script];
};
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
export const localeFor = (tag) => {
    const exact = NAME_LOCALES.find((held) => held.toLowerCase() === tag.toLowerCase());
    if (exact !== undefined)
        return exact;
    const [language, script] = subtags(tag);
    if (language === "")
        return undefined;
    return NAME_LOCALES.find((held) => {
        const [heldLanguage, heldScript] = subtags(held);
        return heldLanguage === language && (heldScript === undefined || heldScript === script);
    });
};
/**
 * The code a word names, or `undefined`: a book code in any case first, then
 * each given locale's names (any form) in order, then an English name or
 * alias, all after `fold`. Exact only; prefix matching is a navigation box's
 * policy. A code wins, so "Jon" is Jonah, even in Tok Pisin.
 */
export const parseBook = (text, tables = []) => {
    const trimmed = text.trim();
    if (trimmed.length === 3) {
        const upper = trimmed.replace(/[a-z]/g, (c) => c.toUpperCase());
        if (BY_CODE.has(upper))
            return upper;
    }
    const folded = fold(trimmed);
    for (const table of tables) {
        const code = indexOf(table).get(folded);
        if (code !== undefined)
            return code;
    }
    return NAMES.get(folded);
};
/**
 * What a book is called: each table in order, then English (en_ulb's own
 * names, then the book table's, which also covers the books en_ulb lacks).
 * Within a table, "short" falls back to "long"; "abbrev" stands in for
 * nothing. Rust's `canon::name` is this with locale tags for tables.
 */
export const name = (code, form, tables = []) => {
    const row = BY_CODE.get(code);
    if (row === undefined)
        return undefined;
    for (const table of [...tables, english]) {
        const names = table.books[code];
        if (names === undefined)
            continue;
        const at = FORMS.indexOf(form);
        if (names[at] !== "")
            return { text: names[at], locale: table.locale, form };
        if (form === "short" && names[0] !== "") {
            return { text: names[0], locale: table.locale, form: "long" };
        }
    }
    return form === "abbrev" ? undefined : { text: row.english, locale: "en", form };
};
/** `41-MAT.usfm`, or `undefined` for a code outside the table. */
export const usfmFileName = (code) => {
    const row = BY_CODE.get(code);
    return row === undefined ? undefined : `${row.usfmNumber}-${row.code}.usfm`;
};
/** The code in lowercase, as Writer project directories spell it (`mat`). */
export const slug = (code) => code.replace(/[A-Z]/g, (c) => c.toLowerCase());
/**
 * A name as `parseBook` compares it: NFKD with the combining accents
 * (U+0300..U+036F) dropped, trimmed, lowercased, whitespace runs collapsed to
 * one space, and a leading digit separated from the word after it, so
 * "Éxodo" is "exodo". Sefer's `names.ts` folds the same way. Must equal
 * Rust's `canon::fold` on every fixture.
 */
export const fold = (text) => {
    const out = text
        .normalize("NFKD")
        .replace(/[\u0300-\u036f]/gu, "")
        .trim()
        .split(/\s+/u)
        .filter((word) => word !== "")
        .join(" ")
        .toLowerCase();
    const first = out.charAt(0);
    const next = out.charAt(1);
    return /^[0-9]$/.test(first) && next !== "" && next !== " " && !/^[0-9]$/.test(next)
        ? `${first} ${out.slice(1)}`
        : out;
};
