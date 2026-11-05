//! A registered book's declared versification, end to end through the
//! coordinator: which scheme it follows, and the two findings that join its
//! diagnostics.
//!
//! Instrument: SHAPES. Small books written here, one claim each.

use canon::SchemeBuilder;
use sous_core::Brigade;
use usfm_galley::onion::lint::Code;
use usfm_galley::pantry::BookId;
use usfm_galley::sous::Expediter;
use usfm_galley::versification::Declared;
use usfm_galley::{Retain, Role};

const BUDGET: usize = 1 << 20;

/// Ruth with chapter 1 one verse short of ulb's 22, and chapter 2 one past
/// its 23.
fn ruth() -> String {
    let mut text = String::from("\\id RUT\n\\c 1\n\\p\n");
    for v in 1..=21 {
        text.push_str(&format!("\\v {v} a\n"));
    }
    text.push_str("\\c 2\n\\p\n");
    for v in 1..=24 {
        text.push_str(&format!("\\v {v} b\n"));
    }
    text
}

fn codes(sous: &mut Expediter<Brigade>, id: &str) -> Vec<Code> {
    let report = sous.book_lint(&BookId::from(id)).unwrap().unwrap();
    report
        .observations
        .iter()
        .map(|o| o.code)
        .filter(|c| matches!(c, Code::ChapterEndsEarly | Code::VerseBeyondScheme))
        .collect()
}

#[test]
fn a_declared_book_gets_both_findings_and_an_undeclared_one_neither() {
    let mut sous = Expediter::new(Brigade::default(), BUDGET);
    sous.update("ruth", Role::Target, &ruth()).unwrap();
    assert!(codes(&mut sous, "ruth").is_empty(), "nothing declared");

    sous.declare("ruth", Some(Declared::Scheme("ulb".into())))
        .unwrap();
    assert_eq!(
        codes(&mut sous, "ruth"),
        [Code::ChapterEndsEarly, Code::VerseBeyondScheme]
    );
    assert_eq!(
        sous.scheme_of(&BookId::from("ruth")).map(|s| s.name()),
        Some("ulb")
    );

    sous.declare("ruth", None).unwrap();
    assert!(codes(&mut sous, "ruth").is_empty(), "cleared");
}

#[test]
fn a_paired_book_leaves_missing_and_extra_to_presence() {
    let mut sous = Expediter::new(Brigade::default(), BUDGET);
    sous.update("ruth", Role::Target, &ruth()).unwrap();
    sous.declare("ruth", Some(Declared::Scheme("ulb".into())))
        .unwrap();
    sous.update_with("src/ruth", Role::Reference, Retain::ProductsOnly, &ruth())
        .unwrap();
    assert_eq!(
        sous.source_of(&BookId::from("ruth")).map(BookId::as_str),
        Some("src/ruth")
    );
    assert!(codes(&mut sous, "ruth").is_empty());
}

#[test]
fn follow_source_takes_the_sources_declaration() {
    let mut sous = Expediter::new(Brigade::default(), BUDGET);
    sous.update("ruth", Role::Target, &ruth()).unwrap();
    sous.declare("ruth", Some(Declared::FollowSource)).unwrap();
    assert!(
        sous.scheme_of(&BookId::from("ruth")).is_none(),
        "no source yet"
    );

    sous.update_with("src/ruth", Role::Reference, Retain::ProductsOnly, &ruth())
        .unwrap();
    sous.declare("src/ruth", Some(Declared::Scheme("rso".into())))
        .unwrap();
    assert_eq!(
        sous.scheme_of(&BookId::from("ruth")).map(|s| s.name()),
        Some("rso")
    );
}

#[test]
fn a_defined_scheme_is_declared_by_name_and_an_unknown_name_is_refused() {
    let mut sous = Expediter::new(Brigade::default(), BUDGET);
    sous.update("ruth", Role::Target, &ruth()).unwrap();
    let err = sous
        .declare("ruth", Some(Declared::Scheme("short-ruth".into())))
        .unwrap_err();
    assert!(err.to_string().contains("defineScheme"), "{err}");

    let mut b = SchemeBuilder::new("short-ruth").unwrap();
    b.book(canon::BookKey::new(*b"RUT"), vec![21, 24, 18, 22])
        .unwrap();
    sous.define_scheme(b.build().unwrap());
    sous.declare("ruth", Some(Declared::Scheme("short-ruth".into())))
        .unwrap();
    assert!(codes(&mut sous, "ruth").is_empty(), "exactly its counts");
}

