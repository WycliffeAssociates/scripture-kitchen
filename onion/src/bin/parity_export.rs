//! Expected outputs for a port's parity tests: onion's tokens, CST, verse text
//! and verse index for each input file, as plain text.
//!
//! ```text
//! cargo run --release -p usfm_onion --bin parity_export -- <file-or-dir> <out-dir>
//!
//! GEN.usfm  ->  <out-dir>/GEN.tokens.tsv  GEN.cst.txt  GEN.vref.txt  GEN.toc.tsv
//! ```
//!
//! A directory input takes every `.usfm`, `.sfm` and `.txt` directly inside it.
//! Each file is read as UTF-8 and lexed with no extensions registered. Every
//! offset below is in UTF-16 code units of that same string (half-open
//! `start..end`); a byte inside a character snaps down to the character's
//! start, as `utf16::Utf16Index::to_utf16` does. Lines end in `\n`, fields are
//! tab-separated, and `-` stands for an absent value.
//!
//! **`<stem>.tokens.tsv`** — one line per token, in order:
//!
//! ```text
//! index  kind  flag  markerIdx  markerName  level  start  end
//! 0      Marker  0   38         id          0      0      4
//! ```
//!
//! `kind` is the `TokenKind` variant name. `flag` is 1 for `\+` on `Marker` /
//! `ClosingMarker` and for `-e` on `Milestone`, else 0. `markerIdx` is the
//! token's row in `docs/markers.json` (0 on every non-marker token and on
//! unresolved markers); `markerName` is that row's name, `-` for row 0.
//! `level` is the token's spelled number (`\q2` → 2).
//!
//! **`<stem>.cst.txt`** — the tree in pre-order, one line per node and per
//! leaf token, indented two spaces per depth (the root is depth 0):
//!
//! ```text
//! N  nodeId  openerToken  markerName  context  closeReason  start  end
//!   T  tokenIndex
//! ```
//!
//! A node's children follow it at depth + 1 in document order, its opening
//! marker first. `openerToken` is `root` on node 0. `context` is the stamped
//! `SpecContext`; `closeReason` is `Explicit`, `Implicit`, `Recovery` or
//! `Eof`; `start..end` is `Cst::extent` (opener through the last descendant).
//!
//! **`<stem>.vref.txt`** — exactly `vref::joined(&toc, &mask, source, true)`
//! with `mask` built from `Filter::verse_text()`, plus one final `\n` when not
//! empty. `true` is `trim`: each verse's text loses leading and trailing
//! Unicode White_Space (Rust's `str::trim`), never interior whitespace.
//!
//! **`<stem>.toc.tsv`**:
//!
//! ```text
//! book     text  hex  bookToken          the first \id's code, 3 bytes
//! chapter  row  number  label  labelStart  labelEnd  start  end  token
//! verse    row  chapter  first  last  label  labelStart  labelEnd  at  token
//! member   verseRow  from  to  fromSegStart  fromSegEnd  toSegStart  toSegEnd
//! ```
//!
//! `book` text is the bytes as lossy UTF-8 with NULs dropped (`-` when
//! empty), `hex` the three bytes. Chapter row 0 is the front matter; its
//! `token` is `-`. Each verse's `member` lines follow it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use usfm_onion::cst::{self, Cst};
use usfm_onion::tables::generated;
use usfm_onion::utf16::{Utf16Index, utf16_index};
use usfm_onion::{Filter, Token, TokenKind, mask, toc, vref};

/// `Cst::child_ids` tags a node id with the top bit; an untagged id is a token.
const NODE_ID_BIT: u32 = 1 << 31;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(out)) = (args.next(), args.next()) else {
        eprintln!("usage: parity_export <file-or-dir> <out-dir>");
        std::process::exit(2);
    };
    let (input, out) = (PathBuf::from(input), PathBuf::from(out));
    std::fs::create_dir_all(&out).expect("create the output directory");

    let files: Vec<PathBuf> = if input.is_dir() {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&input)
            .expect("read the input directory")
            .map(|entry| entry.expect("directory entry").path())
            .filter(|path| {
                path.is_file()
                    && path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                        ["usfm", "sfm", "txt"].contains(&e.to_ascii_lowercase().as_str())
                    })
            })
            .collect();
        files.sort();
        files
    } else {
        vec![input]
    };

    for file in &files {
        let source =
            std::fs::read_to_string(file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        let stem = file.file_stem().expect("a file name").to_string_lossy();
        export(&source, &out, &stem);
        println!("{} -> {}/{stem}.*", file.display(), out.display());
    }
}

fn export(source: &str, out: &Path, stem: &str) {
    let bytes = source.as_bytes();
    let tokens = usfm_onion::lex(source);
    let tree = cst::build(&tokens);
    let toc = toc(bytes, &tokens);
    let verse_text = mask(bytes, &tokens, &tree, &Filter::verse_text());
    let ix = utf16_index(bytes);

    let mut vref = vref::joined(&toc, &verse_text, bytes, true);
    if !vref.is_empty() {
        vref.push('\n');
    }

    let write = |ext: &str, text: String| {
        std::fs::write(out.join(format!("{stem}.{ext}")), text).expect("write output");
    };
    write("tokens.tsv", tokens_tsv(&tokens, &ix));
    write("cst.txt", cst_txt(&tree, &tokens, &ix));
    write("vref.txt", vref);
    write("toc.tsv", toc_tsv(&toc, bytes, &ix));
}

