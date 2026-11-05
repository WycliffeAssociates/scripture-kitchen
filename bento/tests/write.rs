//! What bento writes: valid, and an edit changes only what it means to.
//!
//! ```text
//! schema/burrito/  (the fork at b8872ee)        the validator; its $refs resolve to these files, never the network
//! create_bible(project)  → metadata.json        valid; reads back with no findings
//!                        → manifest.yaml        reads back with no findings, sort as canon
//! a Burrito as Sefer writes it today            invalid: the test can fail
//! create(MAT, ROM) + edit(add MRK)              == create(MAT, MRK, ROM), byte for byte (Burrito)
//! edit(catalog RC, add GEN)                     only GEN's lines are new; every other line as it was
//! edit(checksums)                               md5 and size brought up to date; unchanged → no file
//! ```

use std::path::{Path, PathBuf};

use bento::{
    Change, Direction, EditError, Format, NewBibleProject, NewBook, NewLanguage, Related,
    create_bible, edit, read,
};
use canon::Canon;
use mise::books::BookKey;
use serde_json::Value;

fn schema_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/burrito")
}

/// `https://burrito.bible/schema/<path>` → the vendored file.
struct Vendored;

impl jsonschema::Retrieve for Vendored {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let path = uri
            .as_str()
            .strip_prefix("https://burrito.bible/schema/")
            .ok_or_else(|| format!("not a vendored schema: {}", uri.as_str()))?;
        let bytes = std::fs::read(schema_dir().join(path))?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

fn errors(metadata: &[u8]) -> Vec<String> {
    let schema: Value = serde_json::from_slice(
        &std::fs::read(schema_dir().join("metadata.schema.json")).expect("vendored schema"),
    )
    .unwrap();
    let validator = jsonschema::options()
        .with_retriever(Vendored)
        .should_validate_formats(true)
        .build(&schema)
        .expect("the schema compiles");
    let value: Value = serde_json::from_slice(metadata).unwrap();
    validator
        .iter_errors(&value)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect()
}

fn book(code: &[u8; 3], title: &str) -> NewBook {
    NewBook {
        book: BookKey::new(*code),
        title: title.into(),
        bytes: format!("\\id {}\n\\c 1\n\\v 1 …\n", String::from_utf8_lossy(code)).into_bytes(),
    }
}

pub fn project() -> NewBibleProject {
    NewBibleProject {
        title: "Baibel".into(),
        abbreviation: "reg".into(),
        language: NewLanguage {
            tag: "tpi".into(),
            name: "Tok Pisin".into(),
            direction: Direction::Ltr,
        },
        books: vec![book(b"ROM", "Rom"), book(b"MAT", "Matyu")],
        canon: Canon::Protestant66,
        sources: vec![Related {
            language: "en".into(),
            resource: "ulb".into(),
            version: Some("12".into()),
        }],
        contributors: vec!["Mary_Kila".into(), " mary  kila ".into(), "Tomas".into()],
        publisher: "Wycliffe Associates".into(),
        creator: "Bible translation community".into(),
        rights: "CC BY-SA 4.0".into(),
        checking_level: "1".into(),
        copyright: "Licensed under CC BY-SA 4.0.".into(),
        generator: ("Sefer".into(), "1.4.0".into()),
        created_at: "2026-11-05T14:30:00Z".into(),
        repository: "Tech_Advance/tpi_reg".into(),
    }
}

#[test]
fn a_created_burrito_passes_the_forks_schema_and_reads_back_clean() {
    let files = create_bible(&project(), &[Format::Burrito, Format::Rc]).unwrap();
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["metadata.json", "manifest.yaml"]);

    let metadata = &files[0].1;
    assert_eq!(errors(metadata), Vec::<String>::new());
    let r = read("metadata.json", metadata);
    assert_eq!(r.findings, [], "{:#?}", r.findings);
    let m = r.manifest.unwrap();
    assert_eq!(m.id.as_deref(), Some("Tech_Advance/tpi_reg"));
    let books: Vec<String> = m.books.iter().map(|b| b.book.to_string()).collect();
    assert_eq!(
        books,
        ["MAT", "ROM"],
        "canon's order, whatever order they came in"
    );
    assert_eq!(m.books[0].path.as_deref(), Some("41-MAT.usfm"));
    assert_eq!(m.sources, project().sources);

