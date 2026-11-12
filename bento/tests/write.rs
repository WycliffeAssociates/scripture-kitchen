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
    // `en` is the default locale, and the project's names carry it beside the project's own tag.
    let both =
        |t: &str| bento::LocalizedText::from([("en".into(), t.into()), ("tpi".into(), t.into())]);
    assert_eq!(m.name, both("Baibel"));
    assert_eq!(m.abbreviation, both("reg"));
    assert_eq!(m.language.name, both("Tok Pisin"));
    // A book's `en` is canon's English; the language's own name is under its tag.
    assert_eq!(
        m.books[0].title,
        bento::LocalizedText::from([("en".into(), "Matthew".into()), ("tpi".into(), "Matyu".into())])
    );
    let Some(bento::Detail::Burrito(doc)) = read("metadata.json", metadata).detail else {
        panic!()
    };
    assert_eq!(doc.meta.default_locale.as_deref(), Some("en"));
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

// Why: the RC edit splices text, so its edges are where it can go wrong: a
// book after every other, a manifest with no `projects` yet, a list it
// mustn't touch because it can't place a line in it.
#[test]
fn adding_after_the_last_book_and_to_no_list_at_all() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/rc/hub_tn_luk_text_reg.yaml");
    let before = std::fs::read_to_string(&path).unwrap();
    let edited = edit(
        &[("manifest.yaml".into(), before.clone().into_bytes())],
        &add(vec![book(b"REV", "Tshenolo")]),
    )
    .unwrap();
    let after = String::from_utf8(edited.files[0].1.clone()).unwrap();
    let new =
        inserted(&before, &after).unwrap_or_else(|| panic!("more than one run changed:\n{after}"));
    assert_eq!(new.len(), 7, "{new:#?}");
    assert!(after.ends_with("  versification: ufw\n"), "{after}");
    let books: Vec<String> = read("manifest.yaml", after.as_bytes())
        .manifest
        .unwrap()
        .books
        .iter()
        .map(|b| b.book.to_string())
        .collect();
    assert_eq!(books, ["LUK", "REV"]);

    let bare =
        "dublin_core:\n  identifier: reg\n  title: Baibel\n  language:\n    identifier: tpi\n";
    let edited = edit(
        &[("manifest.yaml".into(), bare.as_bytes().to_vec())],
        &add(vec![book(b"MAT", "Matyu")]),
    )
    .unwrap();
    let after = String::from_utf8(edited.files[0].1.clone()).unwrap();
    assert!(after.starts_with(bare), "{after}");
    let r = read("manifest.yaml", after.as_bytes());
    assert_eq!(r.manifest.unwrap().books[0].sort, Some(40), "{after}");

    let flow =
        "dublin_core:\n  identifier: reg\nprojects: [{identifier: mat, path: ./41-MAT.usfm}]\n";
    let refused = edit(
        &[("manifest.yaml".into(), flow.as_bytes().to_vec())],
        &add(vec![book(b"MRK", "Mak")]),
    );
    assert!(
        matches!(refused, Err(EditError::Unreadable(_, ref why)) if why.contains("flow list")),
        "{refused:?}"
    );
}

fn added(before: &str, code: &[u8; 3], title: &str) -> String {
    let edited = edit(
        &[("manifest.yaml".into(), before.as_bytes().to_vec())],
        &add(vec![book(code, title)]),
    )
    .unwrap();
    String::from_utf8(edited.files[0].1.clone()).unwrap()
}

fn paths(yaml: &str) -> Vec<(String, String)> {
    read("manifest.yaml", yaml.as_bytes())
        .manifest
        .unwrap()
        .books
        .iter()
        .map(|b| (b.book.to_string(), b.path.clone().unwrap_or_default()))
        .collect()
}