fn marker_name(idx: u8) -> &'static str {
    match generated::name(idx) {
        "" => "-",
        name => name,
    }
}

fn kind_and_flag(kind: TokenKind) -> (&'static str, u8) {
    match kind {
        TokenKind::Marker { nested } => ("Marker", nested.into()),
        TokenKind::ClosingMarker { nested } => ("ClosingMarker", nested.into()),
        TokenKind::Milestone { end } => ("Milestone", end.into()),
        TokenKind::MilestoneTerminator => ("MilestoneTerminator", 0),
        TokenKind::Newline => ("Newline", 0),
        TokenKind::OptBreak => ("OptBreak", 0),
        TokenKind::Text => ("Text", 0),
        TokenKind::Designator => ("Designator", 0),
        TokenKind::NoteCaller => ("NoteCaller", 0),
        TokenKind::BookCode => ("BookCode", 0),
        TokenKind::AttrList => ("AttrList", 0),
        TokenKind::Pad => ("Pad", 0),
    }
}

fn tokens_tsv(tokens: &[Token], ix: &Utf16Index) -> String {
    let mut out = String::with_capacity(tokens.len() * 32);
    for (i, token) in tokens.iter().enumerate() {
        let (kind, flag) = kind_and_flag(token.kind());
        let _ = writeln!(
            out,
            "{i}\t{kind}\t{flag}\t{}\t{}\t{}\t{}\t{}",
            token.marker_idx,
            marker_name(token.marker_idx),
            token.level,
            ix.to_utf16(token.start),
            ix.to_utf16(token.end()),
        );
    }
    out
}

fn cst_txt(tree: &Cst, tokens: &[Token], ix: &Utf16Index) -> String {
    let mut out = String::with_capacity(tokens.len() * 16);
    // Explicit stack of (child id, depth), pushed in reverse so pops come out
    // in document order.
    let mut stack = vec![(NODE_ID_BIT, 0usize)];
    while let Some((id, depth)) = stack.pop() {
        let indent = "  ".repeat(depth);
        if id & NODE_ID_BIT == 0 {
            let _ = writeln!(out, "{indent}T\t{id}");
            continue;
        }
        let node_id = id & !NODE_ID_BIT;
        let node = &tree.nodes[node_id as usize];
        let (opener, name) = if node_id == 0 {
            ("root".to_string(), "-")
        } else {
            let token = tokens[node.token as usize];
            (node.token.to_string(), marker_name(token.marker_idx))
        };
        let extent = tree.extent(node_id, tokens);
        let _ = writeln!(
            out,
            "{indent}N\t{node_id}\t{opener}\t{name}\t{:?}\t{:?}\t{}\t{}",
            node.context(),
            node.close_reason(),
            ix.to_utf16(extent.start),
            ix.to_utf16(extent.end),
        );
        for slot in node.children.clone().rev() {
            stack.push((tree.child_ids[slot as usize], depth + 1));
        }
    }
    out
}

fn toc_tsv(toc: &usfm_onion::Toc, source: &[u8], ix: &Utf16Index) -> String {
    let mut out = String::new();
    let text: String = String::from_utf8_lossy(&toc.book).replace('\0', "");
    let _ = writeln!(
        out,
        "book\t{}\t{:02x}{:02x}{:02x}\t{}",
        if text.is_empty() { "-" } else { &text },
        toc.book[0],
        toc.book[1],
        toc.book[2],
        toc.book_token.map_or("-".to_string(), |t| t.to_string()),
    );
    let label = |start: u32, end: u32| {
        let text = String::from_utf8_lossy(&source[start as usize..end as usize]).into_owned();
        if text.is_empty() {
            "-".to_string()
        } else {
            text
        }
    };
    for (row, ch) in toc.chapters.iter().enumerate() {
        let _ = writeln!(
            out,
            "chapter\t{row}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            ch.number,
            label(ch.label_start, ch.label_end),
            ix.to_utf16(ch.label_start),
            ix.to_utf16(ch.label_end),
            ix.to_utf16(ch.start),
            ix.to_utf16(ch.end),
            if ch.token == u32::MAX {
                "-".to_string()
            } else {
                ch.token.to_string()
            },
        );
    }
    for (row, v) in toc.verses.iter().enumerate() {
        let _ = writeln!(
            out,
            "verse\t{row}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            v.chapter,
            v.first,
            v.last,
            label(v.label_start, v.label_end),
            ix.to_utf16(v.label_start),
            ix.to_utf16(v.label_end),
            ix.to_utf16(v.at),
            v.token,
        );
        for m in toc.members_of(v) {
            let _ = writeln!(
                out,
                "member\t{row}\t{}\t{}\t{}\t{}\t{}\t{}",
                m.from,
                m.to,
                ix.to_utf16(m.from_segment_start),
                ix.to_utf16(m.from_segment_end),
                ix.to_utf16(m.to_segment_start),
                ix.to_utf16(m.to_segment_end),
            );
        }
    }
    out
}
