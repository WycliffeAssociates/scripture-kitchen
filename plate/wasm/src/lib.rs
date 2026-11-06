//! The browser's plate: USFM → onion CST → Typst markup → Typst → PDF, all in
//! one wasm module, with Typst's embedded fonts only.

use std::sync::Arc;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Studio {
    fonts: Arc<plate::world::FontStore>,
    timings: String,
}

#[wasm_bindgen]
impl Studio {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Studio {
        Studio {
            fonts: plate::world::embedded_fonts(),
            timings: String::new(),
        }
    }

    /// Font families this module can set text in.
    pub fn fonts(&self) -> Vec<String> {
        plate::world::families(&self.fonts)
    }

    /// Compiles the books with `settings` (JSON) and `template` (usfm.typ's
    /// text) and returns the PDF. `timings()` then holds where the time went.
    pub fn render(&mut self, usfm: Vec<String>, settings: &str, template: &str) -> Result<Vec<u8>, JsError> {
        let (pdf, t) = plate::world::render_with(&self.fonts, &usfm, settings, template)
            .map_err(|e| JsError::new(&e))?;
        comemo::evict(10);
        self.timings = serde_json::to_string(&t).unwrap();
        Ok(pdf)
    }

    pub fn timings(&self) -> String {
        self.timings.clone()
    }
}

impl Default for Studio {
    fn default() -> Self {
        Self::new()
    }
}
