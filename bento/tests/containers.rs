//! Real manifests, read: GOLDEN.
//!
//! ```text
//! testData/…/en_ulb, en_ult, bdf_reg            the catalog's RCs: Bible, sort as canon, no findings
//! fixtures/rc/sefer_llx_reg.yaml                a Sefer project's RC
//! fixtures/rc/sefer_written_tpi_reg.yaml        exactly as Sefer's resourceContainerText writes: sort from 0
//! fixtures/rc/hub_tn_luk_text_reg.yaml          as the hub's synthetic consolidation writes
//! fixtures/burrito/sefer_written_tpi_reg.json   exactly as Sefer's burritoText writes: the fork's schema
//!                                               fails it in four places, and reading says so
//! fixtures/burrito/sefer_en_ulb.json            Sefer's hand-written Burrito fixture: primary breaks the fork's rules
//! testData/…/examples.bsb/metadata.json         a Burrito that's only a copyright
//! fixtures/bttwriter/kpass_tmf_3jn_text_reg.json  a Writer project
//! ```
//!
//! Each one's common shape and findings are pinned. A missing fixture fails.

use std::path::Path;

use bento::{Check, Format, Read, Repo, Severity, check, find, read};

fn fixture(path: &str) -> Vec<u8> {
    let full = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    std::fs::read(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()))
}

