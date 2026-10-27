/** The `usfm` (and `sfm`) TextMate grammar, for markdown fences and `highlight.ts`. */
import type { LanguageRegistration } from "shiki";
import grammar from "./usfm.tmLanguage.json" with { type: "json" };

export const usfmLang = grammar as unknown as LanguageRegistration;
