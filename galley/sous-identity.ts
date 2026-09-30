/**
 * Stable identities for a finding, its pattern and its place, for a consumer
 * that saves suppressions to disk. A persisted contract: the strings survive
 * reloads, republishes, edits elsewhere and settings changes, and changing
 * their format needs a migration of every saved one.
 *
 * ```text
 * patternIdentity(pattern)                    → "v1:p:Placement:2014:next:Digit"
 * identityOf(finding, pattern, { verseRef: "EXO 38:26", siteText: "—", ordinal: 0 })
 *                                             → "v1:f:p:Placement:2014:next:Digit:EXO 38%3A26:—:0"
 * siteIdentityOf({ verseRef: "EXO 38:26", siteText: "—", ordinal: 0 })
 *                                             → "v1:s:EXO 38%3A26:—:0"
 * ordinalOf("a a, a", "a", 5)                 → 2     // two earlier "a"s in the verse
 * ```
 *
 * Fields are `:`-separated, and every field is escaped: `%`, `:` and the C0
 * controls and DEL become `%XX`. What is in each and why: `sous-messages.md`,
 * "Stable identity".
 */

// The reader's types only, so this module loads beside it without importing it.
import type { Finding, Pattern } from "./sous-reader.ts";

/** The format's version, the first field of every identity. */
export const IDENTITY_VERSION = "v1";

/** Where a finding sits, as only the consumer knows it. */
export interface SiteContext {
  /** The verse, as the consumer names it: `"GEN 1:2"`. */
  readonly verseRef: string;
  /** The finding's own text: its span sliced from the book's text. */
  readonly siteText: string;
  /** How many identical `siteText`s start earlier in the same verse, 0-based:
   * `ordinalOf`. */
  readonly ordinal: number;
}

export interface PatternContext {
  /** A book's stable key by its position in the publication (`BookRate` only,
   * whose key names a book): `(index) => snapshot.book(index)!.key`. */
  readonly bookKey?: (index: number) => string;
}

/** `PATTERN_DIGIT_GLYPH` in the reader. */
const DIGIT_GLYPH = 0xffffffff;

function escape(field: string): string {
  return field.replace(/[%:\u0000-\u001f\u007f]/g, (char) => `%${char.charCodeAt(0).toString(16).toUpperCase().padStart(2, "0")}`);
}

function scalar(glyph: number): string {
  return glyph === DIGIT_GLYPH ? "digit" : glyph.toString(16);
}

function hash(value: bigint): string {
  return value.toString(16).padStart(16, "0");
}

/** The pattern's fields after `v1:p:`, each escaped. */
function patternFields(pattern: Pattern, context: PatternContext): string[] {
  const { key } = pattern;
  const glyph = scalar(pattern.glyph);
  switch (key.kind) {
    case "ExactNeighbor":
      return [key.kind, glyph, key.neighbor.toString(16)];
    case "PooledNeighbor":
      return [key.kind, glyph, key.pool];
    case "RunShape":
      return [key.kind, glyph, key.pure ? "pure" : "mixed", String(key.bucket)];
    case "Placement":
      return [key.kind, glyph, key.side, key.class];
    case "Rarity":
    case "SentenceStart":
      return [key.kind, glyph];
    case "LetterRun":
      return [key.kind, glyph, String(key.length)];
    case "Casing":
      return [key.kind, hash(key.hash), key.form];
    case "WordLength":
      return [key.kind, hash(key.hash)];
    case "Doubled":
      return [key.kind, hash(key.hash), key.separated ? "separated" : "bare"];
    case "BookRate": {
      if (context.bookKey === undefined) {
        throw new Error("a BookRate pattern's identity needs context.bookKey");
      }
      return [key.kind, glyph, key.side, key.class, escape(context.bookKey(key.book))];
    }
  }
}

/**
 * What a pattern claims, never how strongly: the channel, the glyph (a hex
 * code point, `digit` for the pooled digit lane) or the word's hash, and the
 * key fields.
 *
 * ```text
 * Placement      v1:p:Placement:2014:next:Digit      — before a digit
 * ExactNeighbor  v1:p:ExactNeighbor:27:2e            ' then .
 * RunShape       v1:p:RunShape:3b:mixed:2
 * Casing         v1:p:Casing:9f3c…e1:Title           the word by its hash
 * BookRate       v1:p:BookRate:2c:prev:Nonletter:GEN
 * ```
 */
export function patternIdentity(pattern: Pattern, context: PatternContext = {}): string {
  return [IDENTITY_VERSION, "p", ...patternFields(pattern, context)].join(":");
}

/** A non-convention finding's code, which settings cannot move. */
function findingCode(finding: Finding): string[] {
  switch (finding.kind) {
    case "Hygiene":
      return ["hygiene", finding.hygiene.class];
    case "Presence":
      return ["presence", finding.presence.kind];
    case "SourceCopy":
      return ["sourceCopy"];
    case "LengthProportionality":
      return ["length"];
    case "Convention":
      throw new Error("unreachable");
  }
}

function siteFields(site: SiteContext): string[] {
  if (!Number.isInteger(site.ordinal) || site.ordinal < 0) {
    throw new Error(`an ordinal is a whole number, not ${site.ordinal}`);
  }
  return [escape(site.verseRef), escape(site.siteText), String(site.ordinal)];
}

/**
 * This finding here: its pattern's fields (or its kind's code) and its place.
 *
 * ```text
 * v1:f:p:Rarity:2013:JOB 3%3A8:–:0
 * v1:f:hygiene:NoBreakSpace:GEN 1%3A1: :0
 * ```
 */
export function identityOf(finding: Finding, pattern: Pattern | undefined, site: SiteContext & PatternContext): string {
  let code: string[];
  if (finding.kind === "Convention") {
    if (pattern === undefined) throw new Error("a Convention finding needs its pattern for an identity");
    code = ["p", ...patternFields(pattern, site)];
  } else {
    code = findingCode(finding);
  }
  return [IDENTITY_VERSION, "f", ...code, ...siteFields(site)].join(":");
}

/**
 * This text here, whatever flags it: a settings change can move which rule
 * headlines a site, and a suppression saved against the site survives that.
 *
 * ```text
 * v1:s:GEN 48%3A20:'."":0
 * ```
 */
export function siteIdentityOf(site: SiteContext): string {
  return [IDENTITY_VERSION, "s", ...siteFields(site)].join(":");
}

/**
 * How many times `siteText` starts in `verseText` before `at`, overlapping
 * starts included: the `ordinal` of the occurrence at `at`.
 *
 * ```text
 * ordinalOf("a a, a", "a", 0)   → 0
 * ordinalOf("a a, a", "a", 5)   → 2
 * ordinalOf("aaa", "aa", 1)     → 1
 * ```
 *
 * `verseText` is the verse the consumer names in `verseRef`, in the same text
 * `siteText` was sliced from, and `at` the site's offset in it. Count the
 * same way every time: the ordinal is part of the saved identity.
 */
export function ordinalOf(verseText: string, siteText: string, at: number): number {
  if (siteText === "") return 0;
  let count = 0;
  for (let from = verseText.indexOf(siteText); from !== -1 && from < at; from = verseText.indexOf(siteText, from + 1)) {
    count += 1;
  }
  return count;
}
