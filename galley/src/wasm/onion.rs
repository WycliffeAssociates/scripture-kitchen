//! Every `onion-wasm` door, on this module.
//!
//! ```text
//! import { parse, toUtf16, setExtensions, Galley } from "usfm-galley";
//!
//! toUtf16(text, 41)                  // the stateless onion door
//! setExtensions(list)                // …including the process-wide ones
//! new Galley().update(id, text)      // the resident sous handle
//! ```
//!
//! This module is why a host imports ONE package: anything onion can do at the
//! wall, galley does too. A door that lands only on `onion-wasm` is
//! unreachable from the superset build, which is the only one Sefer vendors.
//!
//! The doors are `onion-wasm`'s own shims under their own `js_name`s, not
//! wrappers: a wrapper would collide with them symbol for symbol, because
//! delegating to a door is what links the door.
//!
//! A cdylib keeps an rlib's object only where a symbol in it is wanted, and
//! `onion-wasm` is one module, so every shim, class, and describe section it
//! has rides in one codegen unit. [`DOOR`] wants one symbol out of that unit
//! and the rest arrive with it. `tests/sous_conformance.mjs` asserts the exact
//! export list, so a door that stopped arriving — `onion-wasm` split into
//! modules, say — fails there rather than going quiet.

/// The linker root. Never called; the reference is the point.
#[used]
static DOOR: fn(&str, u32) -> u32 = onion_wasm::to_utf16;

/// The same doors for a Rust caller — a test across the wall, or a native
/// consumer that wants the JS-shaped call without naming the second crate.
pub use onion_wasm::{
    Edits, FormatOpts, Splices, attr_resolve, attrs, book, diff, extensions_from_markers_ext,
    format, format_edits, format_edits_in, locate, mask, merge, merge_splices, parse,
    set_extensions, to_byte, to_utf16,
};