// Why: the splice anchors on lines, and three shapes of valid YAML once put
// the new lines inside another item or another document. Each answer is
// pinned as text, since bento's own reader forgives what a strict one won't.
#[test]
fn the_splice_finds_an_items_dash_its_end_and_the_documents_end() {
    // A dash alone on its line, its mapping below.
    let dash = "projects:\n  -\n    identifier: gen\n    title: Genesis\n    path: ./01-GEN.usfm\n";
    let after = added(dash, b"MRK", "Mak");
    assert!(after.starts_with(dash), "{after}");
    assert!(
        after[dash.len()..].starts_with("  - title: Mak\n"),
        "{after}"
    );
    assert_eq!(
        paths(&after),
        [
            ("GEN".into(), "./01-GEN.usfm".into()),
            ("MRK".into(), "./42-MRK.usfm".into())
        ]
    );
    // Before it: GEN goes ahead of EXO, so the insert lands on EXO's dash line.
    let exo = "projects:\n  -\n    identifier: exo\n    path: ./02-EXO.usfm\n";
    let after = added(exo, b"GEN", "Stat");
    assert!(after.starts_with("projects:\n  - title: Stat\n"), "{after}");
    assert!(after.ends_with(&exo["projects:\n".len()..]), "{after}");
    assert_eq!(paths(&after).len(), 2, "{after}");

    // A comment at column 0 inside the last item doesn't end it.
    let comment =
        "projects:\n  - identifier: gen\n# note\n    title: Genesis\n    path: ./01-GEN.usfm\n";
    let after = added(comment, b"MRK", "Mak");
    assert!(after.starts_with(comment), "{after}");
    assert_eq!(paths(&after)[1], ("MRK".into(), "./42-MRK.usfm".into()));

    // Comments and blanks after the list stay after the new item.
    let trailing =
        "projects:\n  - identifier: gen\n    path: ./01-GEN.usfm\n\n# the rest\nextra: 1\n";
    let after = added(trailing, b"MRK", "Mak");
    assert!(after.ends_with("\n\n# the rest\nextra: 1\n"), "{after}");
    assert!(
        after.contains("path: ./01-GEN.usfm\n  - title: Mak\n"),
        "{after}"
    );

    // No list, and the document closes with `...`: the list goes inside it.
    let closed = "dublin_core:\n  identifier: reg\n...\n";
    let after = added(closed, b"MAT", "Matyu");
    assert!(
        after.starts_with("dublin_core:\n  identifier: reg\nprojects:\n"),
        "{after}"
    );
    assert!(after.ends_with("...\n"), "{after}");
    assert_eq!(paths(&after), [("MAT".into(), "./41-MAT.usfm".into())]);
    let opened = "---\ndublin_core:\n  identifier: reg\n---\nother: 1\n";
    let after = added(opened, b"MAT", "Matyu");
    assert!(after.ends_with("---\nother: 1\n"), "{after}");
    assert_eq!(paths(&after).len(), 1, "{after}");
}

// --- Book names ---------------------------------------------------------------

fn names(code: &[u8; 3], short: &str, long: &str, abbr: &str) -> bento::BookNames {
    let some = |s: &str| Some(s.to_owned());
    bento::BookNames {
        book: BookKey::new(*code),
        short: some(short),
        long: some(long),
        abbr: some(abbr),
    }
}

fn open(files: &[(String, Vec<u8>)]) -> bento::Manifests {
    bento::Manifests::open(files, Canon::Protestant66).unwrap()
}

/// The lines that differ, as (before, after), for files of the same length.
fn changed_lines(before: &[u8], after: &[u8]) -> Vec<(String, String)> {
    let (b, a) = (String::from_utf8_lossy(before), String::from_utf8_lossy(after));
    assert_eq!(b.lines().count(), a.lines().count(), "{a}");
    b.lines()
        .zip(a.lines())
        .filter(|(x, y)| x != y)
        .map(|(x, y)| (x.to_owned(), y.to_owned()))
        .collect()
}

