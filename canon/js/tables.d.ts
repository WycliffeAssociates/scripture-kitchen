export type Division = "ot" | "nt" | "deuterocanon" | "additional" | "peripheral" | "extra";
export interface Book {
    readonly code: string;
    readonly usfmNumber: string;
    readonly division: Division;
    readonly canonicalIndex: number | null;
    readonly english: string;
    readonly aliases: readonly string[];
}
export declare const BOOKS: readonly Book[];
/** Every English name and alias, folded, to the code it names. */
export declare const NAMES: ReadonlyMap<string, string>;
/** The locales with a names module: `import fr from "…/names/fr"`. */
export declare const NAME_LOCALES: readonly string[];
