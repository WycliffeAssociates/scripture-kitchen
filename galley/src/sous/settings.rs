//! The judging settings a host sets from JavaScript, and the typed
//! `sous-settings.ts` generated from them.
//!
//! ```text
//! settings! { doubled_bare: bool = doubled.bare => Switch, Rules, "Doubled with a space", "Flags …"; … }
//!   → SousSettings { doubled_bare: bool, … }      flat, the wasm class
//!   → from / apply                                 config.doubled.bare <-> settings.doubled_bare
//!   → sous-settings.ts
//!       export type SousCheck = "ExactNeighbor" | … | "LengthProportionality";
//!       export const SOUS_SETTINGS = { doubled: {…}, doubled_bare: { check: "Doubled", kind: "switch", … }, … };
//!       export function checkOf(code: SousCode): SousCheck | null;
//! ```
//!
//! A setting's check is where its path lives in [`JudgingConfig`]: the first
//! segment naming a check (`doubled.bare`, `channels.doubled`) decides, and
//! `shared` and `words` are general. The table lists check by check, each
//! check's own switch first, then its sub-switches, then its thresholds,
//! general last; the generator refuses any other order.
//!
//! `cargo run -p usfm_galley --bin codegen` writes it;
//! `tests/codegen_output_matches_input.rs` fails when it is stale.

use core::fmt::Write as _;

use sous_core::judge::{Channel, JudgingConfig};
use sous_core::{Check, finding_codes};

/// One row per setting: its key, its type, its path in [`JudgingConfig`],
/// then its page entry. Defines [`SousSettings`], its conversions, and
/// [`SETTINGS`], so a path that names no field does not compile.
macro_rules! settings {
    ($(
        $key:ident: $ty:ty = $($seg:ident).+ => $kind:ident, $group:ident,
            $label:literal,
            $description:literal $(, min $min:literal)?;
    )*) => {
        /// The judging settings that cross the wall, flat, so bindgen writes
        /// the getters and setters and JS assigns `settings.casing = false`.
        ///
        /// Not on the wall: the two ladders' bands (a `Staircase` is a
        /// validated ladder, not a plain field), the tri-state policies, and
        /// the rarity roster. No consumer has asked for them; everything not a
        /// knob keeps the current config's value through
        /// [`apply`](SousSettings::apply), so widening this later breaks
        /// nothing.
        #[cfg_attr(feature = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct SousSettings {
            $(pub $key: $ty,)*
        }

        impl SousSettings {
            /// Every knob as this config holds it.
            pub fn from(config: &JudgingConfig) -> Self {
                Self { $($key: config.$($seg).+,)* }
            }

            /// Writes them back, leaving every field that is not a knob alone.
            pub fn apply(self, config: &mut JudgingConfig) {
                $(config.$($seg).+ = self.$key;)*
            }

            fn values(self) -> Vec<Value> {
                vec![$(Value::from(self.$key)),*]
            }
        }

        /// Every field of [`SousSettings`], in declaration order.
        pub const SETTINGS: &[Setting] = &[$(
            Setting {
                key: stringify!($key),
                path: &[$(stringify!($seg)),+],
                kind: Kind::$kind,
                group: Group::$group,
                label: $label,
                description: $description,
                min: settings!(@min $($min)?),
            },
        )*];
    };
    (@min) => { None };
    (@min $min:literal) => { Some($min) };
}

