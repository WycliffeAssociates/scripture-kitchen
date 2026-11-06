//! A study Bible page with ONE notes band per page: full width, in columns,
//! under the text. Typst cannot decide on its own which notes belong on a
//! page while it lays the page out (the band's height moves the page break,
//! the page break decides the notes), so this drives it from outside, the way
//! PTXprint drives XeTeX through repeated runs:
//!
//! 1. lay out with no notes; read the page each verse landed on;
//! 2. give each page the notes of its verses, anchor that page's band (a
//!    page-wide bottom float) before the first paragraph that starts on it
//!    (a page-wide float only works from the top level of the flow, not from
//!    inside a paragraph), lay out again;
//! 3. read verse pages and each band's top and bottom back; a note whose verse
//!    moved on moves with it, and a band taller than `study_max` of the text
//!    area (or pushed off its page) sends its overflowing share of notes on;
//! 4. repeat until nothing moves. A note sits on its verse's current page
//!    plus its carry, the pages overflow has pushed it; only the carry is
//!    monotone. The best pass seen (most notes on their verse's page, no band
//!    past the page foot) is the one exported.

use std::collections::{BTreeMap, HashMap};

use typst::foundations::{Label, Selector, Value};
use typst::introspection::{Introspector, MetadataElem};
use typst::utils::PicoStr;

#[path = "../notes/recipe.rs"]
#[allow(dead_code)]
mod notes;

const MAX_PASSES: usize = 12;

struct Note {
    verse: String,
    markup: String,
}

/// Every `metadata("…")<label>` in the document with its page, in order.
fn marks(doc: &plate::world::Document, label: &str) -> Vec<(String, usize)> {
    marks_at(doc, label).into_iter().map(|(s, p, _)| (s, p)).collect()
}

/// [`marks`] with each mark's distance from the top of its page, in mm.
fn marks_at(doc: &plate::world::Document, label: &str) -> Vec<(String, usize, f64)> {
    let label = Label::new(PicoStr::intern(label)).unwrap();
    let intro = doc.introspector();
    intro
        .query(&Selector::Label(label))
        .iter()
        .filter_map(|c| {
            let Value::Str(s) = &c.to_packed::<MetadataElem>()?.value else { return None };
            let pos = intro.position(c.location()?)?;
            Some((s.to_string(), pos.page.get(), pos.point.y.to_mm()))
        })
        .collect()
}

/// Paragraph index → the page its start landed on.
fn para_marks(doc: &plate::world::Document) -> Vec<(usize, usize)> {
    marks(doc, "pn-para").into_iter().filter_map(|(i, p)| Some((i.parse().ok()?, p))).collect()
}

pub struct Report {
    pub passes: usize,
    pub converged: bool,
    pub notes: usize,
    pub on_own_page: usize,
    pub pass_ms: Vec<f64>,
}

