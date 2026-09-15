//! The JS doorway over the workflows layer — the stateful half of the wall.
//!
//! ```text
//! import init, { Galley, parse } from "usfm-galley";
//! import { FindingsSnapshot } from "usfm-galley/sous-reader";
//!
//! const g = new Galley();                       // one handle, kept across edits
//! g.update("books/MRK.usfm", text);             // → "MRK"
//! FindingsSnapshot.open(g.publish());           // every finding in the corpus
//! deserialize(g.parse("books/MRK.usfm", 1, 1, 1));  // off the retained copy
//! deserialize(parse(text, true, true, true));   // the stateless door, same module
//! ```
//!
//! Every `onion-wasm` door stands on this module too, linked by [`onion`], so
//! a consumer imports ONE package and picks per call: the free functions
//! compute from scratch, [`Galley`] reuses what did not change. Nothing here is
//! a new rendering — [`Galley::parse`] returns the buffer `onion_wasm::parse`
//! returns, and [`Galley::publish`] returns the buffer the native
//! [`Expediter`] returns, byte for byte. Runbook, the door list, and the
//! equivalence claim: `galley/src/wasm.md`.

use wasm_bindgen::prelude::*;

use sous_core::Brigade;
use sous_core::judge::{Channels, JudgingConfig};
use sous_core::proportionality::LengthConfig;

use crate::find::Find;
use crate::pantry::{BookId, Entry, Retain, Role};
use crate::sous::Expediter;

pub mod onion;

/// The two flags the find doors take, as the query Find prepares once.
///
/// `case_sensitive` rather than `case_insensitive` because that is the
/// checkbox a host draws; the engine's option is the negation of it.
fn query(needle: &str, case_sensitive: bool, whole_word: bool) -> Find<'_> {
    Find::literal(needle)
        .case_insensitive(!case_sensitive)
        .whole_word(whole_word)
}

/// Which roles `findAll`'s scope names, in the order their books are searched.
///
/// A string rather than a number, so the JS call reads as what it does; an
/// unknown one is an error rather than a silent fall back to the default.
fn scope_roles(scope: Option<String>) -> Result<&'static [Role], JsError> {
    match scope.as_deref().unwrap_or("targets") {
        "targets" => Ok(&[Role::Target]),
        "references" => Ok(&[Role::Reference]),
        "all" => Ok(&[Role::Target, Role::Reference]),
        other => Err(JsError::new(&format!(
            "unknown find scope {other:?}; expected \"targets\", \"references\" or \"all\""
        ))),
    }
}

/// Products for roughly three of the largest books in the wild. Measured: the
/// biggest book in the corpus (en_ult PSA, 5.1 MB) is 3.27 MB warm, 4.62 MB
/// after fifty keystrokes. Deliberately modest — wasm linear memory grows and
/// never shrinks, so a budget is a permanent high-water mark, not a ceiling
/// the page falls back from.
const DEFAULT_BUDGET: usize = 16 << 20;

/// The corpus, resident: one [`Expediter`], one Pantry, one snapshot out.
///
/// One per project, not per document. Books go in whole by caller id and come
/// back as one complete publication; the Pantry inside owns the chunk cache,
/// so the onion methods read the same warm chunks the analysis does.
#[wasm_bindgen]
pub struct Galley {
    sous: Expediter<Brigade>,
}

#[wasm_bindgen]
impl Galley {
    /// `budgetBytes` bounds resident products; omit it for 16 MB.
    #[wasm_bindgen(constructor)]
    pub fn new(budget_bytes: Option<f64>) -> Galley {
        let budget = budget_bytes
            .filter(|b| b.is_finite() && *b >= 0.0)
            .map_or(DEFAULT_BUDGET, |b| b as usize);
        Galley {
            sous: Expediter::new(Brigade::default(), budget),
        }
    }

    // ── The corpus ──────────────────────────────────────────────────────

