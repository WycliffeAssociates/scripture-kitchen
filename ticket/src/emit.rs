//! The emitters: one record in, one language's half of it out.
//!
//! A format's crate owns its declaration, its templates, its envelope, its
//! magic and its version. What lives here is everything INSIDE a row block —
//! so two formats cannot disagree about how a `u16` reaches the buffer, nor
//! about where a reader looks for it.

use crate::schema::{Field, Layout, Record, Shape, Slot, Space, Width};

/// Every record's writer, in declaration order — the `@@WRITERS@@` a format's
/// Rust template is filled with.
pub fn writers_rs(records: &[&Record]) -> String {
    let mut out = String::new();
    for record in records {
        out.push_str(&writer(record));
    }
    out
}

/// Every record's accessor class, in declaration order — the `@@ROWS@@` a
/// format's TypeScript template is filled with.
pub fn row_classes_ts(records: &[&Record]) -> String {
    let mut out = String::new();
    for record in records {
        out.push_str(&row_class_ts(record));
    }
    out
}

/// Fill a checked-in template's `@@PLACEHOLDER@@` holes.
///
/// An envelope is per-format and hand-written, but its CONSTANTS come from
/// that format's schema through here, so the two sides of one envelope cannot
/// drift apart on a number.
pub fn fill(template: &str, substitutions: &[(&str, String)]) -> String {
    let mut text = template.to_string();
    for (placeholder, value) in substitutions {
        text = text.replace(placeholder, value);
    }
    text
}

/// One record's writer, for a crate declaring its own wire beside this one:
/// `galley::toc` fills its own template with these, so the two generators
/// cannot disagree about how a `u16` reaches the buffer.
pub fn writer(record: &Record) -> String {
    let mut out = String::new();
    let tail = record.tail.as_ref();
    let offsets_in = |fields: &[Field]| fields.iter().any(|f| f.space == Space::Offset);
    let converts = offsets_in(record.fields) || tail.is_some_and(|run| offsets_in(run.of.fields));
    // A tailed record has no single stride to state, so the doc states what it
    // does have: a head, and a run whose length the row itself carries.
    let size = match tail {
        None => format!("{} bytes per row.", record.stride()),
        Some(run) => format!(
            "{} bytes per row head, then `{}` × {}.",
            record.stride(),
            run.count,
            run.of.stride()
        ),
    };
    out.push_str(&format!(
        "/// {}\n///\n/// {}{}\n",
        record.doc,
        size,
        if converts {
            " `offsets` collects the position of every\n/// source offset written, for the UTF-16 pass."
        } else {
            " Every field is an index or a code, so nothing\n/// here is ever converted."
        },
    ));
    out.push_str(&signature(record, converts));
    out.push_str(&format!(
        "    out.reserve(rows.len() * {});\n    for {} in rows {{\n",
        record.stride(),
        record.binding
    ));
    push_fields(&mut out, record.fields, 8);
    if let Some(run) = tail {
        out.push_str(&format!(
            "        for {} in {} {{\n",
            run.of.binding, run.rust
        ));
        push_fields(&mut out, run.of.fields, 12);
        out.push_str("        }\n");
    }
    out.push_str("    }\n}\n\n");
    out
}

/// One `extend_from_slice` per field, in order, at `indent` spaces. A tail's
/// fields are written by the same lines as a head's — one answer to how a
/// field reaches the buffer, whichever level it sits at.
fn push_fields(out: &mut String, fields: &[Field], indent: usize) {
    let pad = " ".repeat(indent);
    for field in fields {
        if field.space == Space::Offset {
            out.push_str(&format!("{pad}offsets.push(out.len());\n"));
        }
        let value = format!("(({}) as {})", field.rust, rust_ty(field.width));
        out.push_str(&format!(
            "{pad}// {}\n{pad}out.extend_from_slice(&{}.to_le_bytes());\n",
            field.doc, value
        ));
    }
}

