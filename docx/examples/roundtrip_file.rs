//! The round trip by hand, for trying a file in Word or LibreOffice:
//!
//! ```text
//! export <in.usfm> <out.docx>      write the .docx, list what it cannot carry
//! import <in.docx> [--book TIT]   print the USFM, report on stderr
//! check <in.usfm> <edited.docx>    is the edited file still the same book, as USJ?
//! ```
#[path = "../tests/common/mod.rs"]
#[allow(dead_code)]
mod common;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("export") => {
            let usfm = std::fs::read_to_string(&args[2]).unwrap();
            let out = docx::export(&usfm, &docx::ExportOptions::default());
            for u in &out.unsupported {
                eprintln!("unsupported at {}: {}", u.at, u.what);
            }
            std::fs::write(&args[3], &out.docx).unwrap();
        }
        Some("import") => {
            let mut settings = docx::Settings::default();
            if let Some(i) = args.iter().position(|a| a == "--book") {
                settings.book = args.get(i + 1).cloned();
            }
            let back = docx::import(&std::fs::read(&args[2]).unwrap(), &settings).unwrap();
            eprintln!("{:?}", back.report);
            for b in &back.books {
                eprintln!("book {:?} source {:?}", b.code, b.source);
                print!("{}", b.usfm);
            }
        }
        Some("check") => {
            let usfm = std::fs::read_to_string(&args[2]).unwrap();
            let back = docx::import(&std::fs::read(&args[3]).unwrap(), &docx::Settings::default()).unwrap();
            eprintln!("{:?}", back.report);
            match common::compare(&usfm, &back.books[0].usfm) {
                None => println!("same book"),
                Some(d) => println!("differs at {d}"),
            }
        }
        _ => eprintln!("export <in.usfm> <out.docx> | import <in.docx> | check <in.usfm> <in.docx>"),
    }
}

