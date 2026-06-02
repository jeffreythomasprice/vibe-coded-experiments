mod cardlist;
mod cli;
mod config;
mod debug;
mod imageops;
mod layout;
mod model;
mod ocr;
mod segment;

use anyhow::{Context, Result};
use cardlist::CardList;
use clap::Parser;
use cli::{Args, CardType};
use config::{LayoutConfig, OcrConfig, SegmentConfig};
use imageops::crop_padded;
use model::{Card, Database};
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let args = Args::parse();
    run(&args)
}

fn run(args: &Args) -> Result<()> {
    let seg_cfg = SegmentConfig::from_args(args);
    let ocr_cfg = OcrConfig::from_args(args);
    let layout_cfg = LayoutConfig::default();

    let inputs = expand_inputs(&args.inputs)?;
    if inputs.is_empty() {
        anyhow::bail!("no input images matched");
    }

    let out_dir = PathBuf::from(&args.out_dir);
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("creating out dir {out_dir:?}"))?;
    let db_path = args
        .db
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| out_dir.join("cards.toml"));
    let debug_dir = if args.debug {
        let d = args
            .debug_dir
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(|| out_dir.join("debug"));
        std::fs::create_dir_all(&d).ok();
        Some(d)
    } else {
        None
    };

    // Load the card list for title correction.
    let card_list = load_card_list(args, &inputs)?;
    if args.verbose {
        if let Some(cl) = &card_list {
            eprintln!("loaded {} known card names", cl.len());
        }
    }

    let mut db = Database {
        generated: timestamp(),
        sheets: 0,
        cards: 0,
        card_list: Vec::new(),
    };

    let mut matched = 0usize;
    let mut unmatched = 0usize;

    for input in &inputs {
        let fname = input
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let card_type = infer_type(args.r#type, &fname);

        let sheet = match image::open(input) {
            Ok(i) => i.to_rgb8(),
            Err(e) => {
                eprintln!("skip {input:?}: {e}");
                continue;
            }
        };

        let seg = segment::detect_cards(&sheet, &seg_cfg);
        db.sheets += 1;
        println!(
            "{fname}: {} card(s) detected ({}x{})",
            seg.boxes.len(),
            sheet.width(),
            sheet.height()
        );
        if args.verbose {
            eprintln!("  binarization threshold: {}", seg.threshold);
        }

        let stem = Path::new(&fname)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();

        if let Some(dd) = &debug_dir {
            let _ = seg.binarized.save(dd.join(format!("{stem}_binary.png")));
            let overlay = debug::sheet_overlay(&sheet, &seg.boxes);
            let _ = overlay.save(dd.join(format!("{stem}_boxes.png")));
        }

        for (idx, b) in seg.boxes.iter().enumerate() {
            let crop = crop_padded(&sheet, b.x, b.y, b.w, b.h, 0);

            let id = format!("{stem}_{idx:02}");
            let card_png = format!("{id}_card.png");
            let card_path = out_dir.join(&card_png);
            crop.save(&card_path)
                .with_context(|| format!("saving {card_path:?}"))?;

            let raw_lines = match ocr::ocr_card(&crop, &ocr_cfg) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("  OCR failed for {id}: {e}");
                    Vec::new()
                }
            };

            let mut analysis = layout::analyze(&crop, &raw_lines, &layout_cfg);

            // If the main pass missed the stylized title, retry on the top strip.
            if analysis.title.is_none() {
                if let Ok(Some(t)) = ocr::ocr_title_strip(&crop, &ocr_cfg) {
                    analysis.title = Some(t);
                }
            }

            // Save the illustration crop, if found.
            let illustration_path = if let Some(ib) = &analysis.illustration {
                let sub =
                    image::imageops::crop_imm(&crop, ib.x, ib.y, ib.w, ib.h).to_image();
                let illo_png = format!("{id}_illustration.png");
                let p = out_dir.join(&illo_png);
                if sub.save(&p).is_ok() {
                    Some(illo_png)
                } else {
                    None
                }
            } else {
                None
            };

            // Title correction against the card list.
            let (title, title_raw, title_match_score) = correct_title(
                analysis.title.clone(),
                card_list.as_ref(),
                args.title_match_threshold,
            );
            if let Some(score) = title_match_score {
                if score >= args.title_match_threshold {
                    matched += 1;
                } else {
                    unmatched += 1;
                }
            }

            if let Some(dd) = &debug_dir {
                let overlay = debug::card_overlay(&crop, &analysis, &raw_lines);
                let _ = overlay.save(dd.join(format!("{id}_regions.png")));
            }

            db.card_list.push(Card {
                id,
                source_image: fname.clone(),
                card_type: card_type.clone(),
                index_in_sheet: idx as u32,
                row: 0,
                col: idx as u32,
                bbox: *b,
                title,
                title_raw,
                title_match_score,
                top_extras: analysis.top_extras,
                body: analysis.body,
                bottom_left: analysis.bottom_left,
                bottom_right: analysis.bottom_right,
                card_image_path: card_png,
                illustration_path,
                raw_lines,
            });
        }
    }

    db.cards = db.card_list.len();
    let toml = toml::to_string_pretty(&db).context("serializing TOML")?;
    std::fs::write(&db_path, toml).with_context(|| format!("writing {db_path:?}"))?;

    println!(
        "\nWrote {} cards from {} sheet(s) to {}",
        db.cards,
        db.sheets,
        db_path.display()
    );
    println!("titles corrected: {matched}, low-confidence/kept-raw: {unmatched}");
    Ok(())
}

