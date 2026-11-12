/**
 * bento: a repo's manifest, read, checked, created and edited. TypeScript over
 * one wasm module; every rule lives in the Rust (`bento/src`), once.
 *
 *     import { init, find, read, readOrThrow, check, createBible, open, edit, schemas } from
 *       "@wycliffeassociates/scripture-kitchen/bento";
 *     await init();                                    // or initSync({ module: bytes })
 *
 *     const found = find(await fs.readdir(root));      // { name: "manifest.yaml", format: "rc" }
 *     const r = read(found.name, bytes);               // { manifest, detail, findings }: never throws
 *     r.manifest?.books.map((b) => b.book);            // ["MAT", "ROM"]
 *     readOrThrow(name, bytes);                        // throws when any finding is an error
 *     check(name, bytes, { name: "Tech_Advance/tpi_reg", files });  // findings that need the repo
 *     createBible(project);                            // Map { "metadata.json" => …, "manifest.yaml" => … }
 *     const m = open(existing);                        // every manifest the project has, open for editing
 *     m.addBook({ book: "MRK", title: "Mak", bytes });  // each call → its findings; one that throws changes nothing
 *     m.renameBook({ book: "JHN", short: "Yohani", long, abbr });
 *     m.updateChecksum("41-MAT.usfm", bytes);
 *     m.names();                                       // { "metadata.json": { JHN: { short, long, abbr } }, … }
 *     m.finish();                                      // { files: only those that changed, findings }
 *     edit(existing, { addBooks, checksums });         // the same, in one call
 *     writerProject(bytes, "tmf_3jn_text_reg");        // a Writer manifest's book project, or undefined
 *
 * Reading never changes anything and never throws: what it forgave or found
 * wrong is a finding, with a JSON pointer (`at`) into the file.
 */
import wasmInit, * as raw from "./pkg-web/bento.js";

/** Start the module: from its URL (the default), a `Response`, or its bytes. */
export const init = (input?: Parameters<typeof wasmInit>[0]) => wasmInit(input);
/** Start the module from bytes already in hand (`{ module: bytes }`). */
export const initSync = raw.initSync;

export type Format = "burrito" | "rc" | "bttwriter";
export type BookCode = string;
/** Text by language tag: `{ en: "Romans", tpi: "Rom" }`. */
export type LocalizedText = Record<string, string>;

export interface Read {
  /** Absent only when nothing could be read (an error finding says why). */
  manifest?: Manifest;
  detail?: Detail;
  findings: ManifestFinding[];
}

/** The file's own value, as written (a YAML file as its JSON equivalent). */
export type Detail =
  | { format: "rc"; document: RcManifest }
  | { format: "burrito"; document: BurritoMetadata }
  | { format: "bttwriter"; document: BttWriterManifest };

/** What every format says about a repo. */
export interface Manifest {
  format: Format;
  /** A Burrito's primary id (`Tech_Advance/cdi_reg`). */
  id?: string;
  name: LocalizedText;
  /** The resource's code: `{ en: "ulb" }`. */
  abbreviation: LocalizedText;
  language: { tag: string; name: LocalizedText; direction: "ltr" | "rtl" };
  kind: Kind;
  /** In the order the file lists them; only books of canon's table. */
  books: BookEntry[];
  sources: Related[];
  version?: string;
  issued?: string;
  modified?: string;
  rights?: string;
  publisher?: string;
}

export interface Kind {
  domain?: "scripture" | "gloss" | "parascriptural" | "peripheral";
  /** Canonical: `Bible`, `Translation Notes`, …; an unknown one as written. */
  subject?: string;
  /** A Burrito's flavor, as written: `textTranslation`. */
  flavor?: string;
  /** What the manifest itself says, as written. */
  declared: { subject?: string; type?: string; format?: string; identifier?: string };
}

export interface BookEntry {
  book: BookCode;
  path?: string;
  title: LocalizedText;
  /** RC's `sort`, as written; never corrected. */
  sort?: number;
}

export interface Related {
  relation: "source";
  language: string;
  resource: string;
  version?: string;
}

export type CheckId =
  | "manifest-missing"
  | "manifest-unreadable"
  | "manifest-valid"
  | "lenient-value"
  | "title-set"
  | "language-set"
  | "book-title-translated"
  | "sort-canon"
  | "repo-name-language"
  | "ingredient-exists"
  | "ingredient-empty"
  | "ingredient-checksum";