// Why: the manifest follows the book (the RFC's rule). A rename writes the
// language's names under its tag, keeps canon's English beside them, and
// moves nothing else; the RC takes the short name as its title.
#[test]
fn a_rename_writes_the_languages_names_and_nothing_else() {
    let files = files_of(&project());
    let mut m = open(&files);
    let found = m.rename_book(names(b"MAT", " Gutpela Nius Matyu ", "Gutnius bilong Matyu", "Mt")).unwrap();
    assert_eq!(found, []);
    let edited = m.finish();
    assert_eq!(edited.findings, []);
    let (burrito, rc) = (&edited.files[0].1, &edited.files[1].1);
    assert_eq!(errors(burrito), Vec::<String>::new());
    let Some(bento::Detail::Burrito(doc)) = read("metadata.json", burrito).detail else {
        panic!()
    };
    let (code, mat) = &doc.localized_names[0];
    assert_eq!(code, "MAT");
    let text = |pairs: &[(&str, &str)]| -> bento::LocalizedText {
        pairs.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect()
    };
    assert_eq!(mat.short, text(&[("en", "Matthew"), ("tpi", "Gutpela Nius Matyu")]));
    assert_eq!(mat.long, text(&[("tpi", "Gutnius bilong Matyu")]));
    assert_eq!(mat.abbr, text(&[("tpi", "Mt")]));
    // The short name's line changes; long and abbr are new lines inside MAT.
    let (before, after) = (String::from_utf8_lossy(&files[0].1), String::from_utf8_lossy(burrito));
    assert_eq!(after.lines().count(), before.lines().count() + 6, "{after}");

    let changed = changed_lines(&files[1].1, rc);
    assert_eq!(
        changed,
        [("  - title: Matyu".into(), "  - title: Gutpela Nius Matyu".into())]
    );
}

#[test]
fn renaming_to_the_names_already_there_returns_no_file() {
    let files = files_of(&project());
    let mut m = open(&files);
    m.rename_book(bento::BookNames {
        book: BookKey::new(*b"MAT"),
        short: Some("Matyu".into()),
        long: Some("  ".into()),
        abbr: None,
    })
    .unwrap();
    let edited = m.finish();
    assert_eq!(edited.files, [], "an empty name is no name; the short one is as it was");
}

// Why: bento used to copy the language's name under `en`. A rename replaces
// such a copy with canon's English; an `en` name someone wrote stays.
#[test]
fn an_en_copy_becomes_english_and_a_real_one_stays() {
    let files = files_of(&project());
    let old = String::from_utf8(files[0].1.clone())
        .unwrap()
        .replace("\"en\": \"Matthew\"", "\"en\": \"Matyu\"")
        .replace("\"en\": \"Romans\"", "\"en\": \"Rome letter\"");
    let old = vec![("metadata.json".to_owned(), old.into_bytes())];
    let mut m = open(&old);
    m.rename_book(bento::BookNames { short: Some("Matyu Gutnius".into()), long: None, abbr: None, book: BookKey::new(*b"MAT") }).unwrap();
    m.rename_book(bento::BookNames { short: Some("Rom Pas".into()), long: None, abbr: None, book: BookKey::new(*b"ROM") }).unwrap();
    let names: Vec<(String, Option<String>)> = {
        let Some(bento::Detail::Burrito(doc)) = read("metadata.json", &m.clone().finish().files[0].1).detail else {
            panic!()
        };
        doc.localized_names.iter().map(|(k, n)| (k.clone(), n.short.get("en").cloned())).collect()
    };
    assert_eq!(
        names,
        [("MAT".into(), Some("Matthew".into())), ("ROM".into(), Some("Rome letter".into()))]
    );
}

// Why: one stray book shouldn't block the rest, and a manifest that can't say
// whose language a name is in isn't guessed at ("und").
#[test]
fn what_a_rename_cant_do_is_a_warning_and_the_rest_goes_ahead() {
    let files = files_of(&project());
    let mut m = open(&files);
    let found = m.rename_book(names(b"JHN", "Jon", "", "")).unwrap();
    let messages: Vec<&str> = found.iter().map(|f| f.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "metadata.json doesn't list JHN, so its names weren't changed.",
            "manifest.yaml doesn't list JHN, so its title wasn't changed."
        ]
    );
    m.rename_book(names(b"ROM", "Rom Pas", "", "")).unwrap();
    let edited = m.finish();
    assert_eq!(edited.findings.len(), 2, "finish says every call's findings");
    assert_eq!(edited.files.len(), 2, "ROM was renamed in both");

    let untagged = String::from_utf8(files[0].1.clone()).unwrap().replace("\"tag\": \"tpi\",", "");
    let mut m = open(&[("metadata.json".into(), untagged.into_bytes())]);
    let found = m.rename_book(names(b"MAT", "Matyu Gutnius", "", "")).unwrap();
    assert_eq!(found[0].message, "metadata.json names no language tag, so MAT's names weren't changed.");
    assert_eq!(m.finish().files, []);
}