fn correct_title(
    raw: Option<String>,
    list: Option<&CardList>,
    threshold: f64,
) -> (Option<String>, Option<String>, Option<f64>) {
    let raw = match raw {
        Some(r) if !r.trim().is_empty() => r,
        _ => return (None, None, None),
    };
    let Some(list) = list else {
        return (Some(raw), None, None);
    };
    match list.best_match(&raw) {
        Some((canonical, score)) if score >= threshold => {
            (Some(canonical), Some(raw), Some(score))
        }
        Some((_, score)) => (Some(raw.clone()), Some(raw), Some(score)),
        None => (Some(raw), None, None),
    }
}

fn infer_type(t: CardType, fname: &str) -> String {
    match t {
        CardType::Door => "door".into(),
        CardType::Loot => "loot".into(),
        CardType::Auto => {
            let lower = fname.to_lowercase();
            if lower.starts_with("door") {
                "door".into()
            } else if lower.starts_with("loot") {
                "loot".into()
            } else {
                "unknown".into()
            }
        }
    }
}

fn load_card_list(args: &Args, inputs: &[PathBuf]) -> Result<Option<CardList>> {
    let path = if let Some(p) = &args.card_list {
        Some(PathBuf::from(p))
    } else {
        inputs
            .first()
            .and_then(|i| i.parent())
            .map(|d| d.join("Munchkin-CardList.txt"))
            .filter(|p| p.exists())
    };
    match path {
        Some(p) if p.exists() => Ok(Some(CardList::load(&p)?)),
        Some(p) => {
            eprintln!("card list not found at {p:?}; titles will not be corrected");
            Ok(None)
        }
        None => Ok(None),
    }
}

fn expand_inputs(patterns: &[String]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for pat in patterns {
        let p = Path::new(pat);
        if p.exists() {
            out.push(p.to_path_buf());
            continue;
        }
        let mut any = false;
        for entry in glob::glob(pat).with_context(|| format!("bad glob {pat}"))? {
            if let Ok(path) = entry {
                out.push(path);
                any = true;
            }
        }
        if !any {
            eprintln!("warning: no match for {pat}");
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// A coarse timestamp without pulling in a date crate: seconds since the epoch.
fn timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("epoch:{secs}")
}
