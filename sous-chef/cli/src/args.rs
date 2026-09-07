//! The command line, and what each flag turns on.
//!
//! ```text
//! sous --findings --source source.usfm --publish out.sous book.usfm
//! ```

use crate::*;

/// Inspect projected scripture text Sous Chef will analyze.
#[derive(Cli)]
#[usage(bin = "sous", version = "0.1.0")]
pub(crate) struct Args {
    /// Print aggregate input, alignment, and wall-clock throughput after the debug walk.
    #[usage(long)]
    pub(crate) stats: bool,

    /// Print only aggregate statistics, suppressing all book and alignment debug rows.
    #[usage(long)]
    pub(crate) stats_only: bool,

    /// Load books in parallel; the default loader is genuinely serial.
    #[usage(long)]
    pub(crate) parallel: bool,

    /// Declared source to compare lengths against: USFM like the target, or an
    /// addressless `BOOK C:V<TAB>text` vref file.
    #[usage(long)]
    pub(crate) source: Option<PathBuf>,

    /// Report runs of consecutive target words the declared source already
    /// holds in the paired verse; the lane ships off.
    #[usage(long)]
    pub(crate) source_copy: bool,

    /// Consecutive target words a source-copy run needs before it is a row;
    /// the default is the shipped floor, and below two nothing fires.
    #[usage(long)]
    pub(crate) source_copy_min_run: Option<u32>,

    /// Print hygiene findings over the target with their raw source location.
    #[usage(long)]
    pub(crate) findings: bool,

    /// Write the target's findings as a complete SOUS corpus buffer in raw-book UTF-16.
    #[usage(long)]
    pub(crate) publish: Option<PathBuf>,

    /// Write a self-contained HTML page of every pattern and its sites in context.
    #[usage(long)]
    pub(crate) report: Option<PathBuf>,

    /// Report rare words one edit from a common word — an on-demand review
    /// action, not a channel: runs alone and ignores every other flag.
    #[usage(long)]
    pub(crate) typos: bool,

    /// One .sfm/.usfm file or a directory of immediate .sfm/.usfm files.
    pub(crate) target: PathBuf,
}