export interface ManifestFinding {
  check: CheckId;
  severity: "error" | "warning" | "info";
  /** A JSON pointer into the manifest: `/projects/0/sort`. */
  at?: string;
  book?: BookCode;
  /** A sentence a person can act on. */
  message: string;
}

/** Resource Container 0.2, as written. Fields bento reads; others pass through. */
export interface RcManifest {
  dublin_core?: {
    conformsto?: string;
    contributor?: string[];
    creator?: string;
    description?: string;
    format?: string;
    identifier?: string;
    issued?: string;
    language?: { identifier?: string; title?: string; direction?: string };
    modified?: string;
    publisher?: string;
    relation?: string[];
    rights?: string;
    source?: Array<{ identifier?: string; language?: string; version?: string }>;
    subject?: string;
    title?: string;
    type?: string;
    version?: string;
    [key: string]: unknown;
  };
  checking?: { checking_entity?: string[]; checking_level?: string };
  projects?: Array<{
    identifier?: string;
    title?: string;
    path?: string;
    sort?: number;
    versification?: string;
    categories?: string[];
    [key: string]: unknown;
  }>;
  [key: string]: unknown;
}

/** Scripture Burrito 1.0 (WA's fork), as written. Fields bento reads; others pass through. */
export interface BurritoMetadata {
  format?: "scripture burrito";
  meta?: {
    version?: string;
    category?: string;
    generator?: { softwareName?: string; softwareVersion?: string };
    defaultLocale?: string;
    dateCreated?: string;
    normalization?: string;
  };
  idAuthorities?: Record<string, { id?: string; name?: LocalizedText }>;
  identification?: {
    name?: LocalizedText;
    abbreviation?: LocalizedText;
    description?: LocalizedText;
    primary?: Record<string, Record<string, { revision?: string; timestamp?: string }>>;
  };
  confidential?: boolean;
  type?: {
    flavorType?: {
      name?: string;
      flavor?: { name?: string; usfmVersion?: string; [key: string]: unknown };
      currentScope?: Record<string, string[]>;
    };
  };
  copyright?: {
    publicDomain?: boolean;
    licenses?: Array<{ url?: string; ingredient?: string }>;
    shortStatements?: Array<{ statement: string; mimetype?: string; lang?: string }>;
  };
  languages?: Array<{ tag?: string; name?: LocalizedText; scriptDirection?: "ltr" | "rtl" }>;
  localizedNames?: Record<string, { short?: LocalizedText; long?: LocalizedText; abbr?: LocalizedText }>;
  ingredients?: Record<
    string,
    { checksum?: { md5?: string }; mimeType?: string; size?: number; scope?: Record<string, string[]> }
  >;
  relationships?: Array<{ relationType?: string; flavor?: string; id?: string; revision?: string }>;
  [key: string]: unknown;
}

/** BTT Writer's `manifest.json`, as written. Fields bento reads; others pass through. */
export interface BttWriterManifest {
  format?: string;
  target_language?: { id?: string; name?: string; direction?: string };
  project?: { id?: string; name?: string };
  type?: { id?: string; name?: string } | string;
  resource?: { id?: string; name?: string } | string;
  source_translations?: Array<{ language_id?: string; resource_id?: string; version?: string | number }>;
  translators?: string[];
  /** `front-title`, `01-title`, `01-05`. Absent when the file has none (translationCore). */
  finished_chunks?: string[];
  [key: string]: unknown;
}

/** What a BTT Writer book project is, once its manifest names a book, a language and a resource. */
export interface WriterProject {
  /** `ROM` */
  book: BookCode;
  language: { id: string; name?: string; direction: "ltr" | "rtl" };
  /** `reg`, `ulb`: lowercase */
  resource: string;
  sources: Related[];
  translators: string[];
}

/**
 * A Writer `manifest.json`'s book project, or `undefined` when it doesn't
 * name a book of the table, a language and a resource. `repo` is the repo or
 * folder name (`tmf_3jn_text_reg`): when the manifest names no resource,
 * Writer's naming `{language}_{book}_text_{resource}` supplies it.
 */
export const writerProject = (bytes: Uint8Array, repo?: string): WriterProject | undefined => {
  const found = raw.writerProject(bytes, repo);
  return found === undefined ? undefined : JSON.parse(found);
};

/** The manifest among a repo's root file names, if it has one. */
export const find = (rootNames: readonly string[]): { name: string; format: Format } | undefined => {
  const found = raw.find([...rootNames]);
  return found === undefined ? undefined : JSON.parse(found);
};

