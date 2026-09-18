//! Instrument: SHAPES — synthetic books built here, chosen so one chapter
//! moves at a time.

use super::*;

/// Six chapters of a plausible book, one `\c` per chunk.
fn book(code: &str, chapters: &[&str]) -> String {
    let mut text = format!("\\id {code}\n\\h {code}\n");
    for (number, body) in chapters.iter().enumerate() {
        text.push_str(&format!("\\c {}\n\\p\n\\v 1 {body}\n", number + 1));
    }
    text
}

fn mark() -> String {
    book(
        "MRK",
        &[
            "In the beginning of the good news.",
            "He entered Capernaum again.",
            "A man with a withered hand.",
            "The sower went out to sow.",
            "They came to the other side.",
            "Is this not the carpenter?",
        ],
    )
}

/// Chunk ranges of `text`, from the same pre-scan the fingerprint uses.
fn chunk_ranges(text: &str) -> Vec<Range<u32>> {
    let starts = onion::chunk::pre_scan(text.as_bytes()).starts;
    (0..starts.len())
        .map(|i| starts[i]..starts.get(i + 1).copied().unwrap_or(text.len() as u32))
        .collect()
}

/// The oracle `changed_chunks` is measured against: chunks of `current`
/// whose BYTES appear nowhere in `baseline`.
fn rework_by_bytes(baseline: &str, current: &str) -> Vec<Range<u32>> {
    let seen: Vec<&str> = chunk_ranges(baseline)
        .into_iter()
        .map(|range| &baseline[range.start as usize..range.end as usize])
        .collect();
    chunk_ranges(current)
        .into_iter()
        .filter(|range| !seen.contains(&&current[range.start as usize..range.end as usize]))
        .collect()
}

fn changed(baseline: &str, current: &str) -> Vec<Range<u32>> {
    fingerprint(baseline).changed_chunks(&fingerprint(current))
}

#[test]
fn a_fingerprint_carries_the_same_checksums_the_hex_recipe_reports() {
    let text = mark();
    let print = fingerprint(&text);
    let hex = crate::chunks(&text);
    assert_eq!(print.chunk_count(), hex.starts.len());
    for (i, checksum) in hex.checksums.iter().enumerate() {
        assert_eq!(format!("{:?}", print.checksums[i]), *checksum);
    }
    assert_eq!(print.checksum(), RawChecksum::of(text.as_bytes()));
}

#[test]
fn an_edited_chapter_is_the_only_rework_range() {
    let before = mark();
    let after = before.replace("The sower went out to sow.", "The sower went out to sow!");
    let ranges = changed(&before, &after);
    assert_eq!(ranges, rework_by_bytes(&before, &after));
    assert_eq!(ranges.len(), 1);
    let edited = &after[ranges[0].start as usize..ranges[0].end as usize];
    assert!(edited.starts_with("\\c 4\n"), "{edited:?}");
    assert!(fingerprint(&before).differs_from(&fingerprint(&after)));
}

#[test]
fn an_inserted_chapter_is_the_only_rework_range() {
    let before = mark();
    let after = before.replace("\\c 5\n", "\\c 4b\n\\p\n\\v 1 An interpolation.\n\\c 5\n");
    let ranges = changed(&before, &after);
    assert_eq!(ranges, rework_by_bytes(&before, &after));
    assert_eq!(ranges.len(), 1);
    let inserted = &after[ranges[0].start as usize..ranges[0].end as usize];
    assert!(inserted.starts_with("\\c 4b\n"), "{inserted:?}");
}

#[test]
fn a_moved_chapter_is_dirty_but_needs_no_rework() {
    let before = mark();
    let chunks = chunk_ranges(&before);
    let (second, third) = (chunks[2].clone(), chunks[3].clone());
    let mut after = before[..second.start as usize].to_string();
    after.push_str(&before[third.start as usize..third.end as usize]);
    after.push_str(&before[second.start as usize..second.end as usize]);
    after.push_str(&before[third.end as usize..]);

    assert_eq!(changed(&before, &after), Vec::new(), "no rework");
    assert_eq!(changed(&before, &after), rework_by_bytes(&before, &after));
    assert!(
        fingerprint(&before).differs_from(&fingerprint(&after)),
        "a moved chapter is a different file"
    );
}

