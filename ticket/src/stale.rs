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