    /// Register or replace one whole book under the caller's `id`, as a
    /// target: it keeps its text, and it publishes findings.
    ///
    /// Returns the `\id` line's canonical book code — `"MRK"` — which is what
    /// orders the publication. Idempotent: the same text costs a checksum.
    ///
    /// `text` must be LF-normalized, the contract every door here documents.
    pub fn update(&mut self, id: &str, text: &str) -> Result<String, JsError> {
        self.sous
            .update(id, Role::Target, text)
            .map(|key| key.to_string())
            .map_err(|error| JsError::new(&error.to_string()))
    }

    /// The same, as a declared source: one projected grapheme length per
    /// verse and no text at all, so a reference costs a fraction of a target.
    ///
    /// A reference publishes no findings of its own; it is the denominator
    /// the length lane compares a target's verses against.
    ///
    /// `keepText` — omitted is `false` — makes it keep the text and the
    /// projection a target keeps too, which is what [`find`](Self::find) and
    /// `findAll`'s `"references"` scope read. It costs what a target costs
    /// minus the resident analysis; a source nobody searches should stay off
    /// it.
    #[wasm_bindgen(js_name = updateReference)]
    pub fn update_reference(
        &mut self,
        id: &str,
        text: &str,
        keep_text: Option<bool>,
    ) -> Result<String, JsError> {
        let retain = match keep_text.unwrap_or(false) {
            true => Retain::Text,
            false => Retain::ProductsOnly,
        };
        self.sous
            .update_with(id, Role::Reference, retain, text)
            .map(|key| key.to_string())
            .map_err(|error| JsError::new(&error.to_string()))
    }

    /// Drop a book, its text, and its cached rows. `false` when the id was
    /// never registered.
    pub fn remove(&mut self, id: &str) -> bool {
        self.sous.remove(&BookId::from(id))
    }

    /// One complete corpus publication over every target, in canonical book
    /// order, in raw-book UTF-16 — the buffer `FindingsSnapshot.open` reads.
    ///
    /// A snapshot replaces the previous one whole; row positions are valid
    /// only inside the buffer they came from.
    pub fn publish(&mut self) -> Result<Vec<u8>, JsError> {
        self.sous
            .publish()
            .map_err(|error| JsError::new(&error.to_string()))
    }

    // ── Find ────────────────────────────────────────────────────────────

    /// Every hit of `needle` in ONE registered book's verse-text projection,
    /// as the find buffer ([`crate::find::wire`] and `wasm.md` state the
    /// layout: magic and version, then little-endian `u32`, UTF-16 offsets,
    /// both coordinate spaces per hit).
    ///
    /// The search runs over the PROJECTION — what a reader sees — so a needle
    /// inside a footnote is not found, and a needle that spans one comes back
    /// as one source range per contiguous piece. That is the whole reason the
    /// buffer carries a piece count per hit.
    ///
    /// Literal only: `needle` is never a pattern. `whole_word` is the words
    /// rule galley restates in `find.md`; case-insensitive is the simple
    /// lowercase fold, not a collator. `limit` bounds hits across the whole
    /// call, and `0` means no bound. Any registered book that retains text and
    /// a projection may be searched — a target, or a reference registered with
    /// `keepText`. One that retains neither errors by name, because answering
    /// "no hits" would say it was clean.
    pub fn find(
        &mut self,
        id: &str,
        needle: &str,
        case_sensitive: bool,
        whole_word: bool,
        limit: u32,
    ) -> Result<Vec<u8>, JsError> {
        let wanted = BookId::from(id);
        if !self.sous.pantry().searchable(&wanted) {
            return Err(JsError::new(&match self.sous.pantry().role(&wanted) {
                None => format!("no book is registered as {id}"),
                Some(Role::Reference) => {
                    format!("reference {id} retains no text; register it with keepText")
                }
                Some(Role::Target) => format!("target {id} retains no verse-text projection"),
            }));
        }
        Ok(self
            .sous
            .find(&query(needle, case_sensitive, whole_word), &[wanted], limit))
    }