settings! {
    placement: bool = channels.placement => Switch, Rules,
        "What a mark touches",
        "Flags a punctuation mark or digit right next to something it is rarely next to in this project, like a comma touching the next word, by counting every use of that mark across the whole project.";
    run_shape: bool = channels.run_shape => Switch, Rules,
        "Groups of marks",
        "Flags a group of marks written together, like “;'”, when this project rarely groups that mark that way and does not write that exact group often.";
    exact_neighbor: bool = channels.exact_neighbor => Switch, Rules,
        "Pairs of marks",
        "Flags two marks side by side, like “?,”, when this project almost never puts the second one right after the first.";
    pooled_neighbor: bool = channels.pooled_neighbor => Switch, Rules,
        "A mark and the kind of mark after it",
        "Flags a mark followed directly by a kind of mark, such as a quotation mark or a dash, that rarely follows it in this project; off by default because the pairs check already covers it.";
    rarity: bool = channels.rarity => Switch, Rules,
        "Rare characters",
        "Lists characters this project uses only a handful of times in all its books, like a curly apostrophe in a project that otherwise uses straight ones.";
    casing: bool = channels.casing => Switch, Rules,
        "Capital letters",
        "Flags a word capitalized, or not, in a way this project rarely writes that word in the middle of a sentence, like “On” where the project writes “on”.";
    terminal_upper_share_bp: u16 = casing.terminal_upper_share_bp => ShareBp, Thresholds,
        "Capital after a mark",
        "How often this project must follow a mark with a capital before a capital there counts as the mark's doing and not the word's.";
    terminal_lower_share_bp: u16 = casing.terminal_lower_share_bp => ShareBp, Thresholds,
        "Lowercase after a mark",
        "How rarely a mark may be followed by a capital for a capital after it to be checked; marks between this and the capital setting are left alone.";
    sentence_start: bool = channels.sentence_start => Switch, Rules,
        "Lowercase after a sentence end",
        "Flags a lowercase word right after a mark, like “?” or “!”, that this project almost always follows with a capital.";
    sentence_start_upper_bp: u16 = sentence_start.upper_bp => ShareBp, Thresholds,
        "Capital expected after a mark",
        "How often this project must follow a mark with a capital before a lowercase word after that mark is flagged.";
    word_length: bool = channels.word_length => Switch, Rules,
        "Very long words",
        "Flags words far longer than the words this project usually uses, which can mean two words ran together; off by default because long names are common.";
    word_length_sigma: u8 = word_length.sigma => Count, Thresholds,
        "How long a very long word is",
        "How far past this project's usual word length a word must be, counted in steps of how much word lengths usually vary, before the long-word check flags it.",
        min 1.0;
    doubled: bool = channels.doubled => Switch, Rules,
        "Doubled words",
        "Turns every check for a word written twice in a row on or off.";
    doubled_bare: bool = doubled.bare => Switch, Rules,
        "Doubled with a space",
        "Flags a word written twice in a row with only a space between, like “the the”, unless this project does that with the word often.";
    doubled_separated: bool = doubled.separated => Switch, Rules,
        "Doubled with punctuation",
        "Flags a word written twice with punctuation between, like “Moses, Moses”, unless this project does that with the word often; off by default because calling someone by name twice is common.";
    doubles_productive_bp: u16 = doubled.productive_bp => ShareBp, Thresholds,
        "Repeating language",
        "When more than this share of the different words in the project are written twice in a row more than once, the project is taken to repeat words on purpose and the doubled-word checks stay silent.";
    letter_runs: bool = channels.letter_runs => Switch, Rules,
        "Repeated letters",
        "Flags a letter written more times in a row than this project ever writes it, like “joyfullly”.";
    book_rate: bool = channels.book_rate => Switch, Rules,
        "One book differs",
        "Flags one book that puts a mark next to something far more often than the project's other books do, like a dash after a space in one book only.";
    book_rate_ratio: u16 = book_rate.ratio => Count, Thresholds,
        "One book: how many times more",
        "How many times more often than the other books typically do one book must put a mark next to something before the one-book check flags it.",
        min 1.0;
    book_rate_min_bp: u16 = book_rate.min_bp => ShareBp, Thresholds,
        "One book: least share",
        "The least share of a mark's uses in one book that must be next to the same thing before the one-book check flags that book.";
    book_rate_min_uses: u32 = book_rate.min_uses => Count, Thresholds,
        "One book: least uses",
        "How many times a book must use a mark before the one-book check may flag that book for it.",
        min 1.0;
    lengths_enabled: bool = lengths.proportionality.enabled => Switch, Reference,
        "Verse lengths",
        "Compares each verse's length with the same verse in the source text and flags verses much longer or shorter than this project usually is.";
    z_long: f32 = lengths.proportionality.z_long => Decimal, Reference,
        "Long verse distance",
        "How far past this project's usual length compared with the source a verse must be, on the long side, before it is flagged as much longer.",
        min 0.0;
    z_short: f32 = lengths.proportionality.z_short => Decimal, Reference,
        "Short verse distance",
        "How far short of this project's usual length compared with the source a verse must be before it is flagged as much shorter.",
        min 0.0;
    min_verses: u32 = lengths.proportionality.min_verses => Count, Reference,
        "Verses to compare a book",
        "How many verses a book must share with the source before its verse lengths are compared within that book as well as across the project.";
    presence: bool = lengths.presence => Switch, Reference,
        "Missing and extra verses",
        "Flags verses the source text has and this book lacks, verses this book has and the source lacks, and verses left empty.";
    source_copy: bool = lengths.source_copy.enabled => Switch, Reference,
        "Copied source words",
        "Flags words in a row spelled exactly as in the same verse of the source text, which can mean source text was pasted in; off by default.";
    source_copy_min_run: u32 = lengths.source_copy.min_run => Count, Reference,
        "Copied words in a row",
        "How many words in a row must match the source verse before the copied-words check flags them.",
        min 2.0;
    support_floor: u32 = shared.floor => Count, Thresholds,
        "Habit count",
        "How many times something must occur before the checks treat it as a habit of this project rather than a one-off.",
        min 1.0;
    word_support_floor: u32 = words.floor => Count, Thresholds,
        "Word habit count",
        "How many times a word, or a run of one letter, must occur before the capital-letter, long-word and repeated-letter checks compare it with the rest of this project.",
        min 1.0;
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

/// What kind of setting it is.
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
pub struct Setting {
    pub key: &'static str,
    /// Its field in [`JudgingConfig`], one segment per struct.
    pub path: &'static [&'static str],
    pub kind: Kind,
    pub group: Group,
    pub label: &'static str,
    pub description: &'static str,
    /// The least a page offers; an integer's type bounds it where this does
    /// not.
    pub min: Option<f64>,
}

