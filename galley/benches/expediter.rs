//! What one publication costs a resident host, warm and after a keystroke.
//!
//!     cargo bench -p usfm_galley --bench expediter
//!
//! One 66-book corpus registered as `Target`s and published once, so the
//! chapter tables and every observation are already resident. Then:
//!
//! - `publish_unchanged` — the whole corpus republished with nothing updated:
//!   chapter-table hits, judge, rebase through the retained mask and UTF-16
//!   table, and encode. No book is projected, mapped, or folded.
//! - `edit_one_chapter_then_publish` — `Expediter::update` with one character
//!   typed into one chapter of MRK, then the same publish. The update's
//!   re-derivation (lex, CST, mask, UTF-16 table for that book) and the one
//!   chapter's re-map are INSIDE the timer: it is what a keystroke costs.
//! - `publish_cold` — a fresh Expediter over the same corpus, registered
//!   outside the timer, publishing for the first time: every chapter mapped.
//!   Run it with and without `--features parallel` for the two map costs.
//! - `publish_after_sweep_churn` — the edit row again, after `CHURN` earlier
//!   edits each published, so the mark-and-sweep walks a book at its full
//!   generation ring. The churn is setup; the delta against
//!   `edit_one_chapter_then_publish` is what the ring costs a publication.
//!
//! No byte counter: no row's cost is a function of corpus bytes.
//! `publish_unchanged` reads no text at all, and the edit row reads exactly one
//! book's.

use std::sync::LazyLock;

use sous_core::Brigade;
use usfm_galley::sous::Expediter;
use usfm_galley::{Role, onion};

fn main() {
    divan::main()
}

/// A committed 66-book USFM corpus, the shape a project actually publishes.
const CORPUS_DIR: &str = "en_ulb";

/// One measured call per sample of the edit row, each a never-seen text, so
/// the prepared texts must outlast the sample count.
const SAMPLES: u32 = 20;
const TEXTS: usize = SAMPLES as usize + 4;

/// Edits behind the swept publication, comfortably past the default four
/// retained generations.
const CHURN: usize = 20;

/// The book one keystroke lands in: a gospel, mid-ladder in size.
const EDITED: &str = "42-MRK.usfm";

struct Corpus {
    books: Vec<(String, String)>,
    /// Successive states of typing into one chapter of one book;
    /// `keystrokes[i]` has `i + 1` characters inserted.
    edited_id: String,
    keystrokes: Vec<String>,
}

static CORPUS: LazyLock<Corpus> = LazyLock::new(load);

fn load() -> Corpus {
    let dir = format!(
        "{}/../testData/exampleCorpora/{CORPUS_DIR}",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{dir}: {error}"))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "usfm"))
        .collect();
    paths.sort();
    let books: Vec<(String, String)> = paths
        .iter()
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(path).unwrap(),
            )
        })
        .collect();
    assert_eq!(books.len(), 66, "{dir} is a whole-Bible corpus");

    // A middle chapter of one book, and a text token inside it: one keystroke
    // moves exactly one chapter's projected content.
    let (edited_id, text) = books
        .iter()
        .find(|(name, _)| name == EDITED)
        .expect("the corpus carries the edited book")
        .clone();
    let starts = onion::chunk::pre_scan(text.as_bytes()).starts;
    let chunk = (starts.len() / 2).max(1);
    let (from, to) = (
        starts[chunk],
        starts.get(chunk + 1).copied().unwrap_or(text.len() as u32),
    );
    let at = onion::lex(&text)
        .iter()
        .find(|token| {
            token.kind() == onion::TokenKind::Text
                && token.start >= from
                && token.start < to
                && token.len > 0
        })
        .map(|token| token.start as usize)
        .expect("a text token in the chosen chapter");
    let keystrokes = (1..=TEXTS)
        .map(|typed| {
            let mut edited = text.clone();
            edited.insert_str(at, &"x".repeat(typed));
            edited
        })
        .collect();

    Corpus {
        books,
        edited_id,
        keystrokes,
    }
}

/// Every book registered and one publication already done, so the next one
/// answers entirely from the chapter tables.
fn warmed(corpus: &Corpus) -> Expediter<Brigade> {
    let mut sous = Expediter::new(Brigade::default(), 64 << 20);
    for (id, text) in &corpus.books {
        sous.update(id.as_str(), Role::Target, text).unwrap();
    }
    sous.publish().unwrap();
    assert!(
        sous.last_mapped() > 0,
        "the first publish maps every chapter"
    );
    sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 0, "the second maps none");
    sous
}

#[divan::bench(sample_count = SAMPLES, sample_size = 1)]
fn publish_unchanged(bencher: divan::Bencher) {
    let corpus = &*CORPUS;
    let mut sous = warmed(corpus);
    bencher.bench_local(|| divan::black_box(sous.publish().unwrap()));
}

#[divan::bench(sample_count = SAMPLES, sample_size = 1)]
fn edit_one_chapter_then_publish(bencher: divan::Bencher) {
    let corpus = &*CORPUS;
    let mut sous = warmed(corpus);

    // The first keystroke proves the claim the row is measuring, outside the
    // timer; the text list is sized so the samples still never repeat one.
    sous.update(
        corpus.edited_id.as_str(),
        Role::Target,
        &corpus.keystrokes[0],
    )
    .unwrap();
    sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 1, "one keystroke, one chapter");

    let mut next = 1;
    bencher.bench_local(|| {
        let typed = &corpus.keystrokes[next];
        next += 1;
        sous.update(corpus.edited_id.as_str(), Role::Target, typed)
            .unwrap();
        divan::black_box(sous.publish().unwrap())
    });
}

/// A fresh Expediter over the whole corpus, published once: every book
/// projected and every chapter mapped. Registration is the input, so the row
/// is the publication itself — and under `--features parallel` it is the same
/// publication with the chapter map on rayon.
#[divan::bench(sample_count = SAMPLES, sample_size = 1)]
fn publish_cold(bencher: divan::Bencher) {
    let corpus = &*CORPUS;
    bencher
        .with_inputs(|| {
            let mut sous = Expediter::new(Brigade::default(), 64 << 20);
            for (id, text) in &corpus.books {
                sous.update(id.as_str(), Role::Target, text).unwrap();
            }
            sous
        })
        .bench_local_refs(|sous| divan::black_box(sous.publish().unwrap()));
}

#[divan::bench(sample_count = SAMPLES, sample_size = 1)]
fn publish_after_sweep_churn(bencher: divan::Bencher) {
    let corpus = &*CORPUS;
    bencher
        .with_inputs(|| {
            let mut sous = warmed(corpus);
            for typed in &corpus.keystrokes[..CHURN] {
                sous.update(corpus.edited_id.as_str(), Role::Target, typed)
                    .unwrap();
                sous.publish().unwrap();
            }
            sous
        })
        // By reference: dropping a 66-book Expediter is not what this measures.
        .bench_local_refs(|sous| {
            sous.update(
                corpus.edited_id.as_str(),
                Role::Target,
                &corpus.keystrokes[CHURN],
            )
            .unwrap();
            divan::black_box(sous.publish().unwrap())
        });
}