    let rc = read("manifest.yaml", &files[1].1);
    assert_eq!(rc.findings, [], "{:#?}", rc.findings);
    let m = rc.manifest.unwrap();
    assert_eq!(m.books[0].sort, Some(40), "MAT is 40, as the catalog says");
    assert_eq!(m.books[1].sort, Some(45));
    assert_eq!(m.books[0].path.as_deref(), Some("./41-MAT.usfm"));
    let Some(bento::Detail::Rc(doc)) = rc.detail else {
        panic!()
    };
    assert_eq!(
        doc.dublin_core.contributor,
        ["Mary_Kila", "Tomas"],
        "one Mary Kila"
    );
    assert_eq!(doc.dublin_core.issued.as_deref(), Some("2026-11-05"));
}

// Why: the fork's localized text can't start or end with space; a name
// typed into a form often does.
#[test]
fn names_are_trimmed_so_the_schema_holds() {
    let mut p = project();
    p.title = " Baibel ".into();
    p.language.name = "Tok Pisin\n".into();
    p.books[0].title = " Rom".into();
    let files = create_bible(&p, &[Format::Burrito, Format::Rc]).unwrap();
    assert_eq!(errors(&files[0].1), Vec::<String>::new());
    assert_eq!(files, files_of(&project()));
    let added = edit(&files_of(&project()), &add(vec![book(b"MRK", " Mak ")])).unwrap();
    let mut three = project();
    three.books.push(book(b"MRK", "Mak"));
    assert_eq!(added.files[0].1, files_of(&three)[0].1);
}

#[test]
fn deuterocanon_books_need_a_canon_that_holds_them() {
    let mut p = project();
    p.books.push(book(b"TOB", "Tobit"));
    assert_eq!(
        create_bible(&p, &[Format::Burrito]),
        Err(bento::CreateError::NotInCanon(BookKey::new(*b"TOB")))
    );
    p.canon = Canon::WithDeuterocanon;
    let files = create_bible(&p, &[Format::Burrito, Format::Rc]).unwrap();
    assert_eq!(errors(&files[0].1), Vec::<String>::new());
    let rc = read("manifest.yaml", &files[1].1);
    let tob = rc
        .manifest
        .unwrap()
        .books
        .into_iter()
        .find(|b| b.book.to_string() == "TOB")
        .unwrap();
    assert_eq!(
        tob.sort,
        Some(67),
        "after the 66, in the chosen canon's order"
    );
}

#[test]
fn bad_arguments_are_refused_not_written() {
    use bento::CreateError;
    let mut p = project();
    p.books.push(book(b"MAT", "Matyu"));
    assert_eq!(
        create_bible(&p, &[Format::Rc]),
        Err(CreateError::Twice(BookKey::new(*b"MAT")))
    );
    let mut p = project();
    p.created_at = "2026-11-05".into();
    assert!(matches!(
        create_bible(&p, &[Format::Rc]),
        Err(CreateError::CreatedAt(_))
    ));
    let mut p = project();
    p.repository = "tpi_reg".into();
    assert!(matches!(
        create_bible(&p, &[Format::Rc]),
        Err(CreateError::Repository(_))
    ));
    let mut p = project();
    p.books.clear();
    assert_eq!(create_bible(&p, &[Format::Rc]), Err(CreateError::NoBooks));
}

// Why: a validator that passes everything proves nothing. A Burrito written
// as Sefer writes it today must fail, in the places the plan found.
#[test]
fn a_burrito_as_sefer_writes_it_fails_the_schema() {
    let sefer = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/burrito/sefer_written_tpi_reg.json"),
    )
    .unwrap();
    let found = errors(&sefer);
    let joined = found.join("\n");
    for expected in [
        "copyright",
        "/meta/dateCreated",
        "softwareVersion",
        "primary",
    ] {
        assert!(
            joined.contains(expected),
            "no error naming {expected}:\n{joined}"
        );
    }
}

fn files_of(p: &NewBibleProject) -> Vec<(String, Vec<u8>)> {
    create_bible(p, &[Format::Burrito, Format::Rc]).unwrap()
}

fn add(books: Vec<NewBook>) -> Change {
    Change {
        add_books: books,
        canon: Canon::Protestant66,
        checksums: Vec::new(),
    }
}