/// The config segment each check owns. `shared` and `words` are the ladders
/// several checks read; `channels` and `lengths` only hold checks.
const SEGMENTS: [(&str, Option<Check>); 16] = [
    ("shared", None),
    ("words", None),
    (
        "exact_neighbor",
        Some(Check::Convention(Channel::ExactNeighbor)),
    ),
    (
        "pooled_neighbor",
        Some(Check::Convention(Channel::PooledNeighbor)),
    ),
    ("run_shape", Some(Check::Convention(Channel::RunShape))),
    ("placement", Some(Check::Convention(Channel::Placement))),
    ("rarity", Some(Check::Convention(Channel::Rarity))),
    ("casing", Some(Check::Convention(Channel::Casing))),
    ("word_length", Some(Check::Convention(Channel::WordLength))),
    ("doubled", Some(Check::Convention(Channel::Doubled))),
    ("letter_runs", Some(Check::Convention(Channel::LetterRun))),
    (
        "sentence_start",
        Some(Check::Convention(Channel::SentenceStart)),
    ),
    ("book_rate", Some(Check::Convention(Channel::BookRate))),
    ("presence", Some(Check::Presence)),
    ("source_copy", Some(Check::SourceCopy)),
    ("proportionality", Some(Check::LengthProportionality)),
];

/// Where a setting stands within its check: the check's own switch, a switch
/// inside it, or a threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Switch,
    SubSwitch,
    Threshold,
}

impl Setting {
    /// The check this setting tunes, `None` when several share it.
    ///
    /// # Panics
    ///
    /// When no segment of its path is in [`SEGMENTS`].
    pub fn check(&self) -> Option<Check> {
        self.path
            .iter()
            .find_map(|segment| SEGMENTS.iter().find(|(name, _)| name == segment))
            .unwrap_or_else(|| {
                panic!(
                    "`{}` lives at `{}`, which names no check and is not shared",
                    self.key,
                    self.path.join(".")
                )
            })
            .1
    }

