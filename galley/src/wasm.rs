//! The JS doorway over the workflows layer — the stateful half of the wall.
//!
//! ```text
//! import init, { Galley, analyze } from "usfm-galley";
//!
//! const g = new Galley();           // one handle, kept across keystrokes
//! g.analyze(text, wantsAll());      // clean chapters served from the cache
//! analyze(text, wantsAll());        // the stateless engine door, same module
//! ```
//!
//! `onion-wasm`'s exports ride into this module as linked shims, so a consumer
//! imports ONE package and picks per call: the free functions compute from
//! scratch, [`Galley`] reuses what did not change. Nothing here is a new
//! rendering — [`Galley::analyze`] returns the object `onion_wasm::analyze`
//! returns, built by the same emit.

use wasm_bindgen::prelude::*;

use crate::FoldCache;
use crate::onion;

/// Products for roughly three of the largest books in the wild. Measured: the
/// biggest book in the corpus (en_ult PSA, 5.1 MB) is 3.27 MB warm, 4.62 MB
/// after fifty keystrokes. Deliberately modest — wasm linear memory grows and
/// never shrinks, so a budget is a permanent high-water mark, not a ceiling
/// the page falls back from.
const DEFAULT_BUDGET: usize = 16 << 20;

/// A kitchen that keeps its prep: the per-chapter fold, held across calls.
///
/// One per document. The cache keys on chapter CONTENT, so reordering
/// chapters, undoing an edit, or reopening a book all hit.
#[wasm_bindgen]
pub struct Galley {
    cache: FoldCache,
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
            cache: FoldCache::new(budget),
        }
    }

    /// Every read `onion_wasm::analyze` returns, with the lex, the tree and the
    /// lint walk reused for every chapter whose bytes did not change.
    ///
    /// `text` must be LF-normalized — the same contract the stateless door
    /// documents, and the one the chapter checksums are stable against.
    pub fn analyze(&mut self, text: &str, wants: u32) -> js_sys::Object {
        onion_wasm::object(&self.cache.analyze(text, wants))
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
        let mask = self.cache.masked(text, &onion::mask::Filter::verse_text());
        mask.text(text.as_bytes())
    }

    /// The structure recipe's text, the verse-text mask's sibling.
    #[wasm_bindgen(js_name = structureText)]
    pub fn structure_text(&mut self, text: &str) -> String {
        let mask = self.cache.masked(text, &onion::mask::Filter::structure());
        mask.text(text.as_bytes())
    }

    /// Units computed rather than reused, cumulative — the number that says
    /// whether a call hit. Telemetry, not a contract.
    pub fn misses(&self) -> f64 {
        self.cache.misses() as f64
    }

    /// Resident product bytes, the value the budget bounds.
    #[wasm_bindgen(js_name = residentBytes)]
    pub fn resident_bytes(&self) -> f64 {
        self.cache.resident_bytes() as f64
    }

    /// Cached units. Zero for a one-chapter book: galley does not cache what it
    /// cannot reuse.
    #[wasm_bindgen(js_name = entryCount)]
    pub fn entry_count(&self) -> f64 {
        self.cache.len() as f64
    }
}
