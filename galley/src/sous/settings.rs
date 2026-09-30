//! The judging settings a host sets from JavaScript, and the typed
//! `sous-settings.ts` generated from them.
//!
//! ```text
//! SousSettings { doubled_bare: bool, support_floor: u32, z_long: f32, … }
//!   + SETTING_DOCS ("doubled_bare", Switch, Rules, "Doubled with a space", "Flags a word …")
//!   → sous-settings.ts
//!       export type SettingKey = "placement" | … | "source_copy_min_run";
//!       export interface SousSettingsValues { readonly doubled_bare: boolean; … }
//!       export const SOUS_SETTINGS = { doubled_bare: { kind: "switch", default: true, … }, … };
//!       export function toSettings(galley, values): void; fromSettings(galley)
//! ```
//!
//! `cargo run -p usfm_galley --bin codegen` writes it;
//! `tests/codegen_output_matches_input.rs` fails when it is stale.

use core::fmt::Write as _;

use sous_core::judge::{Channels, JudgingConfig};
use sous_core::proportionality::LengthConfig;

/// The judging settings that cross the wall: every plain scalar of
/// [`JudgingConfig`], flat, so bindgen writes the getters and setters and JS
/// assigns `settings.casing = false`.
///
/// Not on the wall: `bands` and `word_bands` (a `Staircase` is a validated
/// ladder, not a plain field), `letters` and `doubles` (tri-state policies),
/// and the roster bounds. They have no plain-field shape and no consumer has
/// asked for them; everything not a knob keeps the current config's value
/// through [`apply`](SousSettings::apply), so widening this later breaks nothing.
#[cfg_attr(feature = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SousSettings {
    // Channels.
    pub placement: bool,
    pub run_shape: bool,
    pub exact_neighbor: bool,
    pub pooled_neighbor: bool,
    pub rarity: bool,
    pub casing: bool,
    pub word_length: bool,
    pub doubled: bool,
    pub letter_runs: bool,
    pub sentence_start: bool,
    pub book_rate: bool,
    // Thresholds.
    pub support_floor: u32,
    pub word_support_floor: u32,
    pub terminal_upper_share_bp: u16,
    pub sentence_start_upper_bp: u16,
    pub word_length_sigma: u8,
    pub doubles_productive_bp: u16,
    // Which doublings the doubled channel judges.
    pub doubled_bare: bool,
    pub doubled_separated: bool,
    // When one book's rate breaks from the rest.
    pub book_rate_ratio: u16,
    pub book_rate_min_bp: u16,
    // The source-compared lane.
    pub z_long: f32,
    pub z_short: f32,
    pub min_verses: u32,
    pub lengths_enabled: bool,
    pub presence: bool,
    pub source_copy: bool,
    pub source_copy_min_run: u32,
}

impl SousSettings {
    /// Every knob as this config holds it.
    pub fn from(config: &JudgingConfig) -> Self {
        let Channels {
            placement,
            run_shape,
            exact_neighbor,
            pooled_neighbor,
            rarity,
            casing,
            word_length,
            doubled,
            letter_runs,
            sentence_start,
            book_rate,
        } = config.channels;
        Self {
            placement,
            run_shape,
            exact_neighbor,
            pooled_neighbor,
            rarity,
            casing,
            word_length,
            doubled,
            letter_runs,
            sentence_start,
            book_rate,
            support_floor: config.support_floor,
            word_support_floor: config.word_support_floor,
            terminal_upper_share_bp: config.terminal_upper_share_bp,
            sentence_start_upper_bp: config.sentence_start_upper_bp,
            word_length_sigma: config.word_length_sigma,
            doubles_productive_bp: config.doubles_productive_bp,
            doubled_bare: config.doubled_bare,
            doubled_separated: config.doubled_separated,
            book_rate_ratio: config.book_rate_ratio,
            book_rate_min_bp: config.book_rate_min_bp,
            z_long: config.lengths.z_long,
            z_short: config.lengths.z_short,
            min_verses: config.lengths.min_verses,
            lengths_enabled: config.lengths.enabled,
            presence: config.lengths.presence,
            source_copy: config.lengths.source_copy,
            source_copy_min_run: config.lengths.source_copy_min_run,
        }
    }