    /// The same over every searchable book in `scope`, in canonical book order
    /// — the project-wide find.
    ///
    /// `scope` is `"targets"` (the default when omitted), `"references"`, or
    /// `"all"`, which searches the targets and then the references. A
    /// reference registered without `keepText` is in no scope: it retains
    /// nothing to search, so it is not listed either.
    ///
    /// The buffer's `bookIndex` indexes its own id table, which names every
    /// book searched whether or not it matched, so a consumer never has to ask
    /// a second question to learn which book a hit is in.
    #[wasm_bindgen(js_name = findAll)]
    pub fn find_all(
        &mut self,
        needle: &str,
        case_sensitive: bool,
        whole_word: bool,
        limit: u32,
        scope: Option<String>,
    ) -> Result<Vec<u8>, JsError> {
        let ids: Vec<BookId> = scope_roles(scope)?
            .iter()
            .flat_map(|role| self.sous.pantry().books_with_text(*role))
            .collect();
        Ok(self
            .sous
            .find(&query(needle, case_sensitive, whole_word), &ids, limit))
    }

    // ── Judging ─────────────────────────────────────────────────────────

    /// A copy of the settings the next [`publish`](Self::publish) judges with.
    pub fn config(&self) -> SousSettings {
        SousSettings::from(&self.sous.config().1)
    }

    /// Replaces them. No chapter is remapped and no book refolded — judging
    /// reads the config, mapping does not — so a knob flip costs a re-judge.
    #[wasm_bindgen(js_name = setConfig)]
    pub fn set_config(&mut self, settings: SousSettings) {
        let ((), mut substrate, mut words) = *self.sous.config();
        settings.apply(&mut substrate);
        settings.apply(&mut words);
        self.sous.set_config(((), substrate, words));
    }

    // ── Onion products, off the retained copy ───────────────────────────

    /// One registered book's diagnostics, off its retained text.
    ///
    /// Onion has no lint door of its own: a lint report crosses the wall as
    /// the `diagnostics` section of a parse buffer, so this is
    /// [`parse`](Self::parse) with that section alone asked for, read by the
    /// same `reader.ts`.
    pub fn lint(&mut self, id: &str) -> Result<Vec<u8>, JsError> {
        self.book_parse(
            id,
            crate::onion::wire::ParseOptions {
                diagnostics: true,
                ..Default::default()
            },
        )
    }

    /// One registered book, plated — the same buffer `onion_wasm::parse`
    /// returns for that text, with the lex, the tree and the lint walk reused
    /// for every chunk whose bytes did not change.
    ///
    /// Read it with the same `reader.ts` the stateless door's output uses:
    /// nothing here is a new rendering, only a cheaper route to the same
    /// bytes. A book that retains no text refuses.
    pub fn parse(
        &mut self,
        id: &str,
        diagnostics: bool,
        toc: bool,
        utf16: bool,
    ) -> Result<Vec<u8>, JsError> {
        self.book_parse(
            id,
            crate::onion::wire::ParseOptions {
                diagnostics,
                toc,
                utf16,
            },
        )
    }

    /// One registered book's verse text, off the projection it already
    /// retains — no mask is cut and no text crosses in.
    ///
    /// A reference registered without its text retains no projection and
    /// refuses.
    #[wasm_bindgen(js_name = verseText)]
    pub fn verse_text(&mut self, id: &str) -> Result<String, JsError> {
        let book = self.book(id)?;
        let mask = book.mask().map_err(refusal)?;
        let text = book.text().map_err(refusal)?;
        Ok(mask.text(text.as_bytes()))
    }

    // ── The same products over loose text ───────────────────────────────

    /// [`parse`](Self::parse) over text the host holds and has not registered
    /// — a preview pane, a file not yet in the project.
    ///
    /// The chunk cache keys on content, so an unregistered copy of a
    /// registered book still hits; what it costs over the id door is the
    /// string crossing the wall.
    #[wasm_bindgen(js_name = parseText)]
    pub fn parse_text(&mut self, text: &str, diagnostics: bool, toc: bool, utf16: bool) -> Vec<u8> {
        self.sous.parse(
            text,
            crate::onion::wire::ParseOptions {
                diagnostics,
                toc,
                utf16,
            },
        )
    }

