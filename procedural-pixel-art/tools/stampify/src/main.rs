//! Turns a generated "pixel art" sheet (large, on magenta, off-grid) into rough
//! text stamps. Expect to touch the results up by hand.
//!
//! usage: stampify <image> <out-dir> <part> <material> <width> <view,view,...>
//!
//! `width` is the stamp width of the first view; the others use the same scale.
//! A material of `face` keeps only the dark and white pixels.

use image::RgbImage;
use std::collections::HashMap;
use std::error::Error;
use std::ops::Range;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path, out_dir, part, material, width, views] = args.as_slice() else {
        return Err(
            "usage: stampify <image> <out-dir> <part> <material> <width> <view,view,...>".into(),
        );
    };
    let width: u32 = width.parse()?;
    if width == 0 {
        return Err("width must be positive".into());
    }
    let views: Vec<&str> = views.split(',').collect();
    let image = image::open(path)?.to_rgb8();
    let boxes = find_views(&image, views.len())?;
    let scale = (boxes[0].0.len() as f64) / f64::from(width);
    let face = material == "face";
    let header_material = if face { "skin" } else { material };

    std::fs::create_dir_all(out_dir)?;
    for (view, (xs, ys)) in views.iter().zip(boxes) {
        let w = ((xs.len() as f64 / scale).round() as u32).max(1);
        let h = ((ys.len() as f64 / scale).round() as u32).max(1);
        let cells = downscale(&image, &xs, &ys, w, h);
        let rows = classify(&cells, w as usize, face);
        let out = format!("{out_dir}/{part}_{view}.txt");
        let text = format!(
            "material={header_material} anchor={},{}\n{}\n",
            w / 2,
            h / 2,
            rows.join("\n")
        );
        std::fs::write(&out, text)?;
        println!("{out}: {w}x{h}");
    }
    Ok(())
}

/// The generator only approximates #FF00FF, and blends it into edges.
fn is_background([r, g, b]: [u8; 3]) -> bool {
    r > 150 && b > 150 && g < 110 && r.abs_diff(b) < 90
}

fn opaque(image: &RgbImage, x: u32, y: u32) -> bool {
    !is_background(image.get_pixel(x, y).0)
}

/// The column and row ranges of one item on the sheet.
type Bounds = (Range<u32>, Range<u32>);

/// The `count` largest horizontally separated blobs, left to right.
fn find_views(image: &RgbImage, count: usize) -> Result<Vec<Bounds>, String> {
    // A few stray pixels in a column are noise, not content.
    let min_pixels = (image.height() / 200).max(2) as usize;
    let mut runs: Vec<Range<u32>> = Vec::new();
    let mut start = None;
    for x in 0..=image.width() {
        let filled = x < image.width()
            && (0..image.height()).filter(|&y| opaque(image, x, y)).count() >= min_pixels;
        match (filled, start) {
            (true, None) => start = Some(x),
            (false, Some(s)) => {
                runs.push(s..x);
                start = None;
            }
            _ => {}
        }
    }
    if runs.len() < count {
        return Err(format!("found {} items, expected {count}", runs.len()));
    }
    runs.sort_by_key(|r| std::cmp::Reverse(r.len()));
    runs.truncate(count);
    runs.sort_by_key(|r| r.start);

    runs.into_iter()
        .map(|xs| {
            let row_filled = |y: &u32| xs.clone().filter(|&x| opaque(image, x, *y)).count() >= 2;
            let top = (0..image.height()).find(row_filled);
            let bottom = (0..image.height()).rev().find(row_filled);
            match (top, bottom) {
                (Some(t), Some(b)) => Ok((xs, t..b + 1)),
                _ => Err("empty view".to_string()),
            }
        })
        .collect()
}

/// Each cell takes the most common colour under it, or nothing if mostly background.
fn downscale(
    image: &RgbImage,
    xs: &Range<u32>,
    ys: &Range<u32>,
    w: u32,
    h: u32,
) -> Vec<Option<[u8; 3]>> {
    let edge = |range: &Range<u32>, i: u32, n: u32| {
        range.start + (u64::from(i) * range.len() as u64 / u64::from(n)) as u32
    };
    let mut cells = Vec::new();
    for cy in 0..h {
        for cx in 0..w {
            let mut total = 0;
            let mut filled = 0;
            // Colour buckets: count and channel sums.
            let mut buckets: HashMap<[u8; 3], (u32, [u32; 3])> = HashMap::new();
            for y in edge(ys, cy, h)..edge(ys, cy + 1, h) {
                for x in edge(xs, cx, w)..edge(xs, cx + 1, w) {
                    total += 1;
                    if !opaque(image, x, y) {
                        continue;
                    }
                    filled += 1;
                    let c = image.get_pixel(x, y).0;
                    let bucket = buckets.entry(c.map(|v| v >> 4)).or_default();
                    bucket.0 += 1;
                    for (sum, v) in bucket.1.iter_mut().zip(c) {
                        *sum += u32::from(v);
                    }
                }
            }
            let mode = buckets
                .into_iter()
                // The key breaks ties so output does not depend on hash order.
                .max_by_key(|&(key, (n, _))| (n, key))
                .filter(|_| filled * 2 >= total)
                .map(|(_, (n, sums))| sums.map(|s| (s / n) as u8));
            cells.push(mode);
        }
    }
    cells
}

fn luminance([r, g, b]: [u8; 3]) -> u32 {
    (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000
}

const DARK: u32 = 50;
const FACE_DARK: u32 = 100;
const WHITE: u32 = 220;

fn classify(cells: &[Option<[u8; 3]>], width: usize, face: bool) -> Vec<String> {
    // Shade bands span the stamp's own brightness range.
    let mid = cells
        .iter()
        .flatten()
        .map(|&c| luminance(c))
        .filter(|&l| (DARK..WHITE).contains(&l));
    let low = mid.clone().min().unwrap_or(DARK);
    let high = mid.max().unwrap_or(WHITE);
    let span = (high - low + 1).max(1);

    cells
        .chunks(width)
        .map(|row| {
            row.iter()
                .map(|cell| {
                    let Some(color) = *cell else { return '.' };
                    let l = luminance(color);
                    match () {
                        _ if face && l < FACE_DARK => 'k',
                        _ if l >= WHITE => 'w',
                        _ if face => '.',
                        _ if l < DARK => '#',
                        _ => char::from(b'1' + ((l - low) * 4 / span) as u8),
                    }
                })
                .collect()
        })
        .collect()
}