    /// Writes them back, leaving every field that is not a knob alone.
    pub fn apply(self, config: &mut JudgingConfig) {
        config.channels = Channels {
            placement: self.placement,
            run_shape: self.run_shape,
            exact_neighbor: self.exact_neighbor,
            pooled_neighbor: self.pooled_neighbor,
            rarity: self.rarity,
            casing: self.casing,
            word_length: self.word_length,
            doubled: self.doubled,
            letter_runs: self.letter_runs,
            sentence_start: self.sentence_start,
            book_rate: self.book_rate,
        };
        config.support_floor = self.support_floor;
        config.word_support_floor = self.word_support_floor;
        config.terminal_upper_share_bp = self.terminal_upper_share_bp;
        config.sentence_start_upper_bp = self.sentence_start_upper_bp;
        config.word_length_sigma = self.word_length_sigma;
        config.doubles_productive_bp = self.doubles_productive_bp;
        config.doubled_bare = self.doubled_bare;
        config.doubled_separated = self.doubled_separated;
        config.book_rate_ratio = self.book_rate_ratio;
        config.book_rate_min_bp = self.book_rate_min_bp;
        config.lengths = LengthConfig {
            z_long: self.z_long,
            z_short: self.z_short,
            min_verses: self.min_verses,
            enabled: self.lengths_enabled,
            presence: self.presence,
            source_copy: self.source_copy,
            source_copy_min_run: self.source_copy_min_run,
        };
    }
}

impl Default for SousSettings {
    fn default() -> Self {
        Self::from(&JudgingConfig::default())
    }
}

/// How a setting is edited: a switch, a whole count, a share out of 10,000,
/// or a decimal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Switch,
    Count,
    ShareBp,
    Decimal,
}

/// Where a settings page lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Which checks run.
    Rules,
    /// How much evidence a check needs.
    Thresholds,
    /// The checks that compare against a source text.
    Reference,
}

/// One setting's page entry. The description is one plain sentence for a
/// translator who is neither a linguist nor a programmer, and names no unit:
/// the kind carries it, and a page may show a share as a percent.
pub struct SettingDoc {
    pub key: &'static str,
    pub kind: Kind,
    pub group: Group,
    pub label: &'static str,
    pub description: &'static str,
    /// Bounds a page offers; an integer's type bounds it where these do not.
    pub min: Option<f64>,
    pub max: Option<f64>,
}

const fn doc(
    key: &'static str,
    kind: Kind,
    group: Group,
    label: &'static str,
    description: &'static str,
) -> SettingDoc {
    SettingDoc {
        key,
        kind,
        group,
        label,
        description,
        min: None,
        max: None,
    }
}

const fn at_least(mut doc: SettingDoc, min: f64) -> SettingDoc {
    doc.min = Some(min);
    doc
}

