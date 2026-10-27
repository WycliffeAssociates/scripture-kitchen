/**
 * The /features spec sheet: hand-written rows from planning/atlas/features.ts,
 * and the rows a checked-in file already knows, read at build so they cannot
 * drift.
 *
 *   onion-wasm/diagnostics.json   { codes: [{ name, category, fixLabel }] }  → Lint, per category
 *   galley/sous-messages.en.json  { "convention.runShape.swapped": … }      → Sous checks, per family
 *   docs/layouts/*.json           { format, version, doc }                    → Wires
 *   verify-kitchen features/README.md table                                   → How each is proven
 */
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { AFTER, BEFORE, HOSTS, SOUS_EXTRA, SOUS_NOTES, type FeatureGroup, type FeatureItem } from "../../atlas/features.ts";
import { layoutsDir } from "./atlas/paths.ts";
import { citeLink } from "./atlas/github.ts";
import { repoRoot } from "./root.ts";

export type { FeatureGroup, FeatureItem };

export const DIAGNOSTICS = "onion-wasm/diagnostics.json";
export const SOUS_CATALOG = "galley/sous-messages.en.json";
export const FEATURES_DIR = ".claude/skills/verify-kitchen/features";

const json = <T>(path: string): T => JSON.parse(readFileSync(join(repoRoot, path), "utf8")) as T;

export interface LintCategory {
  readonly name: string;
  readonly codes: readonly { readonly name: string; readonly fix: boolean }[];
}

export const lintCategories = (): LintCategory[] => {
  const { codes } = json<{ codes: { name: string; category: string; fixLabel: string | null }[] }>(DIAGNOSTICS);
  const by = new Map<string, { name: string; fix: boolean }[]>();
  for (const c of codes) by.set(c.category, [...(by.get(c.category) ?? []), { name: c.name, fix: c.fixLabel !== null }]);
  return [...by].map(([name, list]) => ({ name, codes: list }));
};

/** `convention.runShape.swapped` → `convention.runShape`; `hygiene.control` → `hygiene`. */
const familyOf = (id: string): string => {
  const parts = id.split(".");
  return parts[0] === "convention" ? parts.slice(0, 2).join(".") : parts[0]!;
};

export interface SousFamily {
  readonly key: string;
  readonly messages: readonly string[];
}

export const sousFamilies = (): SousFamily[] => {
  const by = new Map<string, string[]>();
  for (const id of Object.keys(json<Record<string, unknown>>(SOUS_CATALOG))) by.set(familyOf(id), [...(by.get(familyOf(id)) ?? []), id]);
  return [...by].map(([key, messages]) => ({ key, messages }));
};

export interface WireRow {
  readonly format: string;
  readonly version: number;
  readonly doc: string;
}

export const wireRows = (): WireRow[] =>
  existsSync(layoutsDir)
    ? readdirSync(layoutsDir)
        .filter((f) => f.endsWith(".json"))
        .map((f) => JSON.parse(readFileSync(join(layoutsDir, f), "utf8")) as WireRow)
        .sort((a, b) => a.format.localeCompare(b.format))
    : [];

/** Which concept page explains each wire, by format. */
const WIRE_CONCEPT: Readonly<Record<string, string>> = {
  census: "census", corpus: "corpus", dish: "dish", find: "find-buffer", mask: "mask-map", sous: "findings-snapshot",
};

export interface Proof {
  readonly name: string;
  readonly file: string;
  readonly helper: string;
  readonly exists: boolean;
}

/** The verify-kitchen README's `| Feature | File | Helper |` rows. */
export const proofs = (): Proof[] => {
  const path = join(repoRoot, FEATURES_DIR, "README.md");
  if (!existsSync(path)) return [];
  return readFileSync(path, "utf8")
    .split("\n")
    .filter((l) => /^\|/u.test(l) && !/^\|\s*-/u.test(l))
    .slice(1)
    .map((l) => l.split("|").slice(1, -1).map((c) => c.trim()))
    .map(([name = "", link = "", helper = ""]) => {
      const file = `${FEATURES_DIR}/${/\(([^)]+)\)/u.exec(link)?.[1] ?? link}`;
      return { name, file, helper: helper.replace(/^`|`$/gu, ""), exists: existsSync(join(repoRoot, file)) };
    });
};

export interface Sheet {
  readonly before: readonly FeatureGroup[];
  readonly lint: { readonly door: FeatureItem; readonly categories: LintCategory[] };
  readonly after: readonly FeatureGroup[];
  readonly sous: FeatureGroup;
  readonly wires: FeatureGroup;
  readonly hosts: FeatureGroup;
}

export const sheet = (): Sheet => ({
  before: BEFORE,
  lint: {
    door: { name: "Lint", note: "", concept: "lint", door: "galley.lint(id)", cite: "galley/src/wasm.rs#Galley::lint" },
    categories: lintCategories(),
  },
  after: AFTER,
  sous: {
    id: "sous",
    title: "Sous Chef checks",
    items: [
      ...sousFamilies().map((f): FeatureItem => {
        const note = SOUS_NOTES[f.key];
        return {
          name: note?.name ?? f.key,
          note: `${note?.note ?? ""}${f.messages.length > 1 ? ` · ${f.messages.length} messages` : ""}`,
          ...(note?.concept === undefined ? {} : { concept: note.concept }),
          door: f.key,
        };
      }),
      ...SOUS_EXTRA,
    ],
  },
  wires: {
    id: "wires",
    title: "Wires",
    items: wireRows().map((w) => ({
      name: w.format,
      note: `v${w.version} · ${w.doc}`,
      href: `/atlas/wires/${w.format}`,
      ...(WIRE_CONCEPT[w.format] === undefined ? {} : { concept: WIRE_CONCEPT[w.format] }),
    })),
  },
  hosts: HOSTS,
});

/** Each problem in the hand-written rows, for `pnpm atlas check`. */
export const checkFeatures = (concepts: ReadonlySet<string>, items: ReadonlySet<string>): string[] => {
  const where = "planning/atlas/features.ts";
  const problems: string[] = [];
  const s = sheet();
  const rows = [...s.before, ...s.after, s.sous, s.wires, s.hosts].flatMap((g) => g.items.map((i) => [g.title, i] as const));
  rows.push(["Lint", s.lint.door]);
  for (const [group, i] of rows) {
    const at = `${where}: ${group} › ${i.name}`;
    if (i.concept !== undefined && !concepts.has(i.concept)) problems.push(`${at}: concept "${i.concept}" is not a concept page`);
    if (i.item !== undefined && !items.has(i.item)) problems.push(`${at}: item "${i.item}" is not a plan`);
    if (i.cite !== undefined) {
      const link = citeLink(i.cite);
      if (!link.ok) problems.push(`${at}: ${i.cite}: ${link.error}`);
    }
  }
  const families = new Set(sousFamilies().map((f) => f.key));
  for (const key of families) if (SOUS_NOTES[key] === undefined) problems.push(`${where}: Sous family "${key}" (${SOUS_CATALOG}) has no SOUS_NOTES row`);
  for (const key of Object.keys(SOUS_NOTES)) if (!families.has(key)) problems.push(`${where}: SOUS_NOTES "${key}" is no family in ${SOUS_CATALOG}`);
  for (const w of wireRows()) if (WIRE_CONCEPT[w.format] === undefined) problems.push(`src/lib/features.ts: wire "${w.format}" has no WIRE_CONCEPT row`);
  for (const p of proofs()) if (!p.exists) problems.push(`${FEATURES_DIR}/README.md: ${p.file} does not exist`);
  return problems;
};
