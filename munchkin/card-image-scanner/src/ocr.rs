//! OCR by shelling out to the `tesseract` CLI and parsing TSV output.

use crate::config::OcrConfig;
use crate::model::{BBox, RawLine};
use anyhow::{Context, Result};
use image::RgbImage;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Fallback title OCR: re-OCR just the top strip of a card to recover a
/// stylized title that the main pass missed or scored at confidence 0. Returns
/// the tallest text line found (the title), ignoring confidence since the strip
/// is isolated from illustration noise. Used only when the main pass yields no
/// title.
pub fn ocr_title_strip(crop: &RgbImage, cfg: &OcrConfig) -> Result<Option<String>> {
    let (w, h) = crop.dimensions();
    let strip_h = (h as f32 * 0.22) as u32;
    if strip_h == 0 {
        return Ok(None);
    }
    let strip = image::imageops::crop_imm(crop, 0, 0, w, strip_h).to_image();
    // psm 11 (sparse) with no confidence floor so a conf-0 title survives.
    let lines = run(&strip, cfg, 11, -1.0)?;
    let best = lines
        .into_iter()
        .filter(|l| l.text.chars().filter(|c| c.is_alphanumeric()).count() >= 2)
        .max_by_key(|l| l.glyph_height)
        .map(|l| l.text);
    Ok(best)
}

/// OCR a card crop, returning text lines (words grouped by Tesseract line).
pub fn ocr_card(crop: &RgbImage, cfg: &OcrConfig) -> Result<Vec<RawLine>> {
    run(crop, cfg, cfg.psm, cfg.min_conf)
}

/// Run tesseract on an image with explicit psm and confidence floor.
fn run(crop: &RgbImage, cfg: &OcrConfig, psm: u32, min_conf: f32) -> Result<Vec<RawLine>> {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let mut png = std::env::temp_dir();
    png.push(format!("cardscan_{pid}_{n}.png"));
    let out_base = std::env::temp_dir().join(format!("cardscan_{pid}_{n}_out"));

    crop.save(&png).with_context(|| format!("writing temp {png:?}"))?;

    let status = Command::new(&cfg.bin)
        .arg(&png)
        .arg(&out_base)
        .args(["--psm", &psm.to_string()])
        .args(["-l", &cfg.lang])
        .arg("tsv")
        .output()
        .with_context(|| format!("running tesseract ({})", cfg.bin))?;

    let tsv_path = out_base.with_extension("tsv");
    if !status.status.success() {
        let _ = std::fs::remove_file(&png);
        anyhow::bail!(
            "tesseract failed: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }

    let tsv = std::fs::read_to_string(&tsv_path)
        .with_context(|| format!("reading tesseract output {tsv_path:?}"))?;

    // Best-effort cleanup of temp files.
    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&tsv_path);

    Ok(parse_tsv(&tsv, min_conf))
}

/// One word parsed from the TSV.
struct Word {
    block: u32,
    par: u32,
    line: u32,
    word: u32,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    conf: f32,
    text: String,
}

fn parse_tsv(tsv: &str, min_conf: f32) -> Vec<RawLine> {
    let mut words: Vec<Word> = Vec::new();
    for (i, row) in tsv.lines().enumerate() {
        if i == 0 {
            continue; // header
        }
        let f: Vec<&str> = row.split('\t').collect();
        if f.len() < 12 {
            continue;
        }
        // level == 5 marks a word-level row.
        if f[0] != "5" {
            continue;
        }
        let text = f[11].trim().to_string();
        if text.is_empty() {
            continue;
        }
        let conf: f32 = f[10].parse().unwrap_or(-1.0);
        if conf < min_conf {
            continue;
        }
        words.push(Word {
            block: f[2].parse().unwrap_or(0),
            par: f[3].parse().unwrap_or(0),
            line: f[4].parse().unwrap_or(0),
            word: f[5].parse().unwrap_or(0),
            left: f[6].parse().unwrap_or(0),
            top: f[7].parse().unwrap_or(0),
            width: f[8].parse().unwrap_or(0),
            height: f[9].parse().unwrap_or(0),
            conf,
            text,
        });
    }

    // Group words by (block, par, line).
    let mut groups: Vec<(u32, u32, u32)> = words
        .iter()
        .map(|w| (w.block, w.par, w.line))
        .collect();
    groups.sort_unstable();
    groups.dedup();

    let mut lines = Vec::new();
    for key in groups {
        let mut ws: Vec<&Word> = words
            .iter()
            .filter(|w| (w.block, w.par, w.line) == key)
            .collect();
        ws.sort_by_key(|w| w.word);

        let text = ws
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let x0 = ws.iter().map(|w| w.left).min().unwrap();
        let y0 = ws.iter().map(|w| w.top).min().unwrap();
        let x1 = ws.iter().map(|w| w.left + w.width).max().unwrap();
        let y1 = ws.iter().map(|w| w.top + w.height).max().unwrap();

        let mut heights: Vec<u32> = ws.iter().map(|w| w.height).collect();
        heights.sort_unstable();
        let glyph_height = heights[heights.len() / 2];

        let confidence = ws.iter().map(|w| w.conf).sum::<f32>() / ws.len() as f32;

        lines.push(RawLine {
            text,
            bbox: BBox {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            },
            glyph_height,
            confidence,
            block: key.0,
            par: key.1,
            line: key.2,
        });
    }

    // Order lines top-to-bottom for readability.
    lines.sort_by_key(|l| l.bbox.y);
    lines
}