#[test]
fn identical_text_is_neither_dirty_nor_rework() {
    let text = mark();
    assert_eq!(changed(&text, &text), Vec::new());
    assert!(!fingerprint(&text).differs_from(&fingerprint(&text)));
}

fn pantry() -> Pantry {
    Pantry::new(1 << 20)
}

fn mrk() -> BookId {
    BookId::from("books/mrk.usfm")
}

#[test]
fn an_identical_update_derives_nothing_and_recopies_no_text() {
    let mut pantry = pantry();
    let text = mark();
    let id = mrk();
    let first = pantry.update(id.clone(), Role::Target, &text).unwrap();
    assert_eq!(first.key(), BookKey::new(*b"MRK"));
    let retained = first.text().unwrap().as_ptr();
    let (derivations, misses) = (pantry.derivations(), pantry.chunk_stats().misses);

    let again = pantry.update(id, Role::Target, &text).unwrap();
    assert_eq!(again.key(), BookKey::new(*b"MRK"));
    assert_eq!(
        again.text().unwrap().as_ptr(),
        retained,
        "text not recopied"
    );
    assert_eq!(pantry.derivations(), derivations, "no book derivation");
    assert_eq!(pantry.chunk_stats().misses, misses, "no chunk work");
}

#[test]
fn a_changed_update_replaces_the_products() {
    let mut pantry = pantry();
    let id = mrk();
    let before = mark();
    let entry = pantry.update(id.clone(), Role::Target, &before).unwrap();
    let first = entry.checksum();
    let published = entry.published_len().unwrap();

    let after = before.replace(
        "A man with a withered hand.",
        "A man with a withered hand 🖐.",
    );
    let rework = pantry.changed_since_update(&id, &after).unwrap();
    assert_eq!(rework, rework_by_bytes(&before, &after));
    assert_eq!(rework.len(), 1);

    let entry = pantry.update(id.clone(), Role::Target, &after).unwrap();
    assert_ne!(entry.checksum(), first);
    assert_eq!(entry.checksum(), RawChecksum::of(after.as_bytes()));
    assert_eq!(
        entry.text().unwrap(),
        after,
        "the copy moved with the update"
    );
    // The inserted " 🖐" is a space plus a surrogate pair: three units.
    assert_eq!(entry.published_len().unwrap(), published + 3);
    assert_eq!(
        pantry.changed_since_update(&id, &after).unwrap(),
        Vec::new(),
        "the update moved the baseline"
    );
}

#[test]
fn the_retained_products_answer_without_the_text() {
    let mut pantry = pantry();
    let text = mark();
    let entry = pantry.update(mrk(), Role::Target, &text).unwrap();

    let projected = entry.mask().unwrap().text(text.as_bytes());
    assert!(
        !projected.contains('\\'),
        "no markup survives: {projected:?}"
    );
    assert_eq!(
        projected.lines().filter(|line| !line.is_empty()).count(),
        6,
        "six verses"
    );
    let numbers: Vec<u16> = entry.toc().chapters.iter().map(|row| row.number).collect();
    assert_eq!(numbers, vec![0, 1, 2, 3, 4, 5, 6], "front matter plus six");
    let index = onion::utf16_index(text.as_bytes());
    for range in &entry.mask().unwrap().ranges {
        assert_eq!(
            entry.utf16().unwrap().to_utf16(range.start),
            index.to_utf16(range.start)
        );
        assert_eq!(
            entry.utf16().unwrap().to_utf16(range.end),
            index.to_utf16(range.end)
        );
    }
    assert_eq!(entry.published_len().unwrap(), index.len_utf16());
}

#[test]
fn a_target_retains_its_text_by_default() {
    let mut pantry = pantry();
    let text = mark();
    let mut entry = pantry.update(mrk(), Role::Target, &text).unwrap();
    assert_eq!(entry.text().unwrap(), text);
    assert_eq!(entry.role(), Role::Target);
    assert_eq!(
        entry.lint().unwrap().observations,
        Pantry::new(1 << 20).lint(&text).observations
    );
}

