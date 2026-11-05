//! BTT Writer projects, read as books.
//!
//! ```text
//! files of one Writer book project          stitch(files, ROM, "Romans")
//!   front/title.txt   "Roma\n"                →  \id ROM
//!   05/title.txt      "Pasal 5"                  \ide UTF-8
//!   05/01.txt         "\c 5 \v 1 …"              \h Roma …  \mt1 Roma
//!   05/03.txt         "\v 3 …"                   \c 5
//!   manifest.json     (not the book)             \cl Pasal 5
//!                                                \v 1 …
//!                                                \v 3 …
//! ```
//!
//! BTT Writer keeps one project per book: chunk files (`NN/NN.txt`) holding
//! inline USFM fragments, a title per chapter, and the book's title in
//! `front/title.txt`. [`stitch()`] turns one book's files into one USFM book with
//! the same bytes Sefer's `writerBook.ts` writes; `fixtures/stitched/` pins
//! them on five real projects.
//!
//! [`pack`] writes git objects into one pack and its index in memory: what a
//! replayed history becomes, with no git library and no filesystem.
//!
//! No I/O: callers hand over each file's path and bytes, read however they
//! read them (a folder, a git tree), and write what comes back.

pub mod pack;
pub mod stitch;

pub use stitch::{
    WriterChapter, WriterTree, book_to_usfm, decode_text, is_text_path, stitch, tree_from_files,
};