/// Every field of [`SousSettings`], described. The generator refuses a field
/// without an entry and an entry without a field.
pub const SETTING_DOCS: &[SettingDoc] = {
    use Group::{Reference, Rules, Thresholds};
    use Kind::{Count, Decimal, ShareBp, Switch};
    &[
        doc(
            "placement",
            Switch,
            Rules,
            "What a mark touches",
            "Flags a punctuation mark or digit right next to something it is rarely next to in this project, like a comma touching the next word, by counting every use of that mark across the whole project.",
        ),
        doc(
            "run_shape",
            Switch,
            Rules,
            "Groups of marks",
            "Flags a group of marks written together, like “;'”, when this project rarely groups that mark that way and does not write that exact group often.",
        ),
        doc(
            "exact_neighbor",
            Switch,
            Rules,
            "Pairs of marks",
            "Flags two marks side by side, like “?,”, when this project almost never puts the second one right after the first.",
        ),
        doc(
            "pooled_neighbor",
            Switch,
            Rules,
            "A mark and the kind of mark after it",
            "Flags a mark followed directly by a kind of mark, such as a quotation mark or a dash, that rarely follows it in this project; off by default because the pairs check already covers it.",
        ),
        doc(
            "rarity",
            Switch,
            Rules,
            "Rare characters",
            "Lists characters this project uses only a handful of times in all its books, like a curly apostrophe in a project that otherwise uses straight ones.",
        ),
        doc(
            "casing",
            Switch,
            Rules,
            "Capital letters",
            "Flags a word capitalized, or not, in a way this project rarely writes that word in the middle of a sentence, like “On” where the project writes “on”.",
        ),
        doc(
            "word_length",
            Switch,
            Rules,
            "Very long words",
            "Flags words far longer than the words this project usually uses, which can mean two words ran together; off by default because long names are common.",
        ),
        doc(
            "doubled",
            Switch,
            Rules,
            "Doubled words",
            "Turns every check for a word written twice in a row on or off.",
        ),
        doc(
            "letter_runs",
            Switch,
            Rules,
            "Repeated letters",
            "Flags a letter written more times in a row than this project ever writes it, like “joyfullly”.",
        ),
        doc(
            "sentence_start",
            Switch,
            Rules,
            "Lowercase after a sentence end",
            "Flags a lowercase word right after a mark, like “?” or “!”, that this project almost always follows with a capital.",
        ),
        doc(
            "book_rate",
            Switch,
            Rules,
            "One book differs",
            "Flags one book that puts a mark next to something far more often than the project's other books do, like a dash after a space in one book only.",
        ),
        at_least(
            doc(
                "support_floor",
                Count,
                Thresholds,
                "Habit count",
                "How many times something must occur before the checks treat it as a habit of this project rather than a one-off.",
            ),
            1.0,
        ),
        at_least(
            doc(
                "word_support_floor",
                Count,
                Thresholds,
                "Word habit count",
                "How many times a word must appear in the middle of a sentence before the capital-letter check compares its spellings.",
            ),
            1.0,
        ),
        doc(
            "terminal_upper_share_bp",
            ShareBp,
            Thresholds,
            "Capital after a mark",
            "How often this project must follow a mark with a capital before a capital there counts as the mark's doing and not the word's.",
        ),
        doc(
            "sentence_start_upper_bp",
            ShareBp,
            Thresholds,
            "Capital expected after a mark",
            "How often this project must follow a mark with a capital before a lowercase word after that mark is flagged.",
        ),
        at_least(
            doc(
                "word_length_sigma",
                Count,
                Thresholds,
                "How long a very long word is",
                "How far past this project's usual word length a word must be, counted in steps of how much word lengths usually vary, before the long-word check flags it.",
            ),
            1.0,
        ),
        doc(
            "doubles_productive_bp",
            ShareBp,
            Thresholds,
            "Repeating language",
            "When more than this share of the different words in the project are written twice in a row more than once, the project is taken to repeat words on purpose and the doubled-word checks stay silent.",
        ),
        doc(
            "doubled_bare",
            Switch,
            Rules,
            "Doubled with a space",
            "Flags a word written twice in a row with only a space between, like “the the”, unless this project does that with the word often.",
        ),
        doc(
            "doubled_separated",
            Switch,
            Rules,
            "Doubled with punctuation",
            "Flags a word written twice with punctuation between, like “Moses, Moses”, unless this project does that with the word often; off by default because calling someone by name twice is common.",
        ),
        at_least(
            doc(
                "book_rate_ratio",
                Count,
                Thresholds,
                "One book: how many times more",
                "How many times more often than the other books typically do one book must put a mark next to something before the one-book check flags it.",
            ),
            1.0,
        ),
        doc(
            "book_rate_min_bp",
            ShareBp,
            Thresholds,
            "One book: least share",
            "The least share of a mark's uses in one book that must be next to the same thing before the one-book check flags that book.",
        ),
        at_least(
            doc(
                "z_long",
                Decimal,
                Reference,
                "Long verse distance",
                "How far past this project's usual length compared with the source a verse must be, on the long side, before it is flagged as much longer.",
            ),
            0.0,
        ),
        at_least(
            doc(
                "z_short",
                Decimal,
                Reference,
                "Short verse distance",
                "How far short of this project's usual length compared with the source a verse must be before it is flagged as much shorter.",
            ),
            0.0,
        ),
        doc(
            "min_verses",
            Count,
            Reference,
            "Verses to compare a book",
            "How many verses a book must share with the source before its verse lengths are compared within that book as well as across the project.",
        ),
        doc(
            "lengths_enabled",
            Switch,
            Reference,
            "Verse lengths",
            "Compares each verse's length with the same verse in the source text and flags verses much longer or shorter than this project usually is.",
        ),
        doc(
            "presence",
            Switch,
            Reference,
            "Missing and extra verses",
            "Flags verses the source text has and this book lacks, verses this book has and the source lacks, and verses left empty.",
        ),
        doc(
            "source_copy",
            Switch,
            Reference,
            "Copied source words",
            "Flags words in a row spelled exactly as in the same verse of the source text, which can mean source text was pasted in; off by default.",
        ),
        at_least(
            doc(
                "source_copy_min_run",
                Count,
                Reference,
                "Copied words in a row",
                "How many words in a row must match the source verse before the copied-words check flags them.",
            ),
            2.0,
        ),
    ]
};