/// A target's findings are placed by rescanning its current text, so a
/// target that keeps none cannot be registered at all. `ProductsOnly`
/// waits for `Role::Reference`.
#[test]
fn a_target_cannot_be_products_only() {
    let mut pantry = pantry();
    let id = mrk();
    assert_eq!(
        pantry
            .update_with(
                id.clone(),
                Role::Target,
                Retain::ProductsOnly,
                SourceLanes::Lengths,
                &mark(),
            )
            .err(),
        Some(PantryError::TargetNeedsText { id: id.clone() })
    );
    assert!(
        pantry.books(Role::Target).is_empty(),
        "a refused update registers nothing"
    );
    // The default retention is the one a target has.
    let entry = pantry.update(id, Role::Target, &mark()).unwrap();
    assert_eq!(entry.text().unwrap(), mark());
}

#[test]
fn a_retained_lint_equals_the_loose_text_door() {
    let mut pantry = pantry();
    let text = mark();
    let through_entry = pantry
        .update(mrk(), Role::Target, &text)
        .unwrap()
        .lint()
        .unwrap();
    let direct = Pantry::new(1 << 20).lint(&text);
    assert_eq!(through_entry.observations, direct.observations);

    let opts = onion::wire::ParseOptions {
        toc: true,
        ..Default::default()
    };
    let plated = pantry.book(&mrk()).unwrap().parse(opts).unwrap();
    assert_eq!(plated, Pantry::new(1 << 20).parse(&text, opts));
}

#[test]
fn retention_is_what_resident_bytes_grows_by() {
    let text = mark();
    let mut kept = pantry();
    kept.update(mrk(), Role::Target, &text).unwrap();
    assert_eq!(kept.text_bytes(), text.len(), "the text and nothing else");
}

#[test]
fn book_answers_for_a_registered_id_only() {
    let mut pantry = pantry();
    let id = mrk();
    let key = pantry
        .update(id.clone(), Role::Target, &mark())
        .unwrap()
        .key();
    assert_eq!(pantry.book(&id).unwrap().key(), key);
    assert!(pantry.book(&BookId::from("books/luk.usfm")).is_none());
}

#[test]
fn books_are_canonically_ordered_whatever_the_update_order() {
    let mut pantry = pantry();
    let revelation = book("REV", &["A revelation of Jesus Christ."]);
    let genesis = book("GEN", &["In the beginning."]);
    let mark = mark();
    // Two ids carrying the same \id are both present, ordered by id.
    pantry
        .update("z/rev.usfm", Role::Target, &revelation)
        .unwrap();
    pantry.update("b/mrk.usfm", Role::Target, &mark).unwrap();
    pantry.update("a/gen.usfm", Role::Target, &genesis).unwrap();
    pantry
        .update("a/gen-copy.usfm", Role::Target, &genesis)
        .unwrap();

    let listed: Vec<(&str, BookKey)> = pantry
        .books(Role::Target)
        .iter()
        .map(|(id, key)| (id.as_str(), *key))
        .collect();
    assert_eq!(
        listed,
        vec![
            ("a/gen-copy.usfm", BookKey::new(*b"GEN")),
            ("a/gen.usfm", BookKey::new(*b"GEN")),
            ("b/mrk.usfm", BookKey::new(*b"MRK")),
            ("z/rev.usfm", BookKey::new(*b"REV")),
        ]
    );
}

#[test]
fn remove_drops_the_products() {
    let mut pantry = pantry();
    let id = mrk();
    pantry.update(id.clone(), Role::Target, &mark()).unwrap();
    let resident = pantry.resident_bytes();
    assert!(resident > 0);

    assert!(pantry.remove(&id));
    assert!(pantry.resident_bytes() < resident);
    assert!(pantry.book(&id).is_none());
    assert_eq!(pantry.changed_since_update(&id, &mark()), None);
    assert!(pantry.books(Role::Target).is_empty());
    assert!(!pantry.remove(&id), "already gone");
}

