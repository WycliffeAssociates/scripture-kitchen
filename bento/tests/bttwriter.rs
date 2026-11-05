//! Five real BTT Writer `manifest.json` files, read: GOLDEN.
//!
//! ```text
//! fixtures/bttwriter/<owner>_<repo>.json   the file at HEAD of the projects bttwriter's goldens stitch
//! ```

use std::path::Path;

use bento::bttwriter::read;

struct Case {
    name: &'static str,
    book: &'static str,
    language: &'static str,
    resource: &'static str,
    /// The first source: language, resource, version (`None`: not checked).
    source: (&'static str, &'static str, Option<&'static str>),
}

const CASES: &[Case] = &[
    Case {
        name: "kpass_tmf_3jn_text_reg",
        book: "3JN",
        language: "tmf",
        resource: "reg",
        source: ("en", "ulb", Some("12")),
    },
    Case {
        name: "bahasatech.indotimur_kps-x-kaigar_3jn_text_reg",
        book: "3JN",
        language: "kps-x-kaigar",
        resource: "reg",
        source: ("id", "ayt", None),
    },
    Case {
        name: "chunga_major_bem-x-ngumbo_2ch_text_reg",
        book: "2CH",
        language: "bem-x-ngumbo",
        resource: "reg",
        source: ("en", "ulb", Some("12")),
    },
    Case {
        name: "andriatiana_wa_msh_job_text_udb",
        book: "JOB",
        language: "msh",
        resource: "udb",
        source: ("plt", "ulb", None),
    },
    Case {
        name: "bahasatech.indotimur_saw_isa_text_reg",
        book: "ISA",
        language: "saw",
        resource: "reg",
        source: ("id", "ayt", Some("1.3")),
    },
];

#[test]
fn five_writer_manifests_name_their_project() {
    for case in CASES {
        let name = case.name;
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/bttwriter")
            .join(format!("{name}.json"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let manifest = read(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let project = manifest
            .project(Some(name))
            .unwrap_or_else(|| panic!("{name}: not a project"));
        assert_eq!(project.book.to_string(), case.book, "{name}");
        assert_eq!(project.language.id, case.language, "{name}");
        assert_eq!(project.resource, case.resource, "{name}");
        let first = &project.sources[0];
        let (language, resource, version) = case.source;
        assert_eq!(first.language, language, "{name}");
        assert_eq!(first.resource, resource, "{name}");
        if let Some(version) = version {
            assert_eq!(first.version.as_deref(), Some(version), "{name}");
        }
    }
}
