//! The JS doorway over the workflows layer — the stateful half of the wall.
//!
//! ```text
//! import init, { Galley, parse } from "usfm-galley";
//! import { FindingsSnapshot } from "sous-chef/reader.ts";
//!
//! const g = new Galley();                       // one handle, kept across edits
//! g.update("books/MRK.usfm", text);             // → "MRK"
//! FindingsSnapshot.open(g.publish());           // every finding in the corpus
//! deserialize(g.parse(text, true, true, true)); // clean chunks from the cache
//! deserialize(parse(text, true, true, true));   // the stateless door, same module
//! ```
//!
//! `onion-wasm`'s exports ride into this module as linked shims, so a consumer
//! imports ONE package and picks per call: the free functions compute from
//! scratch, [`Galley`] reuses what did not change. Nothing here is a new
//! rendering — [`Galley::parse`] returns the buffer `onion_wasm::parse`
//! returns, and [`Galley::publish`] returns the buffer the native
//! [`Expediter`] returns, byte for byte. Runbook and the equivalence claim:
//! `galley/src/wasm.md`.

use wasm_bindgen::prelude::*;

use sous_core::Brigade;
use sous_core::judge::{Channels, JudgingConfig};
use sous_core::proportionality::LengthConfig;

use crate::onion;
use crate::pantry::{BookId, Retain, Role};
use crate::sous::Expediter;

/// Products for roughly three of the largest books in the wild. Measured: the
/// biggest book in the corpus (en_ult PSA, 5.1 MB) is 3.27 MB warm, 4.62 MB
/// after fifty keystrokes. Deliberately modest — wasm linear memory grows and
/// never shrinks, so a budget is a permanent high-water mark, not a ceiling
/// the page falls back from.
const DEFAULT_BUDGET: usize = 16 << 20;

/// The corpus, resident: one [`Expediter`], one Pantry, one snapshot out.
///
/// One per project, not per document. Books go in whole by caller id and come
/// back as one complete publication; the Pantry inside owns the Warmer, so the
/// onion methods read the same warm chunks the analysis does.
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
    #[wasm_bindgen(js_name = updateReference)]
    pub fn update_reference(&mut self, id: &str, text: &str) -> Result<String, JsError> {
        self.sous
            .update_with(id, Role::Reference, Retain::ProductsOnly, text)
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

    // ── Judging ─────────────────────────────────────────────────────────

    /// A copy of the knobs the next [`publish`](Self::publish) judges with.
    pub fn config(&self) -> Knobs {
        Knobs::from(&self.sous.config().1)
    }

    /// Replaces them. No chapter is remapped and no book refolded — judging
    /// reads the config, mapping does not — so a knob flip costs a re-judge.
    #[wasm_bindgen(js_name = setConfig)]
    pub fn set_config(&mut self, knobs: Knobs) {
        let ((), mut substrate, mut words) = *self.sous.config();
        knobs.apply(&mut substrate);
        knobs.apply(&mut words);
        self.sous.set_config(((), substrate, words));
    }

    // ── Onion products, off the retained chunks ─────────────────────────

    /// The book, plated — the same buffer `onion_wasm::parse` returns, with
    /// the lex, the tree and the lint walk reused for every chunk whose bytes
    /// did not change.
    ///
    /// Read it with the same `reader.ts` the stateless door's output uses:
    /// nothing here is a new rendering, only a cheaper route to the same bytes.
    pub fn parse(&mut self, text: &str, diagnostics: bool, toc: bool, utf16: bool) -> Vec<u8> {
        self.sous.warmer_mut().parse(
            text,
            onion::wire::ParseOptions {
                diagnostics,
                toc,
                utf16,
            },
        )
    }

    /// The verse text alone, as one string — the reading a downstream text
    /// consumer wants, off the same reused ingredients.
    ///
    /// TODO: this DISCARDS the mask. `Mask` carries `ranges`/`starts` — the map
    /// from a masked offset back to the source — and sous needs it to report a
    /// finding against the unmasked document. Returning the text alone means
    /// whatever consumes this cannot get back.
    #[wasm_bindgen(js_name = verseText)]
    pub fn verse_text(&mut self, text: &str) -> String {
        let mask = self
            .sous
            .warmer_mut()
            .masked(text, &onion::mask::Filter::verse_text());
        mask.text(text.as_bytes())
    }

    /// The structure recipe's text, the verse-text mask's sibling.
    #[wasm_bindgen(js_name = structureText)]
    pub fn structure_text(&mut self, text: &str) -> String {
        let mask = self
            .sous
            .warmer_mut()
            .masked(text, &onion::mask::Filter::structure());
        mask.text(text.as_bytes())
    }

    // ── Telemetry, not contract ─────────────────────────────────────────

    /// Chunk units computed rather than reused, cumulative.
    pub fn misses(&self) -> f64 {
        self.sous.pantry().warmer().misses() as f64
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
        self.sous.pantry().warmer().len() as f64
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
    /// because they were registered while `knobs.source_copy` was off and so
    /// kept no word lane.
    ///
    /// Nonzero after turning the lane on means "re-send those references'
    /// text", not "nothing was found".
    #[wasm_bindgen(js_name = lastWordlessReferences)]
    pub fn last_wordless_references(&self) -> f64 {
        self.sous.last_wordless_references() as f64
    }
}

/// The judging knobs that cross the wall: every plain scalar of
/// [`JudgingConfig`], flat, so bindgen writes the getters and setters and JS
/// assigns `knobs.casing = false`.
///
/// Not on the wall: `bands` and `word_bands` (a `Staircase` is a validated
/// ladder, not a plain field), `letters` and `doubles` (tri-state policies),
/// and the roster bounds. They have no plain-field shape and no consumer has
/// asked for them; everything not a knob keeps the current config's value
/// through [`apply`](Knobs::apply), so widening this later breaks nothing.
#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Knobs {
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

impl Knobs {
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

impl Default for Knobs {
    fn default() -> Self {
        Self::from(&JudgingConfig::default())
    }
}
