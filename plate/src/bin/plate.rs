//! `plate [--settings s.json] [--out out.pdf] [--markup dir] [--runs N] BOOK.usfm…`
//!
//! Compiles the books into one PDF and prints where the time went. `--runs`
//! repeats the compile in-process, so later runs show Typst's warm cache.

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut settings, mut out, mut markup_dir, mut runs) =
        ("{}".to_string(), PathBuf::from("plate.pdf"), None::<PathBuf>, 1usize);
    let mut split = false;
    let mut books = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--settings" => settings = std::fs::read_to_string(args.next().unwrap()).unwrap(),
            "--set" => settings = args.next().unwrap(),
            "--out" => out = args.next().unwrap().into(),
            "--markup" => markup_dir = Some(args.next().unwrap().into()),
            "--runs" => runs = args.next().unwrap().parse().unwrap(),
            "--split" => split = true,
            _ => books.push(PathBuf::from(a)),
        }
    }
    let t = std::time::Instant::now();
    let fonts = plate::world::fonts(&[PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/fonts"))]);
    eprintln!("fonts: {} faces in {:.0} ms", fonts.book().families().count(), t.elapsed().as_secs_f64() * 1e3);
    let usfm: Vec<String> = books.iter().map(|p| std::fs::read_to_string(p).unwrap()).collect();
    if let Some(dir) = &markup_dir {
        std::fs::create_dir_all(dir).unwrap();
        for (p, u) in books.iter().zip(&usfm) {
            let b = plate::markup(u);
            std::fs::write(dir.join(p.file_stem().unwrap()).with_extension("typ"), b.markup).unwrap();
        }
    }
    if split {
        // One Typst document per book, compiled in parallel on rayon's pool
        // (the same pool Typst's own layout uses). PDFs are written per book;
        // merging them is not done here.
        use rayon::prelude::*;
        for run in 1..=runs {
            let t = web_time::Instant::now();
            let results: Vec<_> = usfm
                .par_iter()
                .map(|u| plate::world::render(&fonts, std::slice::from_ref(u), &settings))
                .collect();
            let wall = t.elapsed().as_secs_f64() * 1e3;
            let (mut pages, mut layout, mut slowest) = (0, 0.0, 0.0f64);
            for (i, r) in results.into_iter().enumerate() {
                let (pdf, t) = r.unwrap_or_else(|e| panic!("{e}"));
                pages += t.pages;
                layout += t.layout_ms;
                slowest = slowest.max(t.layout_ms + t.pdf_ms);
                std::fs::write(out.with_extension(format!("{i:02}.pdf")), pdf).unwrap();
            }
            println!("split run {run}: {} books, {pages} pages · wall {wall:.0} ms · sum of layouts {layout:.0} ms · slowest book {slowest:.0} ms", usfm.len());
        }
        return;
    }
    for run in 1..=runs {
        match plate::world::render(&fonts, &usfm, &settings) {
            Ok((pdf, t)) => {
                println!(
                    "run {run}: {} pages · markup {:.1} ms ({} KB) · layout {:.0} ms · pdf {:.0} ms · total {:.0} ms · {} warnings",
                    t.pages, t.markup_ms, t.markup_bytes / 1024, t.layout_ms, t.pdf_ms,
                    t.markup_ms + t.layout_ms + t.pdf_ms, t.warnings
                );
                std::fs::write(&out, pdf).unwrap();
            }
            Err(e) => {
                eprintln!("error:\n{e}");
                std::process::exit(1);
            }
        }
    }
}