fn books(read: &Read) -> String {
    let m = read.manifest.as_ref().unwrap();
    m.books
        .iter()
        .map(|b| b.book.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn findings(read: &Read) -> Vec<String> {
    read.findings
        .iter()
        .map(|f| format!("{} {}", f.check.id(), f.at.as_deref().unwrap_or("-")))
        .collect()
}

fn sources(read: &Read) -> Vec<String> {
    let m = read.manifest.as_ref().unwrap();
    m.sources
        .iter()
        .map(|s| {
            format!(
                "{}/{} {}",
                s.language,
                s.resource,
                s.version.as_deref().unwrap_or("-")
            )
        })
        .collect()
}

const NT: &str = "MAT MRK LUK JHN ACT ROM 1CO 2CO GAL EPH PHP COL 1TH 2TH 1TI 2TI TIT PHM HEB JAS 1PE 2PE 1JN 2JN 3JN JUD REV";

#[test]
fn the_catalogs_rcs_read_cleanly() {
    let ulb = read(
        "manifest.yaml",
        &fixture("../testData/exampleCorpora/en_ulb/manifest.yaml"),
    );
    let m = ulb.manifest.as_ref().unwrap();
    assert_eq!(m.format, Format::Rc);
    assert_eq!(m.language.tag, "en");
    assert_eq!(m.name["en"], "Unlocked Literal Bible");
    assert_eq!(m.abbreviation["en"], "ulb");
    assert_eq!(m.kind.subject.as_deref(), Some("Bible"));
    assert_eq!(m.kind.domain, Some(bento::Domain::Scripture));
    assert_eq!(m.books.len(), 66);
    assert_eq!(m.books[39].book.to_string(), "MAT");
    assert_eq!(
        m.books[39].sort,
        Some(40),
        "the catalog's sort is canon's place"
    );
    assert_eq!(m.books[39].path.as_deref(), Some("./41-MAT.usfm"));
    assert_eq!(sources(&ulb), ["en/asv 1901"]);
    assert_eq!(m.version.as_deref(), Some("24-07"));
    assert_eq!(
        m.issued.as_deref(),
        Some("2024-07-12"),
        "an unquoted date reads as its text"
    );
    assert_eq!(findings(&ulb), Vec::<String>::new());

    let ult = read(
        "manifest.yaml",
        &fixture("../testData/stressCorpora/en_ult/manifest.yaml"),
    );
    let m = ult.manifest.as_ref().unwrap();
    assert_eq!(m.kind.subject.as_deref(), Some("Bible"), "Aligned Bible");
    assert_eq!(m.kind.declared.subject.as_deref(), Some("Aligned Bible"));
    assert_eq!(m.kind.declared.format.as_deref(), Some("text/usfm3"));
    assert_eq!(m.books.len(), 67, "front matter is a book of canon's table");
    assert_eq!(m.books[0].book.to_string(), "FRT");
    assert_eq!(
        sources(&ult),
        [
            "en/asv 1901",
            "hbo/uhb 2.1.26",
            "el-x-koine/ugnt 0.26",
            "en/ult 87"
        ]
    );
    assert_eq!(findings(&ult), Vec::<String>::new());

    let bdf = read(
        "manifest.yaml",
        &fixture("../testData/exampleCorpora/bdf_reg/manifest.yaml"),
    );
    assert_eq!(books(&bdf), NT);
    assert_eq!(
        bdf.manifest.as_ref().unwrap().books[0].title["bdf"],
        "Matiu"
    );
    assert_eq!(findings(&bdf), Vec::<String>::new());
}

#[test]
fn rcs_written_by_sefer_and_the_hub() {
    let llx = read("manifest.yaml", &fixture("fixtures/rc/sefer_llx_reg.yaml"));
    assert_eq!(books(&llx), NT);
    assert_eq!(
        sources(&llx),
        ["en/ulb 21-05", "en/ulb 9", "en/ulb 12", "en/ulb 24-02"]
    );
    assert_eq!(findings(&llx), ["book-title-translated /projects/2/title"]);

    // Sefer's `sortOf` counts from 0: every book one short of canon's place.
    let tpi = read(
        "manifest.yaml",
        &fixture("fixtures/rc/sefer_written_tpi_reg.yaml"),
    );
    assert_eq!(books(&tpi), "MAT ROM");
    assert_eq!(
        findings(&tpi),
        ["sort-canon /projects/0/sort", "sort-canon /projects/1/sort"]
    );
    assert_eq!(
        tpi.findings[0].book.map(|b| b.to_string()).as_deref(),
        Some("MAT")
    );
    assert_eq!(
        tpi.findings[0].message,
        "MAT's sort is 39; its place in the canon is 40."
    );

    let tn = read(
        "manifest.yaml",
        &fixture("fixtures/rc/hub_tn_luk_text_reg.yaml"),
    );
    assert_eq!(books(&tn), "LUK");
    assert_eq!(
        findings(&tn),
        [
            "book-title-translated /projects/0/title",
            "sort-canon /projects/0/sort"
        ]
    );
}

// Why: the plan found Sefer's metadata.json failing WA's schema in four
// places. Reading must say each, so `check` can report projects Sefer has
// already written, and so `createBible` (step 3) can be shown not to.
#[test]
fn a_burrito_as_sefer_writes_it_fails_the_forks_rules_in_four_places() {
    let tpi = read(
        "metadata.json",
        &fixture("fixtures/burrito/sefer_written_tpi_reg.json"),
    );
    let m = tpi.manifest.as_ref().unwrap();
    assert_eq!(m.format, Format::Burrito);
    assert_eq!(m.id, None);
    assert_eq!(m.language.tag, "tpi");
    assert_eq!(m.kind.flavor.as_deref(), Some("textTranslation"));
    assert_eq!(
        m.kind.declared.format.as_deref(),
        Some("text/x-usfm"),
        "as written"
    );
    assert_eq!(books(&tpi), "MAT ROM");
    assert_eq!(m.books[0].path.as_deref(), Some("41-MAT.usfm"));
    assert_eq!(m.books[0].title["tpi"], "Matyu");
    assert_eq!(sources(&tpi), ["en/ulb 12"]);
    assert_eq!(
        findings(&tpi),
        [
            "manifest-valid /meta/dateCreated",
            "manifest-valid /meta/generator/softwareVersion",
            "manifest-valid /identification/primary",
            "manifest-valid /copyright",
        ]
    );
    assert!(tpi.findings.iter().all(|f| f.severity == Severity::Warning));
    assert!(!tpi.has_errors(), "a project Sefer wrote still opens");

    let ulb = read(
        "metadata.json",
        &fixture("fixtures/burrito/sefer_en_ulb.json"),
    );
    assert_eq!(ulb.manifest.as_ref().unwrap().id.as_deref(), Some("en_ulb"));
    assert_eq!(
        books(&ulb),
        "PSA PHM 3JN JUD",
        "the file's order, not sorted"
    );
    assert_eq!(
        findings(&ulb),
        [
            "manifest-valid /identification/primary/wycliffeassociates/en_ulb",
            "manifest-valid /identification/primary/wycliffeassociates/en_ulb/revision",
            "manifest-valid /identification/primary/wycliffeassociates/en_ulb/timestamp",
        ]
    );

    let bsb = read(
        "metadata.json",
        &fixture("../testData/exampleCorpora/examples.bsb/metadata.json"),
    );
    assert_eq!(
        findings(&bsb),
        [
            "manifest-valid /format",
            "manifest-valid /meta",
            "manifest-valid /identification",
            "manifest-valid /type",
            "manifest-valid /ingredients",
            "title-set /identification/name",
            "language-set /languages/0/tag",
        ]
    );
}

#[test]
fn a_writer_manifest_reads_into_the_same_shape() {
    let w = read(
        "manifest.json",
        &fixture("fixtures/bttwriter/kpass_tmf_3jn_text_reg.json"),
    );
    let m = w.manifest.as_ref().unwrap();
    assert_eq!(m.format, Format::BttWriter);
    assert_eq!(m.language.tag, "tmf");
    assert_eq!(m.abbreviation["tmf"], "reg");
    assert_eq!(m.kind.subject.as_deref(), Some("Bible"));
    assert_eq!(books(&w), "3JN");
    assert_eq!(sources(&w), ["en/ulb 12"]);
    assert_eq!(findings(&w), ["book-title-translated /project/name"]);
    let Some(bento::Detail::BttWriter(doc)) = &w.detail else {
        panic!()
    };
    assert!(doc.finished_chunks.is_some());
}

// Why: the hub reads every WACS repo; real files bend every shape (from the
// hub's own leniency test): a BOM, a leading `---`, a second document, one
// source that's only a name, numbers for text, text for a sort, an empty list.
#[test]
fn an_rc_that_bends_every_shape_reads_and_says_how() {
    let yaml = b"\xEF\xBB\xBF---
dublin_core:
  identifier: ulb
  title: Unlocked
  format: text/usfm
  language: { identifier: en, title: English, direction: ltr }
  source:
    - { identifier: ulb, language: en, version: 12 }
    - plain-string-source
  version: 3
  relation: en/tn
projects:
  - { identifier: GEN, path: ./01-GEN.usfm, sort: \"01\", categories: }
  - { identifier: frt, path: ./A0-FRT.usfm, sort: 0 }
  - { identifier: intro, path: ./intro.md, sort: 3 }
---
junk: true
";
    let r = read("manifest.yaml", yaml);
    let m = r.manifest.as_ref().unwrap();
    assert_eq!(books(&r), "GEN FRT", "intro isn't a book");
    assert_eq!(m.books[0].sort, Some(1));
    assert_eq!(m.version.as_deref(), Some("3"));
    assert_eq!(
        sources(&r),
        ["en/ulb 12"],
        "a source with no language isn't related"
    );
    let Some(bento::Detail::Rc(rc)) = &r.detail else {
        panic!()
    };
    assert_eq!(rc.dublin_core.relation, ["en/tn"]);
    assert_eq!(
        rc.dublin_core.source[1].identifier.as_deref(),
        Some("plain-string-source")
    );
    assert_eq!(rc.projects.len(), 3);
    assert_eq!(
        findings(&r),
        [
            "lenient-value -",
            "lenient-value /dublin_core/source/0/version",
            "lenient-value /dublin_core/source/1",
            "lenient-value /dublin_core/relation",
            "lenient-value /dublin_core/version",
            "lenient-value /projects/0/sort",
        ]
    );
    assert!(
        r.findings[0].message.contains("2 YAML documents"),
        "{}",
        r.findings[0].message
    );
}

// Why: a real Burrito on WACS has `"type": "Buffer"` (a serializer's bug at
// its source). The rest of the file must still read.
#[test]
fn a_burrito_with_a_corrupt_type_still_reads() {
    let json = br#"{"format": "scripture burrito", "type": "Buffer",
        "languages": [{"tag": "fr", "name": {"fr": "Francais"}}],
        "ingredients": {"MAT.usfm": {"mimeType": "text/x-usfm", "size": 4, "scope": {"MAT": []}}},
        "localizedNames": {"MAT": {"short": {"fr": "Matthieu"}}},
        "relationships": [{"type": "source", "target": "some-opaque-id"}]}"#;
    let r = read("metadata.json", json);
    let m = r.manifest.as_ref().unwrap();
    assert_eq!(m.kind.flavor, None);
    assert_eq!(books(&r), "MAT");
    assert_eq!(m.books[0].title["fr"], "Matthieu");
    assert_eq!(m.sources, [], "an opaque target isn't a language/resource");
    assert!(
        findings(&r).contains(&"manifest-valid /type".to_string()),
        "{:?}",
        findings(&r)
    );
}

#[test]
fn what_cant_be_read_is_one_error_and_no_manifest() {
    for (name, bytes) in [
        ("manifest.yaml", &b"a: [1, 2\n"[..]),
        ("metadata.json", b"{\"format\": "),
        ("manifest.json", b"[1, 2]"),
        ("manifest.yaml", b"- just\n- a list\n"),
        ("manifest.yaml", b"\xFF\xFE"),
        ("README.md", b"{}"),
    ] {
        let r = read(name, bytes);
        assert_eq!(r.manifest, None, "{name}");
        assert_eq!(findings(&r), ["manifest-unreadable -"], "{name}");
        assert!(r.has_errors());
    }
}

#[test]
fn find_ignores_lookalikes_and_prefers_burrito_then_rc() {
    let names = [
        "README.md",
        "manifest (2).json",
        "org-manifest.json",
        "manifest.json",
    ];
    assert_eq!(find(&names), Some(("manifest.json", Format::BttWriter)));
    assert_eq!(
        find(&["manifest.json", "Manifest.YAML"]),
        Some(("Manifest.YAML", Format::Rc))
    );
    assert_eq!(
        find(&["manifest.yaml", "metadata.json"]),
        Some(("metadata.json", Format::Burrito))
    );
    assert_eq!(find(&["LICENSE.md"]), None);
    assert_eq!(
        bento::ManifestFinding::missing().check,
        Check::ManifestMissing
    );
}

#[test]
fn the_repos_name_should_start_with_its_language() {
    let tn = read(
        "manifest.yaml",
        &fixture("fixtures/rc/hub_tn_luk_text_reg.yaml"),
    );
    let named = |name| {
        check(
            &tn,
            &Repo {
                name: Some(name),
                files: None,
            },
        )
    };
    assert_eq!(named("alfred/tn_luk_text_reg"), []);
    assert_eq!(named("TN_reg"), [], "any case");
    let wrong = named("Tech_Advance/tpi_reg");
    assert_eq!(wrong.len(), 1);
    assert_eq!(wrong[0].check, Check::RepoNameLanguage);
    assert_eq!(
        wrong[0].message,
        "The repo is named tpi_reg, but its manifest's language is tn."
    );
    assert_eq!(check(&tn, &Repo::default()), []);
}

#[test]
fn every_check_has_a_distinct_id() {
    let mut ids: Vec<&str> = Check::ALL.iter().map(|c| c.id()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), Check::ALL.len());
}
