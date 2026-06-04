//! One-off illustration cropper for cards whose center art the layout pass
//! couldn't auto-isolate (sparse/scattered art, or art fused with body text).
//! A manual variant of the `find_illustration` step in `src/layout.rs`.
//!
//! Usage:
//!   cargo run --release --example crop -- <in.png> <out.png> <x> <y> <w> <h>
//!   cargo run --release --example crop -- <in.png> --profile   # dump ink profiles

use image::GenericImageView;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let img = image::open(&args[0]).expect("open input").to_rgb8();
    let (w, h) = img.dimensions();

    if args.get(1).map(String::as_str) == Some("--profile") {
        // Background luma estimate: most common (rounded) luma.
        let mut hist = [0u32; 256];
        for p in img.pixels() {
            let l = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as usize;
            hist[l.min(255)] += 1;
        }
        let bg = (0..256).max_by_key(|&i| hist[i]).unwrap() as i32;
        let dark = (bg - 35).max(0) as f32;
        let luma = |x: u32, y: u32| {
            let p = img.get_pixel(x, y);
            0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
        };
        println!("dims {w}x{h}  bg_luma={bg}  dark<{dark}");
        // Row ink fraction every 2% of height.
        println!("-- rows (y%, ink_frac) --");
        for i in 0..=50 {
            let y = (h - 1) * i / 50;
            let mut c = 0u32;
            for x in 0..w {
                if luma(x, y) < dark {
                    c += 1;
                }
            }
            println!("{:3}%  y={:4}  {:.3}", i * 2, y, c as f32 / w as f32);
        }
        println!("-- cols (x%, ink_frac) --");
        for i in 0..=50 {
            let x = (w - 1) * i / 50;
            let mut c = 0u32;
            for y in 0..h {
                if luma(x, y) < dark {
                    c += 1;
                }
            }
            println!("{:3}%  x={:4}  {:.3}", i * 2, x, c as f32 / h as f32);
        }
        return;
    }

    let x: u32 = args[2].parse().unwrap();
    let y: u32 = args[3].parse().unwrap();
    let cw: u32 = args[4].parse().unwrap();
    let ch: u32 = args[5].parse().unwrap();
    let sub = image::imageops::crop_imm(&img, x, y, cw, ch).to_image();
    sub.save(&args[1]).expect("save output");
    println!("wrote {} ({}x{}) from {}+{},{}", args[1], cw, ch, args[0], x, y);
}