/** Read a manifest. Never throws: a file that can't be read is an error finding. */
export const read = (name: string, bytes: Uint8Array): Read => JSON.parse(raw.read(name, bytes));

/** The error a {@link readOrThrow} throws: every error finding. */
export class ManifestError extends Error {
  readonly findings: ManifestFinding[];
  constructor(name: string, findings: ManifestFinding[]) {
    super(`${name}: ${findings.map((f) => f.message).join(" ")}`);
    this.name = "ManifestError";
    this.findings = findings;
  }
}

/** {@link read}, where a bad manifest should stop the operation: throws on any error finding. */
export const readOrThrow = (name: string, bytes: Uint8Array): Read & { manifest: Manifest } => {
  const r = read(name, bytes);
  const errors = r.findings.filter((f) => f.severity === "error");
  if (errors.length > 0 || r.manifest === undefined) throw new ManifestError(name, errors);
  return r as Read & { manifest: Manifest };
};

/** The checks that need the repo: its name, and its files by path from the root. */
export const check = (
  name: string,
  bytes: Uint8Array,
  repo: { name?: string; files?: ReadonlyMap<string, Uint8Array> },
): ManifestFinding[] =>
  JSON.parse(raw.check(name, bytes, repo.name, repo.files === undefined ? undefined : new Map(repo.files)));

export interface NewBook {
  book: BookCode;
  /** The book's name in the project's language. */
  title: string;
  bytes: Uint8Array;
}

/** A new Bible project. Every field but `canon` is required: each app states its own policy. */
export interface NewBibleProject {
  title: string;
  /** The resource's code: `reg`. */
  abbreviation: string;
  language: { tag: string; name: string; direction: "ltr" | "rtl" };
  /** In any order; written in the canon's. */
  books: NewBook[];
  /** Default `protestant66`. */
  canon?: "protestant66" | "withDeuterocanon";
  sources: Array<{ language: string; resource: string; version?: string }>;
  contributors: string[];
  publisher: string;
  creator: string;
  rights: string;
  checkingLevel: string;
  /** The Burrito's copyright statement. */
  copyright: string;
  generator: { name: string; version: string };
  /** An ISO 8601 date-time by the caller's clock: `2026-11-05T14:30:00Z`. */
  createdAt: string;
  /** The project's WACS repo, `owner/repo`. */
  repository: string;
}

const books = (list: readonly NewBook[]) => ({
  meta: list.map(({ book, title }) => ({ book, title })),
  bytes: list.map((b) => b.bytes),
});

/**
 * The manifests for a new Bible project, by file name. Order, `sort`, paths
 * and categories come from canon. Throws on a book outside the canon, a book
 * twice, a bad `repository` or `createdAt`.
 */
export const createBible = (
  project: NewBibleProject,
  formats: ReadonlyArray<"burrito" | "rc"> = ["burrito", "rc"],
): Map<string, Uint8Array> => {
  const { meta, bytes } = books(project.books);
  return raw.createBible(JSON.stringify({ ...project, books: meta }), bytes, [...formats]);
};

export interface Change {
  /** Books to list; throws if one is listed already. */
  addBooks?: NewBook[];
  /** The project's canon, which places added books. Default `protestant66`. */
  canon?: "protestant66" | "withDeuterocanon";
  /** Files as they'll be committed, by path: their Burrito md5 and size come up to date. */
  checksums?: ReadonlyMap<string, Uint8Array>;
}

/**
 * Apply `change` to the manifests in `existing` (by file name). Returns only
 * the files that changed: commit them with the books, as one commit.
 */
export const edit = (
  existing: ReadonlyMap<string, Uint8Array>,
  change: Change,
): { files: Map<string, Uint8Array>; findings: ManifestFinding[] } => {
  const { meta, bytes } = books(change.addBooks ?? []);
  const checksums = [...(change.checksums ?? new Map<string, Uint8Array>())];
  const out = raw.edit(
    new Map(existing),
    JSON.stringify({ addBooks: meta, canon: change.canon, checksums: checksums.map(([p]) => p) }),
    bytes,
    checksums.map(([, b]) => b),
  ) as { files: Map<string, Uint8Array>; findings: string };
  return { files: out.files, findings: JSON.parse(out.findings) };
};

/** A book's names in the project's language: `\toc2`, `\toc1`, `\toc3`. */
export interface BookNames {
  book: BookCode;
  /** A Burrito's `short`, an RC's `title`. */
  short?: string;
  /** A Burrito's `long`; an RC has none. */
  long?: string;
  /** A Burrito's `abbr`; an RC has none. */
  abbr?: string;
}

