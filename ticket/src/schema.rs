//! The wire vocabulary: what a record and its fields ARE.
//!
//! ```text
//! <format>::schema::RECORDS  ->  ticket::emit::writers_rs      ->  a Rust writer
//!                            ->  ticket::emit::row_classes_ts  ->  a TS reader
//! ```
//!
//! A field declares four things: what it is called on the wire, how wide it is,
//! whether it is a SOURCE OFFSET (a position in text, which some formats
//! convert), and the expression a writer uses to reach it on the engine type.
//! Nothing here describes layout beyond field order — a wire is little-endian
//! by construction, never a cast of a Rust struct.
//!
//! No format's own declaration lives here. See `ticket/README.md`.

/// How many bytes a field occupies on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    U8,
    U16,
    U32,
}

impl Width {
    pub const fn bytes(self) -> usize {
        match self {
            Width::U8 => 1,
            Width::U16 => 2,
            Width::U32 => 4,
        }
    }

    /// The TypeScript `DataView` reader for this width.
    pub const fn getter(self) -> &'static str {
        match self {
            Width::U8 => "getUint8",
            Width::U16 => "getUint16",
            Width::U32 => "getUint32",
        }
    }
}

/// Whether a field's value is a position in text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    /// A position in the text a row describes. WHETHER and WHEN it converts to
    /// another unit is the format's rule, not the vocabulary's: the dish
    /// converts the whole buffer in one sweep under `utf16: true`, the census
    /// converts per book through that book's table, and find has converted
    /// already by the time a row is built.
    Offset,
    /// An index, a code, a count, a flag — not a position, so no format's rule
    /// applies to it.
    Plain,
}

pub struct Field {
    pub name: &'static str,
    pub width: Width,
    pub space: Space,
    /// The writer's expression, with the record's binding in scope. Written as
    /// it reads on the engine type; the emitter casts it to `width`.
    pub rust: &'static str,
    pub doc: &'static str,
}

pub struct Record {
    /// The wire name. Also the reader's class name.
    pub name: &'static str,
    /// The generated writer's suffix — `write_<plural>`. Stated rather than
    /// guessed: "Fix" pluralises to "fixes", which no rule derives.
    pub plural: &'static str,
    /// The Rust type the writer iterates, and the binding its fields use.
    pub rust_ty: &'static str,
    pub binding: &'static str,
    /// Extra `(name, type)` parameters the writer takes, ahead of `out`. A
    /// field whose value is not on the row alone declares what it needs here,
    /// so the schema still states the whole writer signature.
    pub context: &'static [(&'static str, &'static str)],
    pub fields: &'static [Field],
    /// A run of sub-rows CLOSING the row, for a record whose length is a
    /// value it carries — find's hit, which names its own piece count. Tail
    /// position only, so the head keeps a stride and the sub-record keeps its
    /// own; a reader that must walk to find row `n` walks once, at open.
    pub tail: Option<Repeat>,
    pub doc: &'static str,
}

/// The run that closes a tailed record.
pub struct Repeat {
    /// The name of an earlier field OF THIS RECORD holding the run's length.
    /// Named rather than positional, because a reader resolves it by name too.
    pub count: &'static str,
    /// The sub-row's own record. It may not itself have a tail: one level is
    /// what a wire needs and two is a grammar.
    pub of: &'static Record,
    /// The writer's expression for the run, with the record's binding in
    /// scope — an iterator of `of.rust_ty`.
    pub rust: &'static str,
}

impl Repeat {
    /// The reader's accessor name for the run — the sub-record's own plural,
    /// so `HitRow.pieces()` and `write_pieces` are named from one word.
    pub const fn plural_of(&self) -> &'static str {
        self.of.plural
    }
}

impl Record {
    /// Bytes per row HEAD: the fields, in order, with no padding — nothing
    /// casts, so nothing pads. A tailed record's whole row is this plus
    /// `count` × its tail's stride, which only the row itself knows.
    pub const fn stride(&self) -> usize {
        let mut total = 0;
        let mut at = 0;
        while at < self.fields.len() {
            total += self.fields[at].width.bytes();
            at += 1;
        }
        total
    }

    /// A field's byte offset within its row.
    pub fn offset_of(&self, name: &str) -> usize {
        let mut at = 0;
        for field in self.fields {
            if field.name == name {
                return at;
            }
            at += field.width.bytes();
        }
        panic!("no such wire field: {}::{}", self.name, name)
    }
}