    /// The verse text of loose text. See [`parse_text`](Self::parse_text).
    ///
    /// TODO: this DISCARDS the mask. `Mask` carries `ranges`/`starts` — the map
    /// from a masked offset back to the source — and sous needs it to report a
    /// finding against the unmasked document. Returning the text alone means
    /// whatever consumes this cannot get back.
    #[wasm_bindgen(js_name = verseTextOf)]
    pub fn verse_text_of(&mut self, text: &str) -> String {
        let mask = self
            .sous
            .masked(text, &crate::onion::mask::Filter::verse_text());
        mask.text(text.as_bytes())
    }

    /// The structure recipe's text, the verse-text mask's sibling. No book
    /// retains a structure projection, so this door takes text only.
    #[wasm_bindgen(js_name = structureTextOf)]
    pub fn structure_text_of(&mut self, text: &str) -> String {
        let mask = self
            .sous
            .masked(text, &crate::onion::mask::Filter::structure());
        mask.text(text.as_bytes())
    }

    // ── Dirty and rework ────────────────────────────────────────────────

    /// Chunk starts plus one checksum each, and no text — the ~1 KB baseline
    /// user land keeps beside a file on disk.
    pub fn fingerprint(&self, text: &str) -> Fingerprint {
        Fingerprint(crate::pantry::fingerprint(text))
    }

    /// Ranges of `text` whose chunk this book's last update never saw — the
    /// chunks a host would have to re-derive, in `text`'s own byte offsets,
    /// as `from, to` pairs.
    ///
    /// `undefined` when the id is not registered, which is the question a
    /// host asks before deciding to register it.
    #[wasm_bindgen(js_name = changedSinceUpdate)]
    pub fn changed_since_update(&self, id: &str, text: &str) -> Option<Vec<u32>> {
        self.sous
            .pantry()
            .changed_since_update(&BookId::from(id), text)
            .map(|ranges| pairs(&ranges))
    }

    // ── Telemetry, not contract ─────────────────────────────────────────

    /// Chunk units computed rather than reused, cumulative.
    pub fn misses(&self) -> f64 {
        self.sous.pantry().chunk_stats().misses as f64
    }

    /// Resident bytes across the whole handle: the Pantry's texts and
    /// products, and the Expediter's own cached rows.
    #[wasm_bindgen(js_name = residentBytes)]
    pub fn resident_bytes(&self) -> f64 {
        self.sous.resident_bytes() as f64
    }

    /// Cached chunk units. Zero for a one-chapter book: galley does not cache
    /// what it cannot reuse.
    #[wasm_bindgen(js_name = entryCount)]
    pub fn entry_count(&self) -> f64 {
        self.sous.pantry().chunk_stats().len as f64
    }

    /// Chapters mapped by the last [`publish`](Self::publish).
    #[wasm_bindgen(js_name = lastMapped)]
    pub fn last_mapped(&self) -> f64 {
        self.sous.last_mapped() as f64
    }

    /// Of those, the ones that kept an observation and re-walked only part.
    #[wasm_bindgen(js_name = lastRemapped)]
    pub fn last_remapped(&self) -> f64 {
        self.sous.last_remapped() as f64
    }

    /// Books rescanned for sites rather than replaying cached rows.
    #[wasm_bindgen(js_name = lastLocated)]
    pub fn last_located(&self) -> f64 {
        self.sous.last_located() as f64
    }

    /// Targets re-paired against their declared source.
    #[wasm_bindgen(js_name = lastPaired)]
    pub fn last_paired(&self) -> f64 {
        self.sous.last_paired() as f64
    }

    /// Declared sources the source-copy lane would have read and could not,
    /// because they were registered while `settings.source_copy` was off and so
    /// kept no word lane.
    ///
    /// Nonzero after turning the lane on means "re-send those references'
    /// text", not "nothing was found".
    #[wasm_bindgen(js_name = lastWordlessReferences)]
    pub fn last_wordless_references(&self) -> f64 {
        self.sous.last_wordless_references() as f64
    }
}