// Why: Sefer's addBooks and the hub's publishing add a book to a project
// that exists. The result must be what creating it with that book would have
// written: same order, same entries, nothing else moved.
#[test]
fn adding_a_book_is_creating_with_it() {
    let two = files_of(&project());
    let mut three = project();
    three.books.push(book(b"MRK", "Mak"));
    let three = files_of(&three);

    let edited = edit(&two, &add(vec![book(b"MRK", "Mak")])).unwrap();
    assert_eq!(edited.findings, []);
    let names: Vec<&str> = edited.files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["metadata.json", "manifest.yaml"]);
    assert!(
        edited.files[0].1 == three[0].1,
        "metadata.json:\n{}",
        String::from_utf8_lossy(&edited.files[0].1)
    );
    assert_eq!(errors(&edited.files[0].1), Vec::<String>::new());

    let rc = read("manifest.yaml", &edited.files[1].1);
    assert_eq!(rc.findings, [], "{:#?}", rc.findings);
    assert_eq!(
        rc.manifest,
        read("manifest.yaml", &three[1].1).manifest,
        "the same manifest"
    );
    let books: Vec<String> = rc
        .manifest
        .unwrap()
        .books
        .iter()
        .map(|b| b.book.to_string())
        .collect();
    assert_eq!(books, ["MAT", "MRK", "ROM"]);
}

/// The lines of `after` that aren't in `before`, if `after` is `before` with
/// one run of lines inserted.
fn inserted(before: &str, after: &str) -> Option<Vec<String>> {
    let (b, a): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
    let head = b.iter().zip(&a).take_while(|(x, y)| x == y).count();
    let tail = b[head..]
        .iter()
        .rev()
        .zip(a[head..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    (head + tail == b.len()).then(|| {
        a[head..a.len() - tail]
            .iter()
            .map(|l| (*l).to_owned())
            .collect()
    })
}

#[test]
fn adding_genesis_to_the_catalogs_rc_changes_only_its_lines() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../testData/exampleCorpora/bdf_reg/manifest.yaml");
    let before = std::fs::read_to_string(&path).unwrap();
    let edited = edit(
        &[("manifest.yaml".into(), before.clone().into_bytes())],
        &add(vec![book(b"GEN", "Kejadian")]),
    )
    .unwrap();
    let after = String::from_utf8(edited.files[0].1.clone()).unwrap();
    let new =
        inserted(&before, &after).unwrap_or_else(|| panic!("more than one run changed:\n{after}"));
    assert_eq!(new.len(), 7, "{new:#?}");
    assert!(new[0].contains("title: Kejadian"), "{new:#?}");
    assert!(new.iter().any(|l| l.trim() == "sort: 1"), "{new:#?}");
    let books: Vec<String> = read("manifest.yaml", after.as_bytes())
        .manifest
        .unwrap()
        .books
        .iter()
        .map(|b| b.book.to_string())
        .collect();
    assert_eq!(books[..2], ["GEN", "MAT"]);
}

// Why: a Sefer-written RC counts sort from 0, so a book added at canon's
// place can tie with its neighbour (ACT 44 beside ROM's 44). The edit says
// so rather than renumbering the neighbours (rule 1).
#[test]
fn a_tie_with_an_old_sort_is_said_not_fixed() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/rc/sefer_written_tpi_reg.yaml");
    let before = std::fs::read(&path).unwrap();
    let edited = edit(
        &[("manifest.yaml".into(), before)],
        &add(vec![book(b"ACT", "Aposel")]),
    )
    .unwrap();
    assert_eq!(edited.findings.len(), 1);
    assert_eq!(
        edited.findings[0].message,
        "ACT's sort, 44, ties with rom's; order by sort, then by canon, is still right."
    );
    let m = read("manifest.yaml", &edited.files[0].1).manifest.unwrap();
    let sorts: Vec<(String, Option<i64>)> = m
        .books
        .iter()
        .map(|b| (b.book.to_string(), b.sort))
        .collect();
    assert_eq!(
        sorts,
        [
            ("MAT".into(), Some(39)),
            ("ACT".into(), Some(44)),
            ("ROM".into(), Some(44))
        ]
    );
}

