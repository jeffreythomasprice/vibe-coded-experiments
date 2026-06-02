//! Shared image helpers.

use image::{GrayImage, Luma, RgbImage};

/// Estimate the modal-ish background luminance of an image by taking a high
/// percentile of the grayscale histogram (the cream card background is the
/// brightest large region).
pub fn background_luma(gray: &GrayImage) -> u8 {
    let mut hist = [0u32; 256];
    for p in gray.pixels() {
        hist[p[0] as usize] += 1;
    }
    let total: u32 = hist.iter().sum();
    // Take the 60th percentile as a robust "background" estimate.
    let target = (total as f32 * 0.60) as u32;
    let mut acc = 0u32;
    for (v, &c) in hist.iter().enumerate() {
        acc += c;
        if acc >= target {
            return v as u8;
        }
    }
    200
}

/// Crop a padded sub-rectangle from an RGB image, clamped to bounds.
pub fn crop_padded(img: &RgbImage, x: u32, y: u32, w: u32, h: u32, pad: u32) -> RgbImage {
    let (iw, ih) = img.dimensions();
    let x0 = x.saturating_sub(pad);
    let y0 = y.saturating_sub(pad);
    let x1 = (x + w + pad).min(iw);
    let y1 = (y + h + pad).min(ih);
    image::imageops::crop_imm(img, x0, y0, x1 - x0, y1 - y0).to_image()
}

/// Convert an RGB image to grayscale (luma).
pub fn to_gray(img: &RgbImage) -> GrayImage {
    image::imageops::grayscale(img)
}

/// Build a binary image (`Luma<u8>`: 255 = ink/foreground) from grayscale.
pub fn binarize(gray: &GrayImage, threshold: u8) -> GrayImage {
    let mut out = GrayImage::new(gray.width(), gray.height());
    for (x, y, p) in gray.enumerate_pixels() {
        let v = if p[0] < threshold { 255 } else { 0 };
        out.put_pixel(x, y, Luma([v]));
    }
    out
}