/// A reference is TOC plus one grapheme count per verse, and nothing
/// else: it publishes no coordinate, so it retains none.
#[test]
fn a_reference_keeps_its_verse_lengths_and_no_projection() {
    let mut pantry = pantry();
    let id = mrk();
    let entry = pantry.update(id.clone(), Role::Reference, &mark()).unwrap();

    assert_eq!(entry.role(), Role::Reference);
    assert_eq!(entry.key(), BookKey::new(*b"MRK"));
    assert_eq!(
        entry.mask().err(),
        Some(PantryError::NoProjection { id: id.clone() })
    );
    assert_eq!(
        entry.utf16().err(),
        Some(PantryError::NoProjection { id: id.clone() })
    );
    assert_eq!(
        entry.published_len().err(),
        Some(PantryError::NoProjection { id: id.clone() })
    );
    assert_eq!(
        entry.text().err(),
        Some(PantryError::NoText { id: id.clone() })
    );

    let lengths = entry.verse_lengths().unwrap();
    assert_eq!(lengths.len(), 6, "one row per verse");
    // "In the beginning of the good news." plus the newline the mask keeps.
    assert_eq!(lengths[0].graphemes(), 35);
    assert_eq!(lengths[0].key(), sous_core::VerseKey::new(1, 1, 1).unwrap());

    assert!(pantry.books(Role::Target).is_empty());
    assert_eq!(
        pantry.books(Role::Reference),
        &[(id, BookKey::new(*b"MRK"))]
    );
    assert_eq!(pantry.text_bytes(), 0, "no text is retained");
}

