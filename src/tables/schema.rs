//! Row types the authored table is written in. DRAFT — fields land with
//! the category-by-category audit; the full column inventory and bit
//! budget (~77 bits + side arrays) is in planning/NEXT-STEPS.md
//! "MarkerRow schema notes".
//!
//! Rules of the schema:
//! - Named fields only. Packing (u128 rows / u64+u32 lanes / windows) is
//!   codegen OUTPUT, never authored.
//! - One row per CANONICAL marker; numbered spellings collapse (the digit
//!   lives in the token's span, validated against `numbered_max`).
//! - Strings that can't be bits become side arrays: marker names,
//!   default-attribute values (6 distinct), doc paths (codegen-only).

/// One authored marker row. DRAFT: columns are added as the audit reaches
/// their category — do not bulk-fill ahead of the audit.
pub struct MarkerRow {
    /// Canonical name, digits stripped (`"q"`, not `"q1"`). Longest spec
    /// name is 6 bytes — must fit a u64 load.
    pub marker: &'static str,
    // TODO(audit, in landing order — see NEXT-STEPS schema notes):
    //   kind            (def-level: Paragraph/Character/Note/Milestone/... ~4 bits)
    //   ws_after_name   (delimiter fold rule — kills the ScanMode TODO)
    //   payload         (None | BookCode | NumberRange — onion: id / c,cp,ca,v,vp,va)
    //   numbered_max    (0 = unnumbered, 1..14 = cap, 15 = unbounded)
    //   allowed_contexts (SpecContext bitmask, 20 variants)
    //   takes_attributes / default_attribute (side-array index)
    //   closing / scope / family / paragraph_category / deprecated
    //   html_element    (render class, ≤16; numbered markers store the CLASS,
    //                    export derives the level from the span's digits)
    //   priority        (codegen-only: fast-path prelude ordering)
}
