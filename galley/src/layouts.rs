//! Every layout galley writes or publishes, for `docs/layouts/`, beside a
//! real buffer of each the engine wrote from one sample book.
//!
//! ```text
//! files(sample.usfm) -> census.json  samples/census.bin
//!                       mask.json    samples/mask.bin
//!                       find.json    samples/find.bin
//!                       corpus.json  samples/corpus.bin
//!                       sous.json    samples/sous.bin
//! ```

use sous_core::Brigade;

use crate::find::Find;
use crate::mask::Recipe;
use crate::pantry::{BookId, Role};
use crate::sous::Expediter;
use crate::{corpus, find, mask, onion, sous, toc};

/// `(path under docs/layouts, bytes)`, in the order above.
pub fn files(text: &str) -> Vec<(&'static str, Vec<u8>)> {
    let json = |layout| ticket::emit::layout_json(&layout).into_bytes();
    let id = BookId::from(corpus::SAMPLE_KEY);
    let mut kitchen = Expediter::new(Brigade::default(), 16 << 20);
    kitchen
        .update(id.clone(), Role::Target, text)
        .expect("the sample book registers");

    let census = toc::encode(kitchen.pantry(), std::slice::from_ref(&id), false)
        .expect("the sample book has a census");
    let masked = {
        let book = kitchen.book(&id).expect("registered");
        let len = book.text().expect("a target keeps its text").len() as u32;
        mask::encode(
            book.mask().expect("a target keeps its projection"),
            Recipe::VerseText,
            None,
            len,
        )
    };
    let found = kitchen.find(
        &Find::literal(find::wire::schema::SAMPLE_NEEDLE).case_insensitive(true),
        std::slice::from_ref(&id),
        0,
    );
    let published = kitchen.publish().expect("the sample book publishes");
    let opts = onion::wire::ParseOptions {
        diagnostics: true,
        toc: true,
        utf16: false,
    };
    let framed = corpus::plate_texts(&[(corpus::SAMPLE_KEY, text)], opts);

    vec![
        ("census.json", json(toc::schema::layout())),
        ("samples/census.bin", census),
        ("mask.json", json(mask::schema::layout())),
        ("samples/mask.bin", masked),
        ("find.json", json(find::wire::schema::layout())),
        ("samples/find.bin", found),
        ("corpus.json", json(corpus::layout())),
        ("samples/corpus.bin", framed),
        ("sous.json", json(sous::layout::layout())),
        ("samples/sous.bin", published),
    ]
}
