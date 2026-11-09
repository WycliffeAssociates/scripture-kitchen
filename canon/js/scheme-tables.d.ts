export type SchemeName = "org" | "eng" | "lxx" | "vul" | "rsc" | "rso" | "ulb";
export declare const SCHEME_NAMES: readonly SchemeName[];
/** Per scheme, each book's last verse per chapter, chapter 1 first. */
export declare const VERSE_COUNTS: Readonly<Record<SchemeName, ReadonlyMap<string, readonly number[]>>>;
/** Per scheme, the verses it omits on purpose, as `"MAT 17:21"`. */
export declare const EXCLUDED: Readonly<Record<SchemeName, ReadonlySet<string>>>;