pub fn run(
    fonts: &std::sync::Arc<plate::world::FontStore>,
    usfm: &str,
    tsv: &str,
    settings: &str,
) -> Result<(Vec<u8>, plate::world::Timings, Report), String> {
    // First note's first paragraph per verse: printed study-Bible density.
    let mut by_verse: HashMap<String, String> = HashMap::new();
    for line in tsv.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 7 {
            continue;
        }
        let Some((c, v)) = cols[0].split_once(':') else { continue };
        if c == "front" || v == "intro" {
            continue;
        }
        let key = format!("{c}:{}", v.split(['-', ',']).next().unwrap());
        by_verse.entry(key).or_insert_with(|| {
            notes::markdown(cols[6].replace("\\n", "\n").split("\n\n").next().unwrap_or(""))
        });
    }

    let template = plate::recipe::template();
    // The page's text area and how much of it a band may take.
    let cfg: serde_json::Value = serde_json::from_str(settings).unwrap_or_default();
    let num = |k: &str, d: f64| cfg.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
    let (margin_top, margin_bottom) = (num("margin_top", 14.0), num("margin_bottom", 14.0));
    let share = num("study_max", 0.5);
    // Every paragraph leaves a mark at its start; `band` adds a page's band.
    let compile = |band: &dyn Fn(usize) -> Option<String>| {
        let before = |_: &str, i: usize| -> Option<String> {
            Some(format!("#u.pmark({i});{}", band(i).unwrap_or_default()))
        };
        let hooks = plate::Hooks { before_paragraph: Some(&before), ..Default::default() };
        let book = plate::markup_with(usfm, &hooks);
        let doc = plate::recipe::doc("pagenotes", &[book], "", "", settings, 0.0);
        let t = web_time::Instant::now();
        let laid = plate::world::compile_document(fonts, &doc, &template)?;
        Ok::<_, String>((laid, doc, t.elapsed().as_secs_f64() * 1e3))
    };

    // Pass 0: where verses fall with no notes at all.
    let (mut laid, mut doc, ms0) = compile(&|_| None)?;
    let mut pass_ms = vec![ms0];
    let verse_pages = marks(&laid, "pn-verse");
    let notes: Vec<Note> = verse_pages
        .iter()
        .filter_map(|(v, _)| by_verse.get(v).map(|m| Note { verse: v.clone(), markup: m.clone() }))
        .collect();
    let mut page_of: HashMap<String, usize> = verse_pages.iter().cloned().collect();
    let mut para_pages = para_marks(&laid);
    let page_h = laid.pages()[0].frame.height().to_mm();
    let max_band = (page_h - margin_top - margin_bottom) * share;
    let mut carry: Vec<usize> = vec![0; notes.len()];
    let mut assign: Vec<usize> = notes.iter().map(|n| page_of[&n.verse]).collect();
    // (notes on their own page, overflowing bands, layout, doc) of the best pass.
    let mut best: Option<(usize, usize, plate::world::Document, plate::world::Doc)> = None;

    let mut converged = false;
    let mut passes = 0;
    while passes < MAX_PASSES {
        passes += 1;
        // Each page's band, anchored before the first paragraph starting on
        // that page (or the next page that has one).
        let mut bands: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, &p) in assign.iter().enumerate() {
            bands.entry(p).or_default().push(i);
        }
        let mut anchor: HashMap<usize, usize> = HashMap::new();
        for &p in bands.keys() {
            if let Some((i, _)) = para_pages.iter().find(|(_, pp)| *pp >= p) {
                anchor.entry(*i).or_insert(p);
            }
        }
        let band = |i: usize| -> Option<String> {
            let p = *anchor.get(&i)?;
            // Two columns, balanced here by length: columns() does not
            // balance inside a float, whose height is unbounded.
            let members = &bands[&p];
            let total: usize = members.iter().map(|&i| notes[i].markup.len()).sum();
            let (mut acc, mut split) = (0, members.len());
            for (k, &i) in members.iter().enumerate() {
                if acc * 2 >= total {
                    split = k;
                    break;
                }
                acc += notes[i].markup.len();
            }
            let col = |ids: &[usize]| -> String {
                ids.iter()
                    .map(|&i| format!("#u.pn(\"{}\")[{}];", plate::str_lit(&notes[i].verse), notes[i].markup))
                    .collect()
            };
            Some(format!("#u.band({p})[{}][{}];", col(&members[..split]), col(&members[split..])))
        };
        let (l, d, ms) = compile(&band)?;
        pass_ms.push(ms);
        laid = l;
        doc = d;
        para_pages = para_marks(&laid);
        page_of = marks(&laid, "pn-verse").into_iter().collect();
        // Band → (page it landed on, top, bottom).
        let ends: HashMap<String, (usize, f64)> =
            marks_at(&laid, "pn-band-end").into_iter().map(|(p, pg, y)| (p, (pg, y))).collect();
        let band_at: HashMap<usize, (usize, f64, f64)> = marks_at(&laid, "pn-band")
            .into_iter()
            .filter_map(|(p, pg, top)| {
                let (_, bottom) = *ends.get(&p)?;
                Some((p.parse().ok()?, (pg, top, bottom)))
            })
            .collect();

        // Score this pass before deciding the next one.
        let own = notes.iter().zip(&assign).filter(|(n, p)| page_of[&n.verse] == **p).count();
        let spilled = band_at
            .values()
            .filter(|(_, _, bottom)| *bottom > page_h - margin_bottom + 0.5)
            .count();
        let better = match &best {
            None => true,
            Some((o, sp, _, _)) => (spilled, std::cmp::Reverse(own)) < (*sp, std::cmp::Reverse(*o)),
        };
        if better {
            best = Some((own, spilled, laid.clone(), doc.clone()));
        }
        eprintln!("  pass {passes}: {own} notes on their page, {spilled} bands past the foot");

        for (&p, members) in &bands {
            let Some(&(landed, top, bottom)) = band_at.get(&p) else { continue };
            let height = bottom - top;
            let overflow = bottom > page_h - margin_bottom + 0.5;
            // Too tall, past the page foot, or pushed to a later page: the
            // share of notes over the limit moves on, at least one.
            let excess = if landed > p || overflow {
                members.len().div_ceil(3)
            } else if height > max_band {
                ((members.len() as f64) * (height - max_band) / height).ceil() as usize
            } else {
                0
            };
            for &i in members.iter().rev().take(excess.min(members.len())) {
                carry[i] += 1;
            }
        }
        let next: Vec<usize> =
            notes.iter().zip(&carry).map(|(n, c)| page_of[&n.verse] + c).collect();
        if next == assign {
            converged = true;
            break;
        }
        assign = next;
    }

    let (on_own_page, _, laid, doc) = best.unwrap_or((0, 0, laid, doc));
    let layout_ms: f64 = pass_ms.iter().sum();
    let (pdf, timings) = plate::world::export(&laid, &doc, layout_ms)?;
    Ok((pdf, timings, Report { passes, converged, notes: notes.len(), on_own_page, pass_ms }))
}