/// The writer's `fn` line, wrapped the way rustfmt would wrap it. The emitted
/// file is checked in and `cargo fmt` must leave it alone — a generator whose
/// output the formatter rewrites reports itself stale on every run.
fn signature(record: &Record, converts: bool) -> String {
    const MAX: usize = 100;
    let mut params = vec![format!("rows: &[{}]", record.rust_ty)];
    params.extend(record.context.iter().map(|(n, ty)| format!("{n}: {ty}")));
    params.push("out: &mut Vec<u8>".to_string());
    params.push(format!(
        "{}offsets: &mut Offsets",
        if converts { "" } else { "_" }
    ));

    let head = format!("pub fn write_{}(", record.plural);
    let line = format!("{head}{}) {{", params.join(", "));
    if line.len() <= MAX {
        return line + "\n";
    }
    let mut out = format!("{head}\n");
    for param in &params {
        out.push_str(&format!("    {param},\n"));
    }
    out.push_str(") {\n");
    out
}

const fn rust_ty(width: Width) -> &'static str {
    match width {
        Width::U8 => "u8",
        Width::U16 => "u16",
        Width::U32 => "u32",
    }
}

/// One record's accessor class, for a crate declaring its own wire beside this
/// one. See [`writer`]: the reader half of the same reuse.
pub fn row_class_ts(record: &Record) -> String {
    let mut out = String::new();
    let tail = record.tail.as_ref();
    let size = match tail {
        None => format!("{} bytes per row.", record.stride()),
        Some(run) => format!(
            "{} bytes per row head, then `{}` × {}.",
            record.stride(),
            run.count,
            run.of.stride()
        ),
    };
    let cursor = match tail {
        None => {
            "A CURSOR: `seek` moves it, the getters read\n\
                 * the row it is on, and nothing is allocated per row."
        }
        Some(_) => {
            "A CURSOR: `seek` moves it and the getters read the\n\
                    * row it is on. The row starts are found once, at construction; the\n\
                    * tail accessor makes one view per call."
        }
    };
    out.push_str(&format!(
        "/**\n * {}\n *\n * {} {}\n */\nexport class {}Row {{\n",
        record.doc, size, cursor, record.name
    ));
    // NATIVE private fields. A wire field is free to be called `at` or
    // `view` — `VerseRow.at` is — and an ordinary member of that name would
    // be shadowed by the cursor's own, so the getter would return a row
    // offset instead of reading the wire. `#` cannot collide with a getter
    // name, so the hazard does not exist rather than being avoided.
    match tail {
        None => {
            out.push_str(
                "  readonly #view: DataView;\n  #row = 0;\n\n  \
                 constructor(view: DataView) {\n    this.#view = view;\n  }\n\n  \
                 /** Rows in the section. */\n  get length(): number {\n    return \
                 (this.#view.byteLength / this.stride) | 0;\n  }\n\n",
            );
            out.push_str(&format!(
                "  readonly stride = {};\n\n  /** Move to row `n`; returns `this` so reads \
                 chain. */\n  seek(n: number): this {{\n    this.#row = n * {};\n    return \
                 this;\n  }}\n\n",
                record.stride(),
                record.stride()
            ));
        }
        // A tailed record has no constant stride, so the block is walked ONCE
        // at construction and every later seek is an index into what that
        // walk found. The class shape does not change: a consumer that learned
        // one row class reads this one the same way.
        Some(run) => {
            let counter = record
                .fields
                .iter()
                .find(|field| field.name == run.count)
                .unwrap_or_else(|| panic!("{}::{} names no field", record.name, run.count));
            out.push_str(&format!(
                "  readonly #view: DataView;\n  readonly #starts: Uint32Array;\n  \
                 readonly #end: number;\n  #row = 0;\n\n  \
                 /**\n   * A tailed block's row COUNT is not a function of its bytes, so the\n   \
                 * envelope that framed it passes one; the walk stops at whichever comes\n   \
                 * first, the count or the end of the view.\n   */\n  \
                 constructor(view: DataView, rows = Infinity) {{\n    this.#view = view;\n    \
                 const starts: number[] = [];\n    let at = 0;\n    \
                 while (starts.length < rows && at + {head} <= view.byteLength) {{\n      \
                 starts.push(at);\n      \
                 at += {head} + view.{getter}(at + {count_at}{endian}) * {sub};\n    }}\n    \
                 this.#starts = Uint32Array.from(starts);\n    this.#end = at;\n  }}\n\n  \
                 /** Rows in the block. */\n  get length(): number {{\n    return \
                 this.#starts.length;\n  }}\n\n  \
                 /** Bytes those rows occupy: where the block ENDS. */\n  \
                 get byteLength(): number {{\n    return this.#end;\n  }}\n\n  \
                 /** Bytes of this row's HEAD; the run after it is `{count}` × {sub}. */\n  \
                 readonly stride = {head};\n\n  \
                 /** Move to row `n`; returns `this` so reads chain. */\n  \
                 seek(n: number): this {{\n    const at = this.#starts[n];\n    \
                 if (at === undefined) {{\n      \
                 throw new RangeError(`row ${{n}} of ${{this.#starts.length}}`);\n    }}\n    \
                 this.#row = at;\n    return this;\n  }}\n\n",
                head = record.stride(),
                sub = run.of.stride(),
                count = run.count,
                count_at = record.offset_of(run.count),
                getter = counter.width.getter(),
                endian = if counter.width == Width::U8 {
                    ""
                } else {
                    ", true"
                },
            ));
        }
    }
    let mut at = 0usize;
    for field in record.fields {
        if field.name != "pad" && field.name != "reserved" {
            out.push_str(&format!(
                "  /** {} */\n  get {}(): number {{\n    return this.#view.{}(this.#row + {}{});\n  }}\n\n",
                field.doc,
                field.name,
                field.width.getter(),
                at,
                if field.width == Width::U8 { "" } else { ", true" },
            ));
        }
        at += field.width.bytes();
    }
    if let Some(run) = tail {
        out.push_str(&format!(
            "  /** {doc} */\n  {plural}(): {name}Row {{\n    const at = this.#row + {head};\n    \
             return new {name}Row(\n      new DataView(this.#view.buffer, \
             this.#view.byteOffset + at, this.{count} * {sub}),\n    );\n  }}\n\n",
            doc = run.of.doc,
            plural = run.plural_of(),
            name = run.of.name,
            head = record.stride(),
            count = run.count,
            sub = run.of.stride(),
        ));
    }
    out.push_str("}\n\n");
    out
}