/// A default as the wasm class types it.
#[derive(Debug, Clone, Copy)]
enum Value {
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    F32(f32),
}

impl Value {
    fn ts_type(self) -> &'static str {
        match self {
            Value::Bool(_) => "boolean",
            _ => "number",
        }
    }

    fn literal(self) -> String {
        match self {
            Value::Bool(b) => b.to_string(),
            Value::U8(n) => n.to_string(),
            Value::U16(n) => n.to_string(),
            Value::U32(n) => n.to_string(),
            // The f64 the wasm getter hands back for this f32.
            Value::F32(x) => f64::from(x).to_string(),
        }
    }

    fn type_max(self) -> Option<f64> {
        match self {
            Value::U8(_) => Some(f64::from(u8::MAX)),
            Value::U16(_) => Some(f64::from(u16::MAX)),
            Value::U32(_) => Some(f64::from(u32::MAX)),
            Value::Bool(_) | Value::F32(_) => None,
        }
    }

    fn fits(self, kind: Kind) -> bool {
        matches!(
            (self, kind),
            (Value::Bool(_), Kind::Switch)
                | (Value::F32(_), Kind::Decimal)
                | (Value::U8(_) | Value::U16(_) | Value::U32(_), Kind::Count)
                | (Value::U16(_), Kind::ShareBp)
        )
    }
}

/// Every field in declaration order with its default. Destructured, so a new
/// field does not compile until it is listed here.
fn fields() -> Vec<(&'static str, Value)> {
    let SousSettings {
        placement,
        run_shape,
        exact_neighbor,
        pooled_neighbor,
        rarity,
        casing,
        word_length,
        doubled,
        letter_runs,
        sentence_start,
        book_rate,
        support_floor,
        word_support_floor,
        terminal_upper_share_bp,
        sentence_start_upper_bp,
        word_length_sigma,
        doubles_productive_bp,
        doubled_bare,
        doubled_separated,
        book_rate_ratio,
        book_rate_min_bp,
        z_long,
        z_short,
        min_verses,
        lengths_enabled,
        presence,
        source_copy,
        source_copy_min_run,
    } = SousSettings::default();
    use Value::{Bool, F32, U8, U16, U32};
    vec![
        ("placement", Bool(placement)),
        ("run_shape", Bool(run_shape)),
        ("exact_neighbor", Bool(exact_neighbor)),
        ("pooled_neighbor", Bool(pooled_neighbor)),
        ("rarity", Bool(rarity)),
        ("casing", Bool(casing)),
        ("word_length", Bool(word_length)),
        ("doubled", Bool(doubled)),
        ("letter_runs", Bool(letter_runs)),
        ("sentence_start", Bool(sentence_start)),
        ("book_rate", Bool(book_rate)),
        ("support_floor", U32(support_floor)),
        ("word_support_floor", U32(word_support_floor)),
        ("terminal_upper_share_bp", U16(terminal_upper_share_bp)),
        ("sentence_start_upper_bp", U16(sentence_start_upper_bp)),
        ("word_length_sigma", U8(word_length_sigma)),
        ("doubles_productive_bp", U16(doubles_productive_bp)),
        ("doubled_bare", Bool(doubled_bare)),
        ("doubled_separated", Bool(doubled_separated)),
        ("book_rate_ratio", U16(book_rate_ratio)),
        ("book_rate_min_bp", U16(book_rate_min_bp)),
        ("z_long", F32(z_long)),
        ("z_short", F32(z_short)),
        ("min_verses", U32(min_verses)),
        ("lengths_enabled", Bool(lengths_enabled)),
        ("presence", Bool(presence)),
        ("source_copy", Bool(source_copy)),
        ("source_copy_min_run", U32(source_copy_min_run)),
    ]
}