// Why: the fork requires `short` on a book's names, so a book with no entry
// gets one only from a rename that gives a short name.
#[test]
fn a_book_with_no_names_gets_an_entry_placed_by_canon() {
    let files = files_of(&project());
    let mut doc: Value = serde_json::from_slice(&files[0].1).unwrap();
    doc["localizedNames"].as_object_mut().unwrap().remove("MAT");
    let bare = vec![("metadata.json".to_owned(), serde_json::to_vec_pretty(&doc).unwrap())];

    let mut m = open(&bare);
    let found = m.rename_book(bento::BookNames { book: BookKey::new(*b"MAT"), short: None, long: Some("Gutnius".into()), abbr: None }).unwrap();
    assert!(found[0].message.contains("needs a short one"), "{found:?}");
    m.rename_book(names(b"MAT", "Matyu", "", "")).unwrap();
    let after = m.finish().files[0].1.clone();
    assert_eq!(errors(&after), Vec::<String>::new());
    let Some(bento::Detail::Burrito(doc)) = read("metadata.json", &after).detail else { panic!() };
    let codes: Vec<&str> = doc.localized_names.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(codes, ["MAT", "ROM"]);
    assert_eq!(doc.localized_names[0].1.short.get("en").map(String::as_str), Some("Matthew"));
}

// Why: Sefer's Record sends a new book in both calls (its \toc lines are
// new). In order, the add lists it and the rename names it: long and abbr
// reach the Burrito, and the RC has one title.
#[test]
fn add_then_rename_in_one_handle_is_one_consistent_file() {
    let files = files_of(&project());
    let mut m = open(&files);
    m.add_book(book(b"MRK", "Mak")).unwrap();
    m.rename_book(names(b"MRK", "Gutnius Mak", "Gutnius bilong Mak", "Mk")).unwrap();
    m.update_checksum("41-MAT.usfm", b"\\id MAT\n\\c 1\n\\v 1 New.\n");
    let edited = m.finish();
    assert_eq!(edited.findings, []);
    assert_eq!(errors(&edited.files[0].1), Vec::<String>::new());
    let rc = String::from_utf8(edited.files[1].1.clone()).unwrap();
    assert_eq!(rc.matches("title: Gutnius Mak").count(), 1, "{rc}");
    assert!(!rc.contains("title: Mak\n"), "{rc}");
    let r = read("manifest.yaml", rc.as_bytes());
    assert_eq!(r.findings, []);
}

#[test]
fn a_call_that_fails_changes_nothing() {
    let files = files_of(&project());
    let mut m = open(&files);
    assert_eq!(m.add_book(book(b"ROM", "Rom")), Err(EditError::Listed(BookKey::new(*b"ROM"))));
    assert!(m.add_book(book(b"TOB", "Tobit")).is_err());
    assert_eq!(m.finish(), bento::Edited { files: vec![], findings: vec![] });
}

#[test]
fn names_reads_each_manifest_in_the_projects_language() {
    let files = files_of(&project());
    let mut m = open(&files);
    m.rename_book(names(b"ROM", "Rom Pas", "Pas bilong Rom", "Ro")).unwrap();
    let got = m.names();
    assert_eq!(got[0].0, "metadata.json");
    assert_eq!(got[0].1[1], names(b"ROM", "Rom Pas", "Pas bilong Rom", "Ro"));
    assert_eq!(got[1].0, "manifest.yaml");
    assert_eq!(
        got[1].1[1],
        bento::BookNames { book: BookKey::new(*b"ROM"), short: Some("Rom Pas".into()), long: None, abbr: None }
    );
}

fn renamed_rc(before: &str, code: &[u8; 3], short: &str) -> (String, Vec<bento::ManifestFinding>) {
    let mut m = open(&[("manifest.yaml".into(), before.as_bytes().to_vec())]);
    m.rename_book(bento::BookNames { book: BookKey::new(*code), short: Some(short.into()), long: None, abbr: None }).unwrap();
    let edited = m.finish();
    let after = edited.files.first().map_or_else(|| before.to_owned(), |(_, b)| String::from_utf8(b.clone()).unwrap());
    (after, edited.findings)
}