/// The whole point of the role: the same book costs less as a reference,
/// because the text, the mask, and the UTF-16 table are the weight. The
/// margin is not a half — the word lane is comparable in size to the
/// projected text it hashes (evidence.md, U1 (a)).
#[test]
fn a_reference_costs_less_than_a_target() {
    // A book big enough that the retained text, the mask, and the UTF-16
    // table dominate the per-book struct both roles pay for.
    let chapters: Vec<String> = (0..40)
        .map(|at| format!("Chapter {at} of a book long enough to weigh something."))
        .collect();
    let text = book(
        "MRK",
        &chapters.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let mut target = pantry();
    target.update(mrk(), Role::Target, &text).unwrap();
    let mut reference = pantry();
    reference.update(mrk(), Role::Reference, &text).unwrap();

    let cached = |pantry: &Pantry| pantry.chunk_stats().resident_bytes;
    let target_own = target.resident_bytes() - cached(&target);
    let reference_own = reference.resident_bytes() - cached(&reference);
    assert!(
        reference_own < target_own,
        "reference {reference_own} B against target {target_own} B"
    );
    assert!(reference_own > 0);
    assert_eq!(reference.text_bytes(), 0);
}

/// The expensive lane is opt-in: a reference registered for lengths alone
/// weighs less and answers `NoLengths` for its words, and the SAME text
/// re-registered for both is not served from the cheaper one.
#[test]
fn a_reference_keeps_the_word_lane_only_when_it_is_asked_for() {
    let mut lean = pantry();
    lean.update(mrk(), Role::Reference, &mark()).unwrap();
    let mut full = pantry();
    full.update_with(
        mrk(),
        Role::Reference,
        Retain::ProductsOnly,
        SourceLanes::LengthsAndWords,
        &mark(),
    )
    .unwrap();

    assert!(lean.book(&mrk()).unwrap().verse_words().is_err());
    assert!(!full.book(&mrk()).unwrap().verse_words().unwrap().is_empty());
    let own = |pantry: &Pantry| pantry.resident_bytes() - pantry.chunk_stats().resident_bytes;
    assert!(
        own(&lean) < own(&full),
        "lean {} B against full {} B",
        own(&lean),
        own(&full)
    );

    // The same text under the other setting is a real update, not a hit.
    let before = lean.derivations();
    lean.update_with(
        mrk(),
        Role::Reference,
        Retain::ProductsOnly,
        SourceLanes::LengthsAndWords,
        &mark(),
    )
    .unwrap();
    assert_eq!(lean.derivations(), before + 1);
    assert_eq!(own(&lean), own(&full));
}

/// A target refuses `ProductsOnly` and a reference accepts `Text`; the
/// second is allowed and pointless, since nothing reads a reference's
/// text.
#[test]
fn a_reference_may_keep_text_it_will_never_be_asked_for() {
    let mut pantry = pantry();
    let entry = pantry
        .update_with(
            mrk(),
            Role::Reference,
            Retain::Text,
            SourceLanes::Lengths,
            &mark(),
        )
        .unwrap();
    assert_eq!(entry.text().unwrap(), mark());
    assert_eq!(entry.verse_lengths().unwrap().len(), 6);
    assert_eq!(pantry.text_bytes(), mark().len());
}

/// Both roles order canonically and neither sees the other.
#[test]
fn the_two_roles_are_ordered_and_listed_apart() {
    let mut pantry = pantry();
    pantry
        .update("t/rev.usfm", Role::Target, &book("REV", &["A revelation."]))
        .unwrap();
    pantry.update("t/mrk.usfm", Role::Target, &mark()).unwrap();
    pantry
        .update("s/mrk.usfm", Role::Reference, &mark())
        .unwrap();
    pantry
        .update(
            "s/gen.usfm",
            Role::Reference,
            &book("GEN", &["In the beginning."]),
        )
        .unwrap();

    let names = |rows: &[(BookId, BookKey)]| {
        rows.iter()
            .map(|(id, _)| id.as_str().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(pantry.books(Role::Target)),
        vec!["t/mrk.usfm", "t/rev.usfm"]
    );
    assert_eq!(
        names(pantry.books(Role::Reference)),
        vec!["s/gen.usfm", "s/mrk.usfm"]
    );
}

#[test]
fn a_target_retains_no_verse_lengths() {
    let mut pantry = pantry();
    let id = mrk();
    let entry = pantry.update(id.clone(), Role::Target, &mark()).unwrap();
    assert_eq!(
        entry.verse_lengths().err(),
        Some(PantryError::NoLengths { id })
    );
}

#[test]
fn a_book_with_no_id_line_is_refused() {
    let mut pantry = pantry();
    assert_eq!(
        pantry
            .update("scratch.usfm", Role::Target, "\\c 1\n\\p\n\\v 1 keyless\n")
            .err(),
        Some(PantryError::MissingBookKey {
            id: BookId::from("scratch.usfm")
        })
    );
    assert!(pantry.books(Role::Target).is_empty());
}

/// Installing a marker registry invalidates every product derived under the
/// old one: the same bytes are a different document, and both caches key on
/// content alone.
///
/// Serialized against the other registry-touching tests through
/// `onion::extensions`'s process-wide value; the guard restores it.
#[test]
fn a_new_marker_registry_flushes_every_derived_product() {
    use onion::extensions::{CustomMarker, ExtensionCategory, set_extensions};

    /// Clears the registry however this test ends, so no other test in the
    /// process sees a marker it did not install.
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            set_extensions(&[]);
        }
    }
    let _restore = Restore;

    set_extensions(&[]);
    let text = "\\id MRK\n\\h MRK\n\\c 1\n\\p\n\\v 1 Jesus wept\\zmyf + \\ft why\\zmyf*.\n";
    let mut pantry = Pantry::new(1 << 20);
    pantry
        .update("books/MRK.usfm", Role::Target, text)
        .expect("registers");
    let cold = pantry.chunk_stats().misses;
    assert!(cold > 0, "the first derivation missed");

    // Idempotent while nothing moves: same bytes, same registry, no work.
    pantry
        .update("books/MRK.usfm", Role::Target, text)
        .expect("registers");
    assert_eq!(
        pantry.chunk_stats().misses,
        cold,
        "a served update derives nothing"
    );
    let unregistered = pantry
        .masked(text, &Filter::verse_text())
        .text(text.as_bytes());
    assert!(
        unregistered.contains("why"),
        "row 0 is not a Note, so the note's prose rides into verse text"
    );

    // …and then the registry moves.
    let reports = set_extensions(&[CustomMarker {
        name: "zmyf".to_owned(),
        category: ExtensionCategory::Footnote,
        description: String::new(),
        attributes: Vec::new(),
    }]);
    assert!(reports.is_empty(), "{reports:?}");

    pantry
        .update("books/MRK.usfm", Role::Target, text)
        .expect("registers");
    assert!(
        pantry.chunk_stats().misses > cold,
        "identical text must MISS after the registry moved"
    );
    let registered = pantry
        .masked(text, &Filter::verse_text())
        .text(text.as_bytes());
    assert_ne!(
        registered, unregistered,
        "the same bytes now read as a footnote"
    );
    assert_eq!(registered.trim(), "Jesus wept.", "the note subtree drops");

    // The book is still registered, and its dish reflects the new rows.
    let dish = pantry
        .book(&BookId::from("books/MRK.usfm"))
        .expect("still registered")
        .parse(onion::wire::ParseOptions::default())
        .expect("a target parses");
    assert!(!dish.is_empty());
}
