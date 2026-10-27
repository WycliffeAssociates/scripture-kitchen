//! One message shape for every checked-in artifact that could go stale.
//!
//! ```text
//! src/tables/generated.rs is STALE — run `cargo run --bin codegen`.
//! First difference at line 412:
//!   checked in: out.extend_from_slice(&((c.number) as u16).to_le_bytes());
//!   fresh:      out.extend_from_slice(&((c.number) as u32).to_le_bytes());
//! ```

/// The failure message for a generated file that no longer matches its
/// generator, naming the first line that differs — not a dump of the file.
///
/// `fix` is the command that regenerates it, because a reader of this message
/// is someone who just edited a schema and does not yet know which bin owns
/// which artifact.
pub fn first_difference(what: &str, fix: &str, fresh: &str, checked_in: &str) -> String {
    let at = fresh
        .lines()
        .zip(checked_in.lines())
        .position(|(a, b)| a != b);
    format!(
        "{what} is STALE — run `{fix}`.\n{}",
        match at {
            Some(line) => format!(
                "First difference at line {}:\n  checked in: {}\n  fresh:      {}",
                line + 1,
                checked_in.lines().nth(line).unwrap_or("<eof>"),
                fresh.lines().nth(line).unwrap_or("<eof>"),
            ),
            None => format!(
                "Lines agree as far as they go; length differs \
                 ({} checked in vs {} fresh).",
                checked_in.lines().count(),
                fresh.lines().count(),
            ),
        }
    )
}

/// The same for a checked-in binary: the first byte offset that differs.
///
/// ```text
/// samples/dish.bin is STALE — run `cargo run --bin codegen`.
/// First difference at byte 0x38: checked in 0x0c, fresh 0x0d (1168 vs 1172 bytes).
/// ```
pub fn first_byte_difference(what: &str, fix: &str, fresh: &[u8], checked_in: &[u8]) -> String {
    let at = fresh
        .iter()
        .zip(checked_in)
        .position(|(a, b)| a != b)
        .unwrap_or(fresh.len().min(checked_in.len()));
    let byte = |bytes: &[u8]| {
        bytes
            .get(at)
            .map_or("<eof>".into(), |b| format!("0x{b:02x}"))
    };
    format!(
        "{what} is STALE — run `{fix}`.\nFirst difference at byte 0x{at:x}: checked in {}, \
         fresh {} ({} vs {} bytes).",
        byte(checked_in),
        byte(fresh),
        checked_in.len(),
        fresh.len(),
    )
}