#[test]
fn removing_a_book_forgets_its_declaration() {
    let mut sous = Expediter::new(Brigade::default(), BUDGET);
    sous.update("ruth", Role::Target, &ruth()).unwrap();
    sous.declare("ruth", Some(Declared::Scheme("ulb".into())))
        .unwrap();
    sous.remove(&BookId::from("ruth"));
    assert!(
        sous.versification()
            .declared(&BookId::from("ruth"))
            .is_none()
    );
}

#[test]
fn the_plated_diagnostics_carry_the_findings_and_are_unchanged_without_them() {
    let mut sous = Expediter::new(Brigade::default(), BUDGET);
    sous.update("ruth", Role::Target, &ruth()).unwrap();
    let opts = usfm_galley::onion::wire::ParseOptions {
        diagnostics: true,
        ..Default::default()
    };
    let id = BookId::from("ruth");
    let plain = sous.book_parse(&id, opts).unwrap().unwrap();
    let direct = sous.book(&id).unwrap().parse(opts).unwrap();
    assert_eq!(plain, direct, "no declaration: exactly the old bytes");

    sous.declare("ruth", Some(Declared::Scheme("ulb".into())))
        .unwrap();
    let declared = sous.book_parse(&id, opts).unwrap().unwrap();
    assert_ne!(declared, plain, "two more findings crossed the wire");
}

/// Psalm 3 numbered two ways: the target as eng (title as `\d`, verses
/// 1-8), its source as org (title as verse 1, verses 2-9).
fn psalm_three(org_numbering: bool) -> String {
    let lines = [
        "A psalm of David, when he fled from Absalom his son.",
        "Yahweh, how many are my enemies!",
        "Many are saying of me, there is no help for him in God.",
        "But you, Yahweh, are a shield around me, my glory.",
        "I lift up my voice to Yahweh, and he answers me from his holy hill.",
        "I lay down and slept; I awoke, for Yahweh sustains me.",
        "I will not be afraid of the ten thousands of people.",
        "Arise, Yahweh! Save me, my God! For you strike all my enemies.",
        "Salvation comes from Yahweh. May your blessing be on your people.",
    ];
    let mut text = String::from("\\id PSA\n\\c 3\n");
    if org_numbering {
        for (at, line) in lines.iter().enumerate() {
            text.push_str(&format!("\\q1\n\\v {} {line}\n", at + 1));
        }
    } else {
        text.push_str(&format!("\\d {}\n", lines[0]));
        for (at, line) in lines[1..].iter().enumerate() {
            text.push_str(&format!("\\q1\n\\v {} {line}\n", at + 1));
        }
    }
    text
}

fn presence_rows(sous: &mut Expediter<Brigade>) -> usize {
    let bytes = sous.publish().unwrap();
    let snapshot = sous_core::CorpusSnapshot::open(&bytes).unwrap();
    let mut presence = 0;
    for index in 0..snapshot.len() {
        let book = snapshot
            .book(sous_core::BookIndex::new(index).unwrap())
            .unwrap();
        for row in 0..book.len() {
            if let sous_core::FindingKind::Presence(_) = book.at(row).unwrap().kind() {
                presence += 1;
            }
        }
    }
    presence
}

/// The pairing itself, across schemes: undeclared, every verse pairs one
/// number off and org's 3:9 looks missing; declared eng against org, every
/// verse pairs with its counterpart and org's title verse is no missing verse.
#[test]
fn a_psalm_numbered_two_ways_pairs_cleanly_once_both_sides_declare() {
    let mut sous = Expediter::new(Brigade::default(), BUDGET);
    sous.update("books/PSA.usfm", Role::Target, &psalm_three(false))
        .unwrap();
    sous.update_with(
        "ref/PSA.usfm",
        Role::Reference,
        Retain::ProductsOnly,
        &psalm_three(true),
    )
    .unwrap();
    let undeclared = presence_rows(&mut sous);
    assert!(undeclared > 0, "as written, the source's 3:9 has no target");

    sous.declare("books/PSA.usfm", Some(Declared::Scheme("eng".into())))
        .unwrap();
    sous.declare("ref/PSA.usfm", Some(Declared::Scheme("org".into())))
        .unwrap();
    assert_eq!(
        presence_rows(&mut sous),
        0,
        "across the schemes, nothing is missing"
    );

    // A declaration that moves re-pairs, though neither text changed.
    sous.declare("books/PSA.usfm", Some(Declared::Scheme("org".into())))
        .unwrap();
    assert_eq!(
        presence_rows(&mut sous),
        undeclared,
        "org against org: numbers as written"
    );
}
