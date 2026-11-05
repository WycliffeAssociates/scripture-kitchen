//! Scripture containers: the manifest at a repo's root that says what the repo is.
//!
//! ```text
//! manifest.json   BTT Writer, one book       bttwriter::read(bytes) → BttWriterManifest
//!                                             .project(Some("tmf_3jn_text_reg")) → WriterProject { book: 3JN, … }
//! manifest.yaml   Resource Container          (to come)
//! metadata.json   Scripture Burrito           (to come)
//! ```
//!
//! Reading is lenient, because real manifests are: a number where a string
//! belongs, a map where a list belongs, a bare string where an object belongs.
//! Reading never changes the bytes it was given.

pub mod bttwriter;

/// A resource a container says it was translated from: `en/ulb`, version `12`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Related {
    /// Lowercase language tag: `en`.
    pub language: String,
    /// Lowercase resource id: `ulb`.
    pub resource: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

/// Why a manifest couldn't be read at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// Not JSON, or JSON of a shape no leniency covers (`"project": 3`).
    Json(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Json(why) => write!(f, "not a readable manifest: {why}"),
        }
    }
}

impl std::error::Error for ReadError {}

/// The bytes after a leading UTF-8 byte order mark, if there is one.
fn without_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}