/// One format's [`Layout`] as the JSON a docs page draws byte strips from.
///
/// Every region and record carries its declaration's `doc`, so what a reader
/// sees on hover is the sentence the code was written against. A fixed region
/// or table whose slots do not tile it exactly panics: a layout with a gap in
/// it would draw bytes nothing declared.
pub fn layout_json(layout: &Layout) -> String {
    let magic = String::from_utf8_lossy(&layout.magic.to_le_bytes()).into_owned();
    let mut out = String::from("{\n");
    out.push_str(&format!("  \"format\": {},\n", json_str(layout.format)));
    out.push_str(&format!("  \"magic\": {},\n", json_str(&magic)));
    out.push_str(&format!("  \"magic_hex\": \"0x{:08x}\",\n", layout.magic));
    out.push_str(&format!("  \"version\": {},\n", layout.version));
    out.push_str(&format!(
        "  \"declared_in\": {},\n",
        json_str(layout.declared_in)
    ));
    out.push_str(&format!("  \"reader\": {},\n", json_str(layout.reader)));
    out.push_str(&format!("  \"generated\": {},\n", layout.generated));
    out.push_str(&format!("  \"doc\": {},\n", json_str(layout.doc)));
    out.push_str("  \"envelope\": [");
    for (n, region) in layout.envelope.iter().enumerate() {
        out.push_str(if n == 0 { "\n" } else { ",\n" });
        let mut head = format!(
            "\"region\": {}, \"at\": {}",
            json_str(region.name),
            json_str(&region.at)
        );
        let slots = match &region.shape {
            Shape::Fixed { bytes, slots } => {
                tiles(region.name, *bytes, slots);
                head.push_str(&format!(", \"kind\": \"fixed\", \"bytes\": {bytes}"));
                Some(slots)
            }
            Shape::Table {
                entry_bytes,
                count,
                slots,
            } => {
                tiles(region.name, *entry_bytes, slots);
                head.push_str(&format!(
                    ", \"kind\": \"table\", \"entry_bytes\": {entry_bytes}, \"count\": {}",
                    json_str(count)
                ));
                Some(slots)
            }
            Shape::Rows { of, count } => {
                head.push_str(&format!(
                    ", \"kind\": \"rows\", \"of\": {}, \"count\": {}",
                    json_str(of.name),
                    json_str(count)
                ));
                None
            }
            Shape::Words { width, count } => {
                head.push_str(&format!(
                    ", \"kind\": \"words\", \"width\": {}, \"count\": {}",
                    width.bytes(),
                    json_str(count)
                ));
                None
            }
            Shape::Blob { bytes, holds } => {
                head.push_str(&format!(
                    ", \"kind\": \"blob\", \"bytes\": {}",
                    json_str(bytes)
                ));
                if let Some(format) = holds {
                    head.push_str(&format!(", \"holds\": {}", json_str(format)));
                }
                None
            }
        };
        out.push_str(&format!("    {{{head}, \"doc\": {}", json_str(region.doc)));
        if let Some(slots) = slots {
            out.push_str(", \"fields\": [");
            for (n, slot) in slots.iter().enumerate() {
                out.push_str(if n == 0 { "\n" } else { ",\n" });
                out.push_str(&format!(
                    "      {{\"name\": {}, \"offset\": {}, \"width\": {}, \"doc\": {}}}",
                    json_str(slot.name),
                    slot.at,
                    slot.bytes,
                    json_str(slot.doc)
                ));
            }
            out.push_str("\n    ]");
        }
        out.push('}');
    }
    out.push_str(if layout.envelope.is_empty() {
        "],\n"
    } else {
        "\n  ],\n"
    });
    out.push_str("  \"records\": [");
    for (n, record) in layout.records.iter().enumerate() {
        out.push_str(if n == 0 { "\n" } else { ",\n" });
        out.push_str(&format!(
            "    {{\"name\": {}, \"stride\": {}, \"doc\": {}",
            json_str(record.name),
            record.stride(),
            json_str(record.doc)
        ));
        if let Some(run) = &record.tail {
            out.push_str(&format!(
                ", \"tail\": {{\"count\": {}, \"of\": {}}}",
                json_str(run.count),
                json_str(run.of.name)
            ));
        }
        out.push_str(", \"fields\": [");
        let mut at = 0;
        for (n, field) in record.fields.iter().enumerate() {
            out.push_str(if n == 0 { "\n" } else { ",\n" });
            out.push_str(&format!(
                "      {{\"name\": {}, \"offset\": {at}, \"width\": {}, \"space\": \"{}\", \"doc\": {}}}",
                json_str(field.name),
                field.width.bytes(),
                match field.space {
                    Space::Offset => "offset",
                    Space::Plain => "plain",
                },
                json_str(field.doc)
            ));
            at += field.width.bytes();
        }
        out.push_str("\n    ]}");
    }
    out.push_str(if layout.records.is_empty() {
        "],\n"
    } else {
        "\n  ],\n"
    });
    match &layout.sample {
        None => out.push_str("  \"sample\": null\n"),
        Some(sample) => {
            let options: Vec<String> = sample
                .options
                .iter()
                .map(|(name, value)| format!("{}: {value}", json_str(name)))
                .collect();
            out.push_str(&format!(
                "  \"sample\": {{\"usfm\": {}, \"bin\": {}, \"options\": {{{}}}}}\n",
                json_str(sample.usfm),
                json_str(sample.bin),
                options.join(", ")
            ));
        }
    }
    out.push_str("}\n");
    out
}

fn tiles(region: &str, bytes: usize, slots: &[Slot]) {
    let mut at = 0;
    for slot in slots {
        assert_eq!(
            slot.at, at,
            "{region}.{} leaves a gap or overlaps",
            slot.name
        );
        at += slot.bytes;
    }
    assert_eq!(
        at, bytes,
        "{region}'s slots cover {at} of its {bytes} bytes"
    );
}

/// A JSON string literal: quotes, backslashes and control characters escaped,
/// everything else as written.
pub fn json_str(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