    fn rank(&self) -> Rank {
        let last = self.path[self.path.len() - 1];
        let owns = SEGMENTS
            .iter()
            .any(|(name, check)| check.is_some() && *name == last);
        match self.kind {
            Kind::Switch if owns || last == "enabled" => Rank::Switch,
            Kind::Switch => Rank::SubSwitch,
            _ => Rank::Threshold,
        }
    }
}

/// `SousCheck`'s spelling of a setting's check.
fn check_name(check: Option<Check>) -> &'static str {
    check.map_or("general", Check::name)
}

/// Panics unless [`SETTINGS`] lists check by check in the promised order and
/// every check has exactly one switch.
fn assert_order() {
    let mut seen: Vec<Option<Check>> = Vec::new();
    let mut last: Option<(Option<Check>, Rank)> = None;
    for setting in SETTINGS {
        let (check, rank) = (setting.check(), setting.rank());
        match last {
            Some((previous, before)) if previous == check => assert!(
                check.is_none() || (before <= rank && rank != Rank::Switch),
                "`{}` is out of order within {}: switch, sub-switches, then thresholds",
                setting.key,
                check_name(check)
            ),
            _ => {
                assert!(
                    !seen.contains(&check),
                    "`{}`'s check {} is split across the table",
                    setting.key,
                    check_name(check)
                );
                assert!(
                    !seen.contains(&None),
                    "`{}` follows the general settings, which come last",
                    setting.key
                );
                assert!(
                    check.is_none() || rank == Rank::Switch,
                    "{} does not open with its own switch",
                    check_name(check)
                );
                seen.push(check);
            }
        }
        last = Some((check, rank));
    }
    for check in Check::ALL {
        assert!(
            seen.contains(&Some(check)),
            "{} has no switch",
            check.name()
        );
    }
}

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

macro_rules! value_from {
    ($($ty:ty => $variant:ident),*) => {
        $(impl From<$ty> for Value {
            fn from(value: $ty) -> Self {
                Value::$variant(value)
            }
        })*
    };
}

value_from!(bool => Bool, u8 => U8, u16 => U16, u32 => U32, f32 => F32);

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