// Why: an RC is edited as text, so the only bytes that change are the
// title's, in the quoting it had, in any list style: the catalog's block
// lists, comments, flow lists and flow items, a BOM.
#[test]
fn an_rc_title_changes_in_place_in_every_list_style() {
    let block = "\u{FEFF}projects:\n  - title: 'John'   # English\n    identifier: jhn\n  -\n    title: \"2 John\"\n    identifier: 2jn\n";
    let (after, found) = renamed_rc(block, b"JHN", "Yohani's");
    assert_eq!(found, []);
    assert_eq!(after, block.replace("'John'", "'Yohani''s'"));
    let (after, _) = renamed_rc(block, b"2JN", "2 \"Yohani\"");
    assert_eq!(after, block.replace("\"2 John\"", "\"2 \\\"Yohani\\\"\""));

    let flow = "projects: [{title: John, identifier: jhn}, {identifier: 2jn, title: 2 John}]  # tail\n";
    let (after, _) = renamed_rc(flow, b"2JN", "2 Yohani, Pas");
    assert_eq!(after, "projects: [{title: John, identifier: jhn}, {identifier: 2jn, title: \"2 Yohani, Pas\"}]  # tail\n");
    assert_eq!(paths(&after).len(), 2);

    let mixed = "projects:\n- {title: John, identifier: jhn}\n- title: 2 John # c\n  identifier: 2jn\n";
    let (after, _) = renamed_rc(mixed, b"JHN", "Yohani");
    assert_eq!(after, mixed.replace("{title: John,", "{title: Yohani,"));

    let untitled = "projects:\n  - identifier: jhn\n    path: ./44-JHN.usfm\n";
    let (after, _) = renamed_rc(untitled, b"JHN", "Yohani");
    assert_eq!(after, "projects:\n  - identifier: jhn\n    path: ./44-JHN.usfm\n    title: Yohani\n");

    let empty = "projects:\n  - title:\n    identifier: jhn\n  - identifier: 2jn\n    title:\n";
    let (after, _) = renamed_rc(empty, b"JHN", "Jon: Gutnius");
    assert_eq!(after, "projects:\n  - title: \"Jon: Gutnius\"\n    identifier: jhn\n  - identifier: 2jn\n    title:\n");
    let (after, _) = renamed_rc(empty, b"2JN", "2 Jon");
    assert!(after.ends_with("    title: 2 Jon\n"), "{after}");
    assert_eq!(paths(&after).len(), 2, "{after}");

    let twice = "projects:\n  - title: John\n    identifier: jhn\n  - title: John\n    identifier: jhn\n";
    let (after, found) = renamed_rc(twice, b"JHN", "Yohani");
    assert_eq!(after.matches("title: Yohani").count(), 2, "{after}");
    assert_eq!(found[0].message, "manifest.yaml lists JHN 2 times; each one's title was changed.");
}

#[test]
fn the_catalogs_rc_renames_one_line() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../testData/exampleCorpora/bdf_reg/manifest.yaml");
    let before = std::fs::read_to_string(&path).unwrap();
    let (after, found) = renamed_rc(&before, b"MAT", "Injil Matius");
    assert_eq!(found, []);
    let changed = changed_lines(before.as_bytes(), after.as_bytes());
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert!(changed[0].1.ends_with("title: Injil Matius"), "{changed:?}");
}

// Why: a Burrito's `en` names are canon's English now, so "the title is the
// English name" looks only at the language's own.
#[test]
fn an_english_name_beside_the_languages_own_isnt_untranslated() {
    use bento::Check;
    let titles = |bytes: &[u8]| -> usize {
        read("metadata.json", bytes)
            .findings
            .iter()
            .filter(|f| f.check == Check::BookTitleTranslated)
            .count()
    };
    let files = files_of(&project());
    assert_eq!(titles(&files[0].1), 0, "MAT's en is Matthew, its tpi Matyu");
    let mut m = open(&files);
    m.rename_book(names(b"MAT", "Matthew", "", "")).unwrap();
    assert_eq!(titles(&m.finish().files[0].1), 1, "its tpi name is the English one");
    // A book with only an English name has none in the language: still said.
    let mut doc: Value = serde_json::from_slice(&files[0].1).unwrap();
    doc["localizedNames"]["MAT"]["short"].as_object_mut().unwrap().remove("tpi");
    assert_eq!(titles(&serde_json::to_vec(&doc).unwrap()), 1);
}