/**
 * A project's manifests, open for editing. Calls apply in the order made, and
 * each returns its findings; one that throws changes nothing. `finish` ends
 * the handle (a dropped one is freed by the garbage collector; `free` frees
 * it now).
 */
export interface Manifests {
  /** List a book. Throws if it's listed already or isn't in the canon. */
  addBook(book: NewBook): ManifestFinding[];
  /**
   * Set a listed book's names. A Burrito's go under its language's tag, with
   * canon's English under `en`; an RC's title is `short`. A missing or blank
   * name is left as it is; a book a manifest doesn't list is a warning.
   */
  renameBook(names: BookNames): ManifestFinding[];
  /** Bring a file's Burrito md5 and size up to date. */
  updateChecksum(path: string, bytes: Uint8Array): ManifestFinding[];
  /** Each manifest's names for its books, in the project's language. */
  names(): Record<string, Record<BookCode, Omit<BookNames, "book">>>;
  /** The files that changed, and every call's findings. */
  finish(): { files: Map<string, Uint8Array>; findings: ManifestFinding[] };
  free(): void;
}

/** Open the manifests in `existing` (by file name). Throws on an unreadable Burrito. */
export const open = (
  existing: ReadonlyMap<string, Uint8Array>,
  canon: "protestant66" | "withDeuterocanon" = "protestant66",
): Manifests => {
  const handle = new raw.Manifests(new Map(existing), canon);
  return {
    addBook: ({ book, title, bytes }) => JSON.parse(handle.addBook(JSON.stringify({ book, title }), bytes)),
    renameBook: (names) => JSON.parse(handle.renameBook(JSON.stringify(names))),
    updateChecksum: (path, bytes) => JSON.parse(handle.updateChecksum(path, bytes)),
    names: () => JSON.parse(handle.names()),
    finish: () => {
      const out = handle.finish() as { files: Map<string, Uint8Array>; findings: string };
      return { files: out.files, findings: JSON.parse(out.findings) };
    },
    free: () => handle.free(),
  };
};

/** A file's md5 and size, as a Burrito ingredient records them. */
export const fingerprint = (bytes: Uint8Array): { md5: string; size: number } =>
  JSON.parse(raw.fingerprint(bytes));

// --- Standard Schema (https://standardschema.dev), v1 ----------------------

interface Issue {
  readonly message: string;
  readonly path?: ReadonlyArray<PropertyKey>;
}
type Result<Output> = { readonly value: Output; readonly issues?: undefined } | { readonly issues: ReadonlyArray<Issue> };

export interface StandardSchemaV1<Output> {
  readonly "~standard": {
    readonly version: 1;
    readonly vendor: "bento";
    readonly validate: (value: unknown) => Result<Output>;
    readonly types?: { readonly input: unknown; readonly output: Output };
  };
}

/** `/projects/0/sort` → `["projects", 0, "sort"]`. */
const pathOf = (at: string | undefined): PropertyKey[] | undefined =>
  at === undefined || at === ""
    ? undefined
    : at
        .slice(1)
        .split("/")
        .map((p) => p.replaceAll("~1", "/").replaceAll("~0", "~"))
        .map((p) => (/^\d+$/.test(p) ? Number(p) : p));

const encoder = new TextEncoder();

const schema = <Output>(file: string): StandardSchemaV1<Output> => ({
  "~standard": {
    version: 1,
    vendor: "bento",
    validate: (value: unknown): Result<Output> => {
      // JSON is YAML too, so an RC's value reads through the same YAML reader.
      const r = read(file, encoder.encode(JSON.stringify(value) ?? "null"));
      const issues = r.findings
        .filter((f) => f.severity === "error" || f.check === "manifest-valid")
        .map((f): Issue => {
          const path = pathOf(f.at);
          return path === undefined ? { message: f.message } : { message: f.message, path };
        });
      return issues.length > 0 ? { issues } : { value: value as Output };
    },
  },
});

/**
 * Each format as a Standard Schema: `validate(value)` takes an already-parsed
 * manifest and returns it, typed, or the issues: errors, and fields the format
 * requires (`manifest-valid`). Leniencies and health findings aren't issues.
 */
export const schemas = {
  burrito: schema<BurritoMetadata>("metadata.json"),
  rc: schema<RcManifest>("manifest.yaml"),
  bttwriter: schema<BttWriterManifest>("manifest.json"),
};