/// `sous-settings.ts`, from [`SETTINGS`].
///
/// # Panics
///
/// On a kind the field's type cannot hold, a description that is not one
/// sentence or names a unit, a setting whose check cannot be told from its
/// path, or a table out of check order.
#[must_use]
pub fn settings_ts() -> String {
    assert_order();
    let rows: Vec<(&str, Value, &Setting)> = SETTINGS
        .iter()
        .zip(SousSettings::default().values())
        .map(|(entry, value)| {
            let key = entry.key;
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
    let checks: Vec<String> = Check::ALL.iter().map(|check| quote(check.name())).collect();
    let _ = writeln!(
        ts,
        "/** What a settings page groups by: one per convention channel, one per\n * source-compared lane. */\nexport type SousCheck =\n  | {};\n",
        checks.join("\n  | ")
    );
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
            Kind::ShareBp => Some(10_000.0),
            _ => value.type_max(),
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
            "  {key}: {{\n    key: {},\n    check: {},\n    kind: \"{kind}\",\n    default: {},{bounds}\n    group: \"{group}\",\n    label: {},\n    description:\n      {},\n  }},",
            quote(key),
            quote(check_name(entry.check())),
            value.literal(),
            quote(entry.label),
            quote(entry.description),
        );
    }
    ts.push_str("};\n\n");
    ts.push_str("const CHECKS: { readonly [C in SousCode]: SousCheck | null } = {\n");
    for code in finding_codes() {
        let check = code
            .check
            .map_or("null".to_string(), |check| quote(check.name()));
        let _ = writeln!(ts, "  {}: {check},", quote(&code.code));
    }
    ts.push_str("};\n\n");
    ts.push_str(
        "/** The check a finding code belongs to; `null` for hygiene, which no setting tunes. */\n",
    );
    ts.push_str("export function checkOf(code: SousCode): SousCheck | null {\n  return CHECKS[code];\n}\n\n");
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
 * SOUS_SETTINGS.doubled_bare.check          // "Doubled"
 * checkOf("convention.Doubled")             // "Doubled": a finding links to its settings
 * ```
 *
 * `SOUS_SETTINGS` iterates check by check: each check's own switch, then its
 * sub-switches, then its thresholds; the settings several checks share
 * (`check: "general"`) come last. `group` says only what kind of setting it is.
 *
 * A setting kitchen adds, removes, or retypes changes `SousSettingsValues`, so
 * a host that restates it fails to compile rather than missing it.
 */

import type { SousSettings as BundlerSettings } from "./pkg-bundler/usfm_galley.js";
import type { SousSettings as WebSettings } from "./pkg-web/usfm_galley.js";
import type { SousCode } from "./sous-reader.ts";

"#;

const SPEC: &str = r#"export type SettingKind = "switch" | "count" | "share-bp" | "decimal";
export type SettingGroup = "rules" | "thresholds" | "reference";

export interface SettingSpec<K extends SettingKey> {
  readonly key: K;
  /** The check it tunes, or "general" when several share it. */
  readonly check: SousCheck | "general";
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

#[cfg(test)]
mod tests {
    use super::*;

    fn check_of(key: &str) -> &'static str {
        let setting = SETTINGS.iter().find(|setting| setting.key == key).unwrap();
        check_name(setting.check())
    }

    #[test]
    fn a_setting_tunes_the_check_its_path_lives_in() {
        assert_eq!(check_of("doubled"), "Doubled");
        assert_eq!(check_of("doubled_bare"), "Doubled");
        assert_eq!(check_of("doubles_productive_bp"), "Doubled");
        assert_eq!(check_of("terminal_upper_share_bp"), "Casing");
        assert_eq!(check_of("letter_runs"), "LetterRun");
        assert_eq!(check_of("lengths_enabled"), "LengthProportionality");
        assert_eq!(check_of("presence"), "Presence");
        assert_eq!(check_of("source_copy_min_run"), "SourceCopy");
        assert_eq!(check_of("support_floor"), "general");
        assert_eq!(check_of("word_support_floor"), "general");
    }

    #[test]
    #[should_panic(expected = "names no check and is not shared")]
    fn a_path_naming_no_check_is_refused() {
        let stray = Setting {
            key: "stray",
            path: &["lengths", "stray"],
            kind: Kind::Count,
            group: Group::Reference,
            label: "",
            description: "",
            min: None,
        };
        stray.check();
    }

    #[test]
    fn every_finding_code_but_hygiene_has_a_check_with_settings() {
        for code in finding_codes() {
            assert_eq!(
                code.check.is_none(),
                code.code.starts_with("hygiene."),
                "{}",
                code.code
            );
            if let Some(check) = code.check {
                assert!(
                    SETTINGS
                        .iter()
                        .any(|setting| setting.check() == Some(check)),
                    "{} has no settings",
                    code.code
                );
            }
        }
        for check in Check::ALL {
            assert!(
                finding_codes().iter().any(|code| code.check == Some(check)),
                "no finding code is {}'s",
                check.name()
            );
        }
    }

    #[test]
    fn every_setting_round_trips_through_its_path() {
        let mut moved = SousSettings::default();
        moved.placement = !moved.placement;
        moved.doubled_separated = !moved.doubled_separated;
        moved.terminal_lower_share_bp += 1;
        moved.source_copy_min_run += 1;
        moved.z_short += 1.0;
        moved.word_support_floor += 1;
        let mut config = JudgingConfig::default();
        moved.apply(&mut config);
        assert_eq!(SousSettings::from(&config), moved);
        assert!(config.doubled.separated);
        assert_eq!(config.words.floor, 21);
        assert_eq!(config.lengths.source_copy.min_run, 4);
    }
}
