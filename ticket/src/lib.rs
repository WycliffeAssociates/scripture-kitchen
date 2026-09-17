//! `ticket` — the order slip the line and the pass both read from.
//!
//! ```text
//! onion/src/wire/schema.rs    the dish's declaration    ─┐
//! galley/src/toc/schema.rs    the census's declaration   ├─► ticket::emit ─► both ends
//! galley/src/find/…/schema.rs find's declaration        ─┘
//! ```
//!
//! Two things live here and only two: the vocabulary a declaration is built
//! from ([`schema`]), and the emitters that turn one record into one
//! language's half of it ([`emit`]). A format's own declaration, template,
//! envelope, magic, version, codegen bin and generated files stay with that
//! format — `ticket` never knows a format exists. See `README.md`.

pub mod emit;
pub mod schema;
pub mod stale;