#[test]
fn checksums_come_up_to_date_and_nothing_else_moves() {
    let files = files_of(&project());
    let burrito = vec![files[0].clone()];
    let same = book(b"MAT", "Matyu").bytes;
    let unchanged = edit(
        &burrito,
        &Change {
            checksums: vec![("41-MAT.usfm".into(), same)],
            ..add(Vec::new())
        },
    )
    .unwrap();
    assert_eq!(unchanged.files, [], "nothing changed, no file");

    let saved = b"\\id MAT\n\\c 1\n\\v 1 Long ago.\n".to_vec();
    let edited = edit(
        &burrito,
        &Change {
            checksums: vec![
                ("./41-MAT.usfm".into(), saved.clone()),
                ("99-XXX.usfm".into(), Vec::new()),
            ],
            ..add(Vec::new())
        },
    )
    .unwrap();
    assert_eq!(
        edited.findings.len(),
        1,
        "a path with no ingredient is said"
    );
    let Some(bento::Detail::Burrito(doc)) = read("metadata.json", &edited.files[0].1).detail else {
        panic!()
    };
    let (_, mat) = &doc.ingredients[0];
    assert_eq!(
        mat.md5.as_deref(),
        Some(bento::fingerprint(&saved).md5.as_str())
    );
    assert_eq!(mat.size, Some(saved.len() as i64));
    let (before, after) = (
        String::from_utf8_lossy(&files[0].1).into_owned(),
        String::from_utf8_lossy(&edited.files[0].1).into_owned(),
    );
    let changed = before
        .lines()
        .zip(after.lines())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(changed, 2, "the md5 line and the size line");
    assert_eq!(before.lines().count(), after.lines().count());

    // The RC lists no checksums: it isn't returned.
    let rc_only = edit(
        &[files[1].clone()],
        &Change {
            checksums: vec![("41-MAT.usfm".into(), saved)],
            ..add(Vec::new())
        },
    )
    .unwrap();
    assert_eq!(rc_only.files, []);
}

#[test]
fn a_book_listed_already_is_refused() {
    let files = files_of(&project());
    assert_eq!(
        edit(&files, &add(vec![book(b"ROM", "Rom")])),
        Err(EditError::Listed(BookKey::new(*b"ROM")))
    );
}

// Why: Door43's "files exist, aren't empty, sizes and checksums match" are
// the checks a save can break. A Burrito Sefer forgot to update after a
// save says so; an RC lists no checksums, only paths.
#[test]
fn the_file_checks_compare_the_manifest_with_the_repo() {
    use bento::{Check, Repo, check};
    let p = project();
    let files = files_of(&p);
    let burrito = read("metadata.json", &files[0].1);
    let rc = read("manifest.yaml", &files[1].1);
    let repo: Vec<(String, Vec<u8>)> = vec![
        ("41-MAT.usfm".into(), book(b"MAT", "").bytes),
        ("46-ROM.usfm".into(), book(b"ROM", "").bytes),
    ];
    fn with(files: &[(String, Vec<u8>)]) -> Repo<'_> {
        Repo {
            name: Some("Tech_Advance/tpi_reg"),
            files: Some(files),
        }
    }
    assert_eq!(check(&burrito, &with(&repo)), []);
    assert_eq!(check(&rc, &with(&repo)), []);

    let saved = vec![
        ("./41-MAT.usfm".into(), b"\\id MAT changed\n".to_vec()),
        ("46-ROM.usfm".into(), Vec::new()),
    ];
    let found: Vec<(Check, Option<String>, Option<String>)> = check(&burrito, &with(&saved))
        .into_iter()
        .map(|f| (f.check, f.at, f.book.map(|b| b.to_string())))
        .collect();
    assert_eq!(
        found,
        [
            (
                Check::IngredientChecksum,
                Some("/ingredients/41-MAT.usfm".into()),
                Some("MAT".into())
            ),
            (
                Check::IngredientEmpty,
                Some("/ingredients/46-ROM.usfm".into()),
                Some("ROM".into())
            ),
            (
                Check::IngredientChecksum,
                Some("/ingredients/46-ROM.usfm".into()),
                Some("ROM".into())
            ),
        ]
    );
    let missing = check(&rc, &with(&repo[..1]));
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].check, Check::IngredientExists);
    assert_eq!(
        missing[0].message,
        "./46-ROM.usfm is listed but not in the repo."
    );
}
