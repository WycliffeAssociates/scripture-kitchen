//! The authored marker table. EMPTY until the mechanical translation of
//! onion's marker_defs_data lands (planning/onion_reference/), after which
//! rows are audited category-by-category: paragraphs, character markers,
//! notes, milestones, meta. ~170 canonical rows expected (219 spellings −
//! 68 numbered variants + their ~19 canonical bases).
//!
//! Audit discipline: a row enters this file only after its category's
//! audit sitting — the translation script's output goes in a scratch file,
//! not here, so this file is always all-audited.

use super::schema::MarkerRow;

pub static ROWS: &[MarkerRow] = &[];