impl Galley {
    /// One registered book, or the refusal an unknown id earns.
    fn book(&mut self, id: &str) -> Result<Entry<'_>, JsError> {
        self.sous
            .book(&BookId::from(id))
            .ok_or_else(|| JsError::new(&format!("no book is registered as {id}")))
    }

    fn book_parse(
        &mut self,
        id: &str,
        opts: crate::onion::wire::ParseOptions,
    ) -> Result<Vec<u8>, JsError> {
        self.book(id)?.parse(opts).map_err(refusal)
    }
}

/// A Pantry refusal, as the JS error carrying its text.
fn refusal(error: crate::pantry::PantryError) -> JsError {
    JsError::new(&error.to_string())
}

/// Byte ranges as the flat `from, to` pairs a `Uint32Array` carries.
fn pairs(ranges: &[core::ops::Range<u32>]) -> Vec<u32> {
    ranges
        .iter()
        .flat_map(|range| [range.start, range.end])
        .collect()
}

/// One book's chunk starts and their checksums — no text, ~1 KB, opaque.
///
/// Dirty and rework are two questions, and this answers both separately:
/// [`differs_from`](Self::differs_from) is positional, so a chapter that only
/// moved is dirty; [`changed_chunks`](Self::changed_chunks) is set membership
/// over the checksums, so that same chapter needs no rework.
///
/// A handle, so JS frees it: `print.free()` when the baseline is dropped.
#[wasm_bindgen]
pub struct Fingerprint(crate::pantry::Fingerprint);

#[wasm_bindgen]
impl Fingerprint {
    /// Whether the two byte strings differ at all.
    #[wasm_bindgen(js_name = differsFrom)]
    pub fn differs_from(&self, current: &Fingerprint) -> bool {
        self.0.differs_from(&current.0)
    }

    /// Ranges of `current`'s text whose chunk this fingerprint never saw, as
    /// flat `from, to` pairs.
    #[wasm_bindgen(js_name = changedChunks)]
    pub fn changed_chunks(&self, current: &Fingerprint) -> Vec<u32> {
        pairs(&self.0.changed_chunks(&current.0))
    }

    /// Chunks in the text this fingerprint was taken from.
    #[wasm_bindgen(js_name = chunkCount)]
    pub fn chunk_count(&self) -> u32 {
        self.0.chunk_count() as u32
    }
}

/// The judging settings that cross the wall: every plain scalar of
/// [`JudgingConfig`], flat, so bindgen writes the getters and setters and JS
/// assigns `settings.casing = false`.
///
/// Not on the wall: `bands` and `word_bands` (a `Staircase` is a validated
/// ladder, not a plain field), `letters` and `doubles` (tri-state policies),
/// and the roster bounds. They have no plain-field shape and no consumer has
/// asked for them; everything not a knob keeps the current config's value
/// through [`apply`](SousSettings::apply), so widening this later breaks nothing.
#[wasm_bindgen]
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
    // Thresholds.
    pub support_floor: u32,
    pub word_support_floor: u32,
    pub terminal_upper_share_bp: u16,
    pub sentence_start_upper_bp: u16,
    pub word_length_sigma: u8,
    pub doubles_productive_bp: u16,
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
            support_floor: config.support_floor,
            word_support_floor: config.word_support_floor,
            terminal_upper_share_bp: config.terminal_upper_share_bp,
            sentence_start_upper_bp: config.sentence_start_upper_bp,
            word_length_sigma: config.word_length_sigma,
            doubles_productive_bp: config.doubles_productive_bp,
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
        };
        config.support_floor = self.support_floor;
        config.word_support_floor = self.word_support_floor;
        config.terminal_upper_share_bp = self.terminal_upper_share_bp;
        config.sentence_start_upper_bp = self.sentence_start_upper_bp;
        config.word_length_sigma = self.word_length_sigma;
        config.doubles_productive_bp = self.doubles_productive_bp;
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