/// A TypeScript string literal.
fn quote(text: &str) -> String {
    let mut out = String::from('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `sous-settings.ts`, from [`SousSettings`] and [`SETTING_DOCS`].
///
/// # Panics
///
/// On a field with no entry, an entry with no field, a kind the field's type
/// cannot hold, or a description that is not one sentence or names a unit.
#[must_use]
pub fn settings_ts() -> String {
    let fields = fields();
    for entry in SETTING_DOCS {
        assert!(
            fields.iter().any(|(key, _)| *key == entry.key),
            "SETTING_DOCS describes `{}`, which SousSettings does not have",
            entry.key
        );
    }
    let rows: Vec<(&str, Value, &SettingDoc)> = fields
        .iter()
        .map(|&(key, value)| {
            let entry = SETTING_DOCS
                .iter()
                .find(|entry| entry.key == key)
                .unwrap_or_else(|| panic!("`{key}` has no entry in SETTING_DOCS"));
            assert!(
                value.fits(entry.kind),
                "`{key}` cannot be a {:?}",
                entry.kind
            );
            let sentence = entry.description.trim_end_matches('.');
            assert!(
                entry.description.ends_with('.') && !sentence.contains(". "),
                "`{key}`'s description is not one sentence"
            );
            assert!(
                !["10,000", "percent", "%"]
                    .iter()
                    .any(|unit| entry.description.contains(unit)),
                "`{key}`'s description names a unit its kind already carries"
            );
            (key, value, entry)
        })
        .collect();

    let mut ts = String::from(HEADER);
    let keys: Vec<String> = rows.iter().map(|(key, ..)| quote(key)).collect();
    let _ = writeln!(
        ts,
        "export type SettingKey =\n  | {};\n",
        keys.join("\n  | ")
    );
    ts.push_str("/** Every setting, as the wasm `SousSettings` class types it. */\n");
    ts.push_str("export interface SousSettingsValues {\n");
    for (key, value, _) in &rows {
        let _ = writeln!(ts, "  readonly {key}: {};", value.ts_type());
    }
    ts.push_str("}\n\n");
    ts.push_str(SPEC);
    let _ = writeln!(
        ts,
        "export const SETTING_KEYS: readonly SettingKey[] = [\n  {},\n];\n",
        keys.join(",\n  ")
    );
    ts.push_str("export const SOUS_SETTINGS: { readonly [K in SettingKey]: SettingSpec<K> } = {\n");
    for (key, value, entry) in &rows {
        let kind = match entry.kind {
            Kind::Switch => "switch",
            Kind::Count => "count",
            Kind::ShareBp => "share-bp",
            Kind::Decimal => "decimal",
        };
        let group = match entry.group {
            Group::Rules => "rules",
            Group::Thresholds => "thresholds",
            Group::Reference => "reference",
        };
        let max = match entry.kind {
            Kind::ShareBp => entry.max.or(Some(10_000.0)),
            _ => entry.max.or(value.type_max()),
        };
        let min = entry.min.or(value.type_max().map(|_| 0.0));
        let mut bounds = String::new();
        if let Some(min) = min {
            let _ = write!(bounds, "\n    min: {min},");
        }
        if let Some(max) = max {
            let _ = write!(bounds, "\n    max: {max},");
        }
        let _ = writeln!(
            ts,
            "  {key}: {{\n    key: {},\n    kind: \"{kind}\",\n    default: {},{bounds}\n    group: \"{group}\",\n    label: {},\n    description:\n      {},\n  }},",
            quote(key),
            value.literal(),
            quote(entry.label),
            quote(entry.description),
        );
    }
    ts.push_str("};\n\n");
    ts.push_str(EXACT);
    ts.push_str("/** A copy of the settings the next publication judges with. */\n");
    ts.push_str("export function fromSettings(galley: SettingsHost): SousSettingsValues {\n");
    ts.push_str("  const handle = galley.config();\n  try {\n    return {\n");
    for (key, ..) in &rows {
        let _ = writeln!(ts, "      {key}: handle.{key},");
    }
    ts.push_str("    };\n  } finally {\n    handle.free();\n  }\n}\n\n");
    ts.push_str("/** Replaces every setting; the next publication judges with them. */\n");
    ts.push_str(
        "export function toSettings(galley: SettingsHost, values: SousSettingsValues): void {\n",
    );
    ts.push_str("  const handle = galley.config();\n  try {\n");
    for (key, ..) in &rows {
        let _ = writeln!(ts, "    handle.{key} = values.{key};");
    }
    ts.push_str("  } catch (error) {\n    handle.free();\n    throw error;\n  }\n");
    ts.push_str("  // `setConfig` takes the handle; nothing is left to free.\n");
    ts.push_str("  galley.setConfig(handle);\n}\n");
    ts
}

const HEADER: &str = r#"/**
 * @generated by `cargo run -p usfm_galley --bin codegen` from
 * `galley/src/sous/settings.rs`. DO NOT EDIT.
 *
 * ```text
 * const values = fromSettings(galley);      // { placement: true, …, support_floor: 5, … }
 * toSettings(galley, { ...values, doubled_separated: true });
 * SOUS_SETTINGS.support_floor.description   // "How many times something must occur…"
 * ```
 *
 * A setting kitchen adds, removes, or retypes changes `SousSettingsValues`, so
 * a host that restates it fails to compile rather than missing it.
 */

import type { SousSettings as BundlerSettings } from "./pkg-bundler/usfm_galley.js";
import type { SousSettings as WebSettings } from "./pkg-web/usfm_galley.js";

"#;

const SPEC: &str = r#"export type SettingKind = "switch" | "count" | "share-bp" | "decimal";
export type SettingGroup = "rules" | "thresholds" | "reference";

export interface SettingSpec<K extends SettingKey> {
  readonly key: K;
  /** A share-bp is out of 10,000. */
  readonly kind: SettingKind;
  readonly default: SousSettingsValues[K];
  readonly min?: number;
  readonly max?: number;
  readonly group: SettingGroup;
  readonly label: string;
  /** One plain sentence: what the check looks for. */
  readonly description: string;
}

"#;

const EXACT: &str = r#"type DataKey<T> = { [K in keyof T]: T[K] extends (...args: never[]) => unknown ? never : K }[keyof T];
type Same<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false;
type Field<C, K> = K extends keyof C ? C[K] : never;
type Retyped<C> = { [K in SettingKey]: Same<SousSettingsValues[K], Field<C, K>> extends true ? never : K }[SettingKey];
type ExactlyTheClass<C> = Same<SettingKey, DataKey<C>> extends true ? ([Retyped<C>] extends [never] ? true : false) : false;
// Fails to compile when this file and either build's `SousSettings` disagree.
const exactlyTheClass: [ExactlyTheClass<BundlerSettings>, ExactlyTheClass<WebSettings>] = [true, true];
void exactlyTheClass;

/** The settings handle `Galley.config()` hands out, in either build. */
export type SettingsHandle = { -readonly [K in SettingKey]: SousSettingsValues[K] } & { free(): void };

/** What `toSettings` and `fromSettings` need of a `Galley`, from either build. */
export interface SettingsHost {
  config(): SettingsHandle;
  setConfig(settings: SettingsHandle): void;
}

"#;
