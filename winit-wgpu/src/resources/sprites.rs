use std::collections::HashMap;

use anyhow::Result;
use glam::Vec2;
use image::{DynamicImage, RgbaImage};

use super::texture_atlas::TextureAtlas;

fn parse_sprite(src: &str) -> Result<DynamicImage> {
    let (palette_section, grid_section) = src
        .split_once("\n\n")
        .ok_or_else(|| anyhow::anyhow!("sprite file missing blank-line separator between palette and grid"))?;

    let palette: HashMap<char, [u8; 4]> = palette_section
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let idx = parts.next()?.chars().next()?;
            let hex = parts.next()?;
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((idx, [r, g, b, 255]))
        })
        .collect();

    let rows: Vec<&str> = grid_section.lines().filter(|l| !l.is_empty()).collect();
    let height = rows.len() as u32;
    let width = rows.first().map(|r| r.len()).unwrap_or(0) as u32;

    let mut img = RgbaImage::new(width, height);
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let pixel = if ch == '0' {
                [0u8, 0, 0, 0]
            } else {
                *palette.get(&ch).unwrap_or(&[255, 255, 255, 255])
            };
            img.put_pixel(x as u32, y as u32, image::Rgba(pixel));
        }
    }

    Ok(DynamicImage::ImageRgba8(img))
}

fn extract_hull_points(image: &DynamicImage) -> Vec<Vec2> {
    let img = image.to_rgba8();
    let (w, h) = (img.width() as f32, img.height() as f32);
    let mut points = Vec::new();
    for y in 0..img.height() {
        for x in 0..img.width() {
            if img.get_pixel(x, y)[3] > 0 {
                let (fx, fy) = (x as f32 - w * 0.5, y as f32 - h * 0.5);
                points.extend_from_slice(&[
                    Vec2::new(fx, fy),
                    Vec2::new(fx + 1.0, fy),
                    Vec2::new(fx, fy + 1.0),
                    Vec2::new(fx + 1.0, fy + 1.0),
                ]);
            }
        }
    }
    points
}

pub fn load_atlas(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(TextureAtlas, HashMap<String, Vec<Vec2>>)> {
    let mut map: HashMap<String, DynamicImage> = HashMap::new();

    macro_rules! add {
        ($name:expr) => {
            map.insert(
                $name.to_string(),
                parse_sprite(include_str!(concat!(
                    "../../resources/sprites/",
                    $name,
                    ".sprite"
                )))?,
            );
        };
    }

    add!("squid_f1");
    add!("squid_f2");
    add!("crab_f1");
    add!("crab_f2");
    add!("octopus_f1");
    add!("octopus_f2");
    add!("ufo");
    add!("boss");
    add!("ship1");
    add!("ship2");
    add!("ship3");
    add!("laser_bolt");
    add!("enemy_bomb");
    add!("plasma_bolt");
    add!("explosion_f1");
    add!("explosion_f2");
    add!("bunker");
    add!("shield");
    add!("star");
    add!("heart");

    let hulls: HashMap<String, Vec<Vec2>> = map
        .iter()
        .map(|(name, img)| (name.clone(), extract_hull_points(img)))
        .collect();

    Ok((TextureAtlas::new(device, queue, map), hulls))
}
