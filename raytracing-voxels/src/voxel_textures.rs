use anyhow::Result;
use noise::{NoiseFn, Perlin};

use crate::overlay::{Rgba, Texture};
use crate::texture_atlas::TextureAtlasBuilder;

const TILE_SIZE: u32 = 64;

pub struct VoxelTextureAtlas {
    texture: Texture,
    uv_map: [[f32; 4]; 256],
}

impl VoxelTextureAtlas {
    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    pub fn uv_map(&self) -> &[[f32; 4]; 256] {
        &self.uv_map
    }
}

struct TileDef {
    id: u8,
    name: &'static str,
    generator: fn(u32) -> Texture,
}

#[cfg(test)]
fn sample_fbm(
    x: f64,
    y: f64,
    octaves: u32,
    frequency: f64,
    lacunarity: f64,
    persistence: f64,
    seed: u32,
) -> f64 {
    let perlin = Perlin::new(seed);
    let mut value = 0.0;
    let mut amplitude = 1.0;
    let mut freq = frequency;
    for _ in 0..octaves {
        value += amplitude * perlin.get([x * freq, y * freq]);
        freq *= lacunarity;
        amplitude *= persistence;
    }
    value
}

fn lerp_color(a: Rgba, b: Rgba, t: f64) -> Rgba {
    let t = t.clamp(0.0, 1.0) as f32;
    let inv = 1.0 - t;
    Rgba::rgb(
        (a.r as f32 * inv + b.r as f32 * t) as u8,
        (a.g as f32 * inv + b.g as f32 * t) as u8,
        (a.b as f32 * inv + b.b as f32 * t) as u8,
    )
}

fn pixel_hash(x: u32, y: u32, seed: u32) -> u32 {
    let mut h = x
        .wrapping_mul(374761393)
        .wrapping_add(y.wrapping_mul(668265263))
        .wrapping_add(seed.wrapping_mul(1274126177));
    h = (h ^ (h >> 13)).wrapping_mul(1103515245);
    h ^ (h >> 16)
}

fn posterize(t: f64, levels: u32) -> f64 {
    let max_index = (levels.max(1) - 1).max(1) as f64;
    ((t * levels as f64).floor() / max_index).clamp(0.0, 1.0)
}

fn palette_select(palette: &[Rgba], t: f64) -> Rgba {
    let len = palette.len();
    if len == 0 {
        return Rgba::rgb(0, 0, 0);
    }
    let index = ((t * len as f64).floor() as usize).min(len - 1);
    palette[index]
}

#[allow(clippy::too_many_arguments)]
fn sample_fbm_tiling(
    x: f64,
    y: f64,
    tile_size: f64,
    x_scale: f64,
    y_scale: f64,
    octaves: u32,
    frequency: f64,
    lacunarity: f64,
    persistence: f64,
    seed: u32,
) -> f64 {
    let perlin = Perlin::new(seed);
    let u = x / tile_size;
    let v = y / tile_size;
    let two_pi = std::f64::consts::TAU;
    let rx = x_scale * frequency;
    let ry = y_scale * frequency;

    let mut value = 0.0;
    let mut amplitude = 1.0;
    let mut freq = 1.0;
    for _ in 0..octaves {
        let cx = (two_pi * u).cos() * rx * freq;
        let sx = (two_pi * u).sin() * rx * freq;
        let cy = (two_pi * v).cos() * ry * freq;
        let sy = (two_pi * v).sin() * ry * freq;
        value += amplitude * perlin.get([cx, sx, cy, sy]);
        freq *= lacunarity;
        amplitude *= persistence;
    }
    value
}

struct NoiseLayerConfig {
    octaves: u32,
    frequency: f64,
    lacunarity: f64,
    persistence: f64,
    x_scale: f64,
    y_scale: f64,
    seed: u32,
}

fn noise_layer(x: u32, y: u32, tile_size: u32, config: &NoiseLayerConfig) -> f64 {
    let raw = sample_fbm_tiling(
        x as f64,
        y as f64,
        tile_size as f64,
        config.x_scale,
        config.y_scale,
        config.octaves,
        config.frequency,
        config.lacunarity,
        config.persistence,
        config.seed,
    );
    // Normalize from roughly [-1, 1] to [0, 1]
    ((raw + 1.0) / 2.0).clamp(0.0, 1.0)
}

fn bayer_dither_2x2(x: u32, y: u32, t: f64, levels: u32) -> u32 {
    const BAYER: [[f64; 2]; 2] = [[0.0 / 4.0, 2.0 / 4.0], [3.0 / 4.0, 1.0 / 4.0]];
    let threshold = BAYER[(y % 2) as usize][(x % 2) as usize];
    let scaled = t * (levels - 1) as f64;
    let low = scaled.floor() as u32;
    let frac = scaled - scaled.floor();
    if frac > threshold {
        (low + 1).min(levels - 1)
    } else {
        low.min(levels - 1)
    }
}

fn darken(color: Rgba, amount: u8) -> Rgba {
    Rgba::rgb(
        color.r.saturating_sub(amount),
        color.g.saturating_sub(amount),
        color.b.saturating_sub(amount),
    )
}

fn brighten(color: Rgba, amount: u8) -> Rgba {
    Rgba::rgb(
        color.r.saturating_add(amount),
        color.g.saturating_add(amount),
        color.b.saturating_add(amount),
    )
}

struct GrassParams {
    palette: Vec<Rgba>,
    cluster_frequency: f64,
    cluster_octaves: u32,
    detail_frequency: f64,
    detail_octaves: u32,
    cluster_weight: f64,
    detail_weight: f64,
    hash_jitter_range: f64,
    posterize_levels: u32,
    brightness_shift_range: i16,
}

impl Default for GrassParams {
    fn default() -> Self {
        Self {
            palette: vec![
                Rgba::rgb(30, 80, 20),
                Rgba::rgb(45, 110, 30),
                Rgba::rgb(55, 135, 35),
                Rgba::rgb(70, 160, 45),
                Rgba::rgb(85, 175, 50),
                Rgba::rgb(100, 190, 55),
            ],
            cluster_frequency: 1.0,
            cluster_octaves: 3,
            detail_frequency: 12.0,
            detail_octaves: 2,
            cluster_weight: 0.65,
            detail_weight: 0.25,
            hash_jitter_range: 0.075,
            posterize_levels: 6,
            brightness_shift_range: 5,
        }
    }
}

fn generate_grass_parameterized(seed: u32, params: &GrassParams) -> Texture {
    let mut tex = Texture::new(TILE_SIZE, TILE_SIZE);

    let cluster_config = NoiseLayerConfig {
        octaves: params.cluster_octaves,
        frequency: params.cluster_frequency,
        lacunarity: 2.0,
        persistence: 0.5,
        x_scale: 1.0,
        y_scale: 1.0,
        seed,
    };

    let detail_config = NoiseLayerConfig {
        octaves: params.detail_octaves,
        frequency: params.detail_frequency,
        lacunarity: 2.0,
        persistence: 0.5,
        x_scale: 1.0,
        y_scale: 1.0,
        seed: seed.wrapping_add(500),
    };

    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let cluster = noise_layer(x, y, TILE_SIZE, &cluster_config);
            let detail = noise_layer(x, y, TILE_SIZE, &detail_config);

            let h = pixel_hash(x, y, seed.wrapping_add(1000));
            let hash_jitter =
                (h % 100) as f64 / 100.0 * params.hash_jitter_range * 2.0 - params.hash_jitter_range;

            let t = (cluster * params.cluster_weight + detail * params.detail_weight + hash_jitter
                + 0.05)
                .clamp(0.0, 1.0);

            let color = palette_select(&params.palette, posterize(t, params.posterize_levels));

            let shift_range = params.brightness_shift_range as u32 * 2 + 1;
            let brightness_shift = ((h >> 8) % shift_range) as i16 - params.brightness_shift_range;
            let color = Rgba::rgb(
                (color.r as i16 + brightness_shift).clamp(0, 255) as u8,
                (color.g as i16 + brightness_shift).clamp(0, 255) as u8,
                (color.b as i16 + brightness_shift).clamp(0, 255) as u8,
            );

            tex.set_pixel(x, y, color);
        }
    }

    tex
}

fn generate_grass(seed: u32) -> Texture {
    generate_grass_parameterized(seed, &GrassParams::default())
}

struct DirtParams {
    base_palette: Vec<Rgba>,
    reddish_color: Rgba,
    base_frequency: f64,
    base_octaves: u32,
    hue_frequency: f64,
    hue_octaves: u32,
    grit_frequency: f64,
    grit_octaves: u32,
    crevice_frequency: f64,
    crevice_octaves: u32,
    pebble_count_min: u32,
    pebble_count_range: u32,
    pebble_darken: u8,
    crevice_darken: u8,
    hue_threshold: f64,
}

impl Default for DirtParams {
    fn default() -> Self {
        let mc_palette = vec![
            Rgba::rgb(80, 60, 35),
            Rgba::rgb(100, 75, 45),
            Rgba::rgb(120, 90, 55),
            Rgba::rgb(140, 105, 65),
            Rgba::rgb(165, 125, 80),
            Rgba::rgb(185, 145, 95),
        ];
        // p=8 f=6: 8 posterize levels sampled from the mc_palette
        let posterize_levels = 8u32;
        let base_palette: Vec<Rgba> = (0..posterize_levels)
            .map(|i| {
                let t = i as f64 / (posterize_levels - 1).max(1) as f64;
                let idx = (t * (mc_palette.len() - 1) as f64).round() as usize;
                mc_palette[idx.min(mc_palette.len() - 1)]
            })
            .collect();
        Self {
            base_palette,
            reddish_color: Rgba::rgb(120, 90, 55),
            base_frequency: 6.0,
            base_octaves: 2,
            hue_frequency: 3.0,
            hue_octaves: 2,
            grit_frequency: 10.0,
            grit_octaves: 1,
            crevice_frequency: 2.0,
            crevice_octaves: 1,
            pebble_count_min: 0,
            pebble_count_range: 0,
            pebble_darken: 0,
            crevice_darken: 5,
            hue_threshold: 1.1,
        }
    }
}

fn generate_dirt_parameterized(seed: u32, params: &DirtParams) -> Texture {
    let mut tex = Texture::new(TILE_SIZE, TILE_SIZE);

    let num_pebbles =
        params.pebble_count_min + (pixel_hash(0, 0, seed) % params.pebble_count_range.max(1));
    let mut pebbles = Vec::with_capacity(num_pebbles as usize);
    for i in 0..num_pebbles {
        let cx = pixel_hash(i, 100, seed) % TILE_SIZE;
        let cy = pixel_hash(i, 200, seed) % TILE_SIZE;
        let radius = 2 + (pixel_hash(i, 300, seed) % 2);
        pebbles.push((cx, cy, radius));
    }

    let base_config = NoiseLayerConfig {
        octaves: params.base_octaves,
        frequency: params.base_frequency,
        lacunarity: 2.0,
        persistence: 0.5,
        x_scale: 1.0,
        y_scale: 1.0,
        seed,
    };

    let hue_config = NoiseLayerConfig {
        octaves: params.hue_octaves,
        frequency: params.hue_frequency,
        lacunarity: 2.0,
        persistence: 0.5,
        x_scale: 1.0,
        y_scale: 1.0,
        seed: seed.wrapping_add(500),
    };

    let grit_config = NoiseLayerConfig {
        octaves: params.grit_octaves,
        frequency: params.grit_frequency,
        lacunarity: 2.0,
        persistence: 0.5,
        x_scale: 1.0,
        y_scale: 1.0,
        seed: seed.wrapping_add(1000),
    };

    let crevice_config = NoiseLayerConfig {
        octaves: params.crevice_octaves,
        frequency: params.crevice_frequency,
        lacunarity: 2.0,
        persistence: 0.5,
        x_scale: 1.0,
        y_scale: 1.0,
        seed: seed.wrapping_add(1500),
    };

    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let base_val = noise_layer(x, y, TILE_SIZE, &base_config);
            let posterized = posterize(base_val, params.base_palette.len() as u32);
            let mut color = palette_select(&params.base_palette, posterized);

            let hue_val = noise_layer(x, y, TILE_SIZE, &hue_config);
            if hue_val > params.hue_threshold {
                color = params.reddish_color;
            }

            for &(cx, cy, radius) in &pebbles {
                let dx = (x as i32 - cx as i32).unsigned_abs();
                let dy = (y as i32 - cy as i32).unsigned_abs();
                if dx * dx + dy * dy <= radius * radius {
                    color = darken(color, params.pebble_darken);
                    break;
                }
            }

            let grit_val = noise_layer(x, y, TILE_SIZE, &grit_config);
            if grit_val > 0.55 {
                let amount = (((grit_val - 0.55) / 0.45) * 5.0) as u8 + 5;
                color = brighten(color, amount);
            } else if grit_val < 0.45 {
                let amount = (((0.45 - grit_val) / 0.45) * 5.0) as u8 + 5;
                color = darken(color, amount);
            }

            let crevice_val = noise_layer(x, y, TILE_SIZE, &crevice_config);
            if crevice_val < 0.3 {
                color = darken(color, params.crevice_darken);
            }

            tex.set_pixel(x, y, color);
        }
    }
    tex
}

fn generate_dirt(seed: u32) -> Texture {
    generate_dirt_parameterized(seed, &DirtParams::default())
}

struct StoneParams {
    num_points: u32,
    edge_divisor: f64,
    fbm_frequency: f64,
    fbm_octaves: u32,
    warm_shift_scale: f64,
    base_gray_min: u8,
    base_gray_range: f64,
}

impl Default for StoneParams {
    fn default() -> Self {
        Self {
            num_points: 13,
            edge_divisor: 0.3,
            fbm_frequency: 6.0,
            fbm_octaves: 2,
            warm_shift_scale: 5.0,
            base_gray_min: 100,
            base_gray_range: 56.0,
        }
    }
}

fn generate_stone_parameterized(seed: u32, params: &StoneParams) -> Texture {
    let mut tex = Texture::new(TILE_SIZE, TILE_SIZE);
    let mut points = Vec::with_capacity(params.num_points as usize);

    for i in 0..params.num_points {
        let h1 = pixel_hash(i, 0, seed);
        let h2 = pixel_hash(i, 1, seed);
        let px = (h1 % TILE_SIZE) as f64;
        let py = (h2 % TILE_SIZE) as f64;
        points.push((px, py));
    }

    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let fx = x as f64;
            let fy = y as f64;

            let mut d1 = f64::MAX;
            let mut d2 = f64::MAX;
            for &(px, py) in &points {
                let mut dx = (fx - px).abs();
                let mut dy = (fy - py).abs();
                if dx > TILE_SIZE as f64 / 2.0 {
                    dx = TILE_SIZE as f64 - dx;
                }
                if dy > TILE_SIZE as f64 / 2.0 {
                    dy = TILE_SIZE as f64 - dy;
                }
                let dist = (dx * dx + dy * dy).sqrt();
                if dist < d1 {
                    d2 = d1;
                    d1 = dist;
                } else if dist < d2 {
                    d2 = dist;
                }
            }

            let edge = (d2 - d1) / (TILE_SIZE as f64 * params.edge_divisor);
            let edge_clamped = edge.clamp(0.0, 1.0);

            let variation = sample_fbm_tiling(
                x as f64,
                y as f64,
                TILE_SIZE as f64,
                1.0,
                1.0,
                params.fbm_octaves,
                params.fbm_frequency,
                2.0,
                0.5,
                seed.wrapping_add(500),
            );
            let warm_shift = (variation * params.warm_shift_scale) as i16;

            let base_gray = params.base_gray_min + (edge_clamped * params.base_gray_range) as u8;
            let r = (base_gray as i16 + warm_shift).clamp(0, 255) as u8;
            let g = base_gray;
            let b = (base_gray as i16 - warm_shift).clamp(0, 255) as u8;

            tex.set_pixel(x, y, Rgba::rgb(r, g, b));
        }
    }
    tex
}

fn generate_stone(seed: u32) -> Texture {
    generate_stone_parameterized(seed, &StoneParams::default())
}

fn generate_brick(seed: u32) -> Texture {
    let mut tex = Texture::new(TILE_SIZE, TILE_SIZE);
    let mortar = Rgba::rgb(180, 175, 165);
    let brick_dark = Rgba::rgb(140, 55, 40);
    let brick_light = Rgba::rgb(180, 80, 55);

    let brick_height: u32 = 8;
    let brick_width: u32 = 16;
    let mortar_size: u32 = 2;

    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let row = y / brick_height;
            let y_in_brick = y % brick_height;

            let x_offset = if row % 2 == 1 { brick_width / 2 } else { 0 };
            let shifted_x = (x + TILE_SIZE - x_offset) % TILE_SIZE;
            let x_in_brick = shifted_x % brick_width;
            let col = shifted_x / brick_width;

            let is_mortar = y_in_brick < mortar_size || x_in_brick < mortar_size;

            if is_mortar {
                tex.set_pixel(x, y, mortar);
            } else {
                let brick_seed = pixel_hash(col, row, seed);
                let t = (brick_seed % 1000) as f64 / 1000.0;
                let base_color = lerp_color(brick_dark, brick_light, t);

                let detail = sample_fbm_tiling(
                    x as f64,
                    y as f64,
                    TILE_SIZE as f64,
                    1.0,
                    1.0,
                    2,
                    8.0,
                    2.0,
                    0.5,
                    seed.wrapping_add(200),
                );
                let shift = (detail * 10.0) as i16;

                let color = Rgba::rgb(
                    (base_color.r as i16 + shift).clamp(0, 255) as u8,
                    (base_color.g as i16 + shift / 2).clamp(0, 255) as u8,
                    (base_color.b as i16 + shift / 3).clamp(0, 255) as u8,
                );
                tex.set_pixel(x, y, color);
            }
        }
    }
    tex
}

const TILE_DEFS: &[TileDef] = &[
    TileDef {
        id: 1,
        name: "stone",
        generator: generate_stone,
    },
    TileDef {
        id: 2,
        name: "dirt",
        generator: generate_dirt,
    },
    TileDef {
        id: 3,
        name: "grass",
        generator: generate_grass,
    },
    TileDef {
        id: 4,
        name: "brick",
        generator: generate_brick,
    },
];

pub fn build_voxel_atlas(seed: u32) -> Result<VoxelTextureAtlas> {
    let mut builder = TextureAtlasBuilder::new();

    for def in TILE_DEFS {
        let tile = (def.generator)(seed);
        builder.add(def.name, tile);
    }

    let atlas = builder.build()?;

    let mut uv_map = [[0.0f32; 4]; 256];

    for def in TILE_DEFS {
        if let Some((uv_min, uv_max)) = atlas.uv_rect(def.name) {
            uv_map[def.id as usize] = [uv_min[0], uv_min[1], uv_max[0], uv_max[1]];
        }
    }

    Ok(VoxelTextureAtlas {
        texture: atlas.into_texture(),
        uv_map,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_atlas_succeeds_and_non_empty() -> Result<()> {
        let atlas = build_voxel_atlas(42)?;
        assert!(atlas.texture().width() > 0);
        assert!(atlas.texture().height() > 0);
        assert!(!atlas.texture().data().is_empty());
        Ok(())
    }

    #[test]
    fn uv_rects_valid_for_defined_ids() -> Result<()> {
        let atlas = build_voxel_atlas(42)?;
        for def in TILE_DEFS {
            let uv = atlas.uv_map()[def.id as usize];
            let [u_min, v_min, u_max, v_max] = uv;
            assert!(
                u_min < u_max,
                "id {} u_min ({}) should be < u_max ({})",
                def.id,
                u_min,
                u_max
            );
            assert!(
                v_min < v_max,
                "id {} v_min ({}) should be < v_max ({})",
                def.id,
                v_min,
                v_max
            );
            assert!(u_min >= 0.0 && u_max <= 1.0, "id {} u out of range", def.id);
            assert!(v_min >= 0.0 && v_max <= 1.0, "id {} v out of range", def.id);
        }
        Ok(())
    }

    #[test]
    fn id_zero_uv_rect_is_zeroed() -> Result<()> {
        let atlas = build_voxel_atlas(42)?;
        assert_eq!(atlas.uv_map()[0], [0.0, 0.0, 0.0, 0.0]);
        Ok(())
    }

    #[test]
    fn sample_fbm_in_range() {
        for i in 0..100 {
            let x = (i % 10) as f64 * 0.1;
            let y = (i / 10) as f64 * 0.1;
            let v = sample_fbm(x, y, 4, 1.0, 2.0, 0.5, 42);
            assert!(
                v > -2.0 && v < 2.0,
                "fbm value {v} out of range at ({x}, {y})"
            );
        }
    }

    #[test]
    fn grass_correct_size_and_opaque() {
        let tex = generate_grass(42);
        assert_eq!(tex.width(), TILE_SIZE);
        assert_eq!(tex.height(), TILE_SIZE);
        assert!(tex.data().iter().all(|p| p.a == 255));
    }

    #[test]
    fn grass_deterministic() {
        let a = generate_grass(42);
        let b = generate_grass(42);
        assert_eq!(a.data(), b.data());
    }

    #[test]
    fn grass_different_seeds_differ() {
        let a = generate_grass(42);
        let b = generate_grass(99);
        assert_ne!(a.data(), b.data());
    }

    #[test]
    fn dirt_correct_size_and_opaque() {
        let tex = generate_dirt(42);
        assert_eq!(tex.width(), TILE_SIZE);
        assert_eq!(tex.height(), TILE_SIZE);
        assert!(tex.data().iter().all(|p| p.a == 255));
    }

    #[test]
    fn dirt_deterministic() {
        let a = generate_dirt(42);
        let b = generate_dirt(42);
        assert_eq!(a.data(), b.data());
    }

    #[test]
    fn dirt_different_seeds_differ() {
        let a = generate_dirt(42);
        let b = generate_dirt(99);
        assert_ne!(a.data(), b.data());
    }

    #[test]
    fn dirt_at_least_3_distinct_colors() {
        let tex = generate_dirt(42);
        let mut unique = std::collections::HashSet::new();
        for p in tex.data() {
            unique.insert((p.r, p.g, p.b));
        }
        assert!(
            unique.len() >= 3,
            "expected at least 3 distinct colors, got {}",
            unique.len()
        );
    }

    #[test]
    fn dirt_red_channel_stddev_above_10() {
        let tex = generate_dirt(42);
        let pixels = tex.data();
        let mean: f64 = pixels.iter().map(|p| p.r as f64).sum::<f64>() / pixels.len() as f64;
        let variance: f64 = pixels
            .iter()
            .map(|p| (p.r as f64 - mean).powi(2))
            .sum::<f64>()
            / pixels.len() as f64;
        let stddev = variance.sqrt();
        assert!(
            stddev > 10.0,
            "dirt red channel stddev {stddev} should be > 10"
        );
    }

    #[test]
    fn stone_correct_size_and_opaque() {
        let tex = generate_stone(42);
        assert_eq!(tex.width(), TILE_SIZE);
        assert_eq!(tex.height(), TILE_SIZE);
        assert!(tex.data().iter().all(|p| p.a == 255));
    }

    #[test]
    fn stone_deterministic() {
        let a = generate_stone(42);
        let b = generate_stone(42);
        assert_eq!(a.data(), b.data());
    }

    #[test]
    fn stone_has_brightness_variation() {
        let tex = generate_stone(42);
        let pixels = tex.data();
        let mean: f64 = pixels.iter().map(|p| p.g as f64).sum::<f64>() / pixels.len() as f64;
        let variance: f64 = pixels
            .iter()
            .map(|p| (p.g as f64 - mean).powi(2))
            .sum::<f64>()
            / pixels.len() as f64;
        let stddev = variance.sqrt();
        assert!(stddev > 5.0, "stone stddev {stddev} too low");
    }

    #[test]
    fn brick_correct_size_and_opaque() {
        let tex = generate_brick(42);
        assert_eq!(tex.width(), TILE_SIZE);
        assert_eq!(tex.height(), TILE_SIZE);
        assert!(tex.data().iter().all(|p| p.a == 255));
    }

    #[test]
    fn brick_deterministic() {
        let a = generate_brick(42);
        let b = generate_brick(42);
        assert_eq!(a.data(), b.data());
    }

    #[test]
    fn brick_has_mortar_and_brick_pixels() {
        let tex = generate_brick(42);
        let pixels = tex.data();
        let mortar_count = pixels.iter().filter(|p| p.r > 150 && p.g > 150).count();
        let brick_count = pixels
            .iter()
            .filter(|p| p.r > 100 && (p.g as u16) < 100)
            .count();
        assert!(mortar_count > 0, "no mortar pixels found");
        assert!(brick_count > 0, "no brick pixels found");
    }

    #[test]
    fn atlas_has_valid_uv_for_ids_1_to_4() -> Result<()> {
        let atlas = build_voxel_atlas(42)?;
        for id in 1..=4u8 {
            let uv = atlas.uv_map()[id as usize];
            assert!(uv[0] < uv[2], "id {id} has invalid u range");
            assert!(uv[1] < uv[3], "id {id} has invalid v range");
        }
        Ok(())
    }

    #[test]
    fn atlas_dimensions_at_least_128() -> Result<()> {
        let atlas = build_voxel_atlas(42)?;
        assert!(atlas.texture().width() >= 128, "atlas width too small");
        assert!(atlas.texture().height() >= 128, "atlas height too small");
        Ok(())
    }

    #[test]
    fn lerp_color_endpoints_and_midpoint() {
        let a = Rgba::rgb(0, 0, 0);
        let b = Rgba::rgb(200, 100, 50);
        let at_0 = lerp_color(a, b, 0.0);
        assert_eq!(at_0, a);
        let at_1 = lerp_color(a, b, 1.0);
        assert_eq!(at_1, b);
        let mid = lerp_color(a, b, 0.5);
        assert_eq!(mid.r, 100);
        assert_eq!(mid.g, 50);
        assert_eq!(mid.b, 25);
    }

    #[test]
    fn posterize_values() {
        assert!((posterize(0.0, 4) - 0.0).abs() < 1e-9);
        assert!((posterize(0.99, 4) - 1.0).abs() < 1e-9);
        assert!((posterize(0.3, 4) - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn palette_select_values() {
        let palette = [
            Rgba::rgb(10, 20, 30),
            Rgba::rgb(40, 50, 60),
            Rgba::rgb(70, 80, 90),
            Rgba::rgb(100, 110, 120),
        ];
        assert_eq!(palette_select(&palette, 0.0), palette[0]);
        assert_eq!(palette_select(&palette, 0.99), palette[3]);
        assert_eq!(palette_select(&palette, 0.5), palette[2]);
    }

    #[test]
    fn sample_fbm_tiling_deterministic() {
        let a = sample_fbm_tiling(10.0, 20.0, 64.0, 1.0, 1.0, 4, 1.0, 2.0, 0.5, 42);
        let b = sample_fbm_tiling(10.0, 20.0, 64.0, 1.0, 1.0, 4, 1.0, 2.0, 0.5, 42);
        assert!((a - b).abs() < 1e-12);
    }

    #[test]
    fn sample_fbm_tiling_wraps() {
        // f(0, y) should equal f(tile_size, y) due to torus mapping
        for y in 0..10 {
            let yf = y as f64 * 6.4;
            let at_zero = sample_fbm_tiling(0.0, yf, 64.0, 1.0, 1.0, 4, 1.0, 2.0, 0.5, 42);
            let at_tile =
                sample_fbm_tiling(64.0, yf, 64.0, 1.0, 1.0, 4, 1.0, 2.0, 0.5, 42);
            assert!(
                (at_zero - at_tile).abs() < 1e-10,
                "x-wrap mismatch at y={yf}: {at_zero} vs {at_tile}"
            );
        }
        // f(x, 0) should equal f(x, tile_size)
        for x in 0..10 {
            let xf = x as f64 * 6.4;
            let at_zero = sample_fbm_tiling(xf, 0.0, 64.0, 1.0, 1.0, 4, 1.0, 2.0, 0.5, 42);
            let at_tile =
                sample_fbm_tiling(xf, 64.0, 64.0, 1.0, 1.0, 4, 1.0, 2.0, 0.5, 42);
            assert!(
                (at_zero - at_tile).abs() < 1e-10,
                "y-wrap mismatch at x={xf}: {at_zero} vs {at_tile}"
            );
        }
    }

    #[test]
    fn bayer_dither_2x2_mixed() {
        let mut results = Vec::new();
        for y in 0..2 {
            for x in 0..2 {
                results.push(bayer_dither_2x2(x, y, 0.5, 2));
            }
        }
        let has_zero = results.iter().any(|&v| v == 0);
        let has_one = results.iter().any(|&v| v == 1);
        assert!(
            has_zero && has_one,
            "expected mix of 0s and 1s, got {results:?}"
        );
    }

    #[test]
    fn darken_saturating() {
        let result = darken(Rgba::rgb(100, 50, 20), 30);
        assert_eq!(result, Rgba::rgb(70, 20, 0));
    }

    #[test]
    fn brighten_saturating() {
        let result = brighten(Rgba::rgb(200, 230, 250), 30);
        assert_eq!(result, Rgba::rgb(230, 255, 255));
    }

    #[test]
    fn grass_green_channel_mean_exceeds_red() {
        let tex = generate_grass(42);
        let pixels = tex.data();
        let red_mean: f64 = pixels.iter().map(|p| p.r as f64).sum::<f64>() / pixels.len() as f64;
        let green_mean: f64 = pixels.iter().map(|p| p.g as f64).sum::<f64>() / pixels.len() as f64;
        assert!(
            green_mean > red_mean,
            "green mean ({green_mean}) should exceed red mean ({red_mean})"
        );
    }

    #[test]
    fn grass_all_pixels_are_green_dominant() {
        let tex = generate_grass(42);
        let pixels = tex.data();
        let green_dominant = pixels.iter().filter(|p| p.g > p.r && p.g > p.b).count();
        assert!(
            green_dominant as f64 / pixels.len() as f64 > 0.95,
            "expected >95% green-dominant pixels, got {}%",
            green_dominant * 100 / pixels.len()
        );
    }

    #[test]
    fn grass_green_channel_stddev_above_15() {
        let tex = generate_grass(42);
        let pixels = tex.data();
        let mean: f64 = pixels.iter().map(|p| p.g as f64).sum::<f64>() / pixels.len() as f64;
        let variance: f64 = pixels
            .iter()
            .map(|p| (p.g as f64 - mean).powi(2))
            .sum::<f64>()
            / pixels.len() as f64;
        let stddev = variance.sqrt();
        assert!(
            stddev > 15.0,
            "grass green channel stddev {stddev} should be > 15"
        );
    }

    /// Helper: check that a texture tiles seamlessly by comparing the average
    /// color difference at the seam (column 0 vs column w-1, row 0 vs row h-1)
    /// against the average difference between adjacent columns/rows in the interior.
    /// The seam difference should not be significantly worse than interior differences.
    fn assert_texture_tiles(tex: &Texture, name: &str) {
        let w = tex.width();
        let h = tex.height();
        let data = tex.data();

        let pixel_diff = |a: Rgba, b: Rgba| -> f64 {
            let dr = (a.r as f64 - b.r as f64).abs();
            let dg = (a.g as f64 - b.g as f64).abs();
            let db = (a.b as f64 - b.b as f64).abs();
            (dr + dg + db) / 3.0
        };

        // Horizontal: compare seam (col w-1 → col 0) vs interior column pairs
        let mut seam_diff_h = 0.0;
        for y in 0..h {
            let left = data[(y * w) as usize];
            let right = data[(y * w + w - 1) as usize];
            seam_diff_h += pixel_diff(left, right);
        }
        seam_diff_h /= h as f64;

        let mut interior_diff_h = 0.0;
        let mut count = 0u32;
        for y in 0..h {
            for x in 0..(w - 1) {
                let a = data[(y * w + x) as usize];
                let b = data[(y * w + x + 1) as usize];
                interior_diff_h += pixel_diff(a, b);
                count += 1;
            }
        }
        interior_diff_h /= count as f64;

        // Vertical: compare seam (row h-1 → row 0) vs interior row pairs
        let mut seam_diff_v = 0.0;
        for x in 0..w {
            let top = data[x as usize];
            let bottom = data[((h - 1) * w + x) as usize];
            seam_diff_v += pixel_diff(top, bottom);
        }
        seam_diff_v /= w as f64;

        let mut interior_diff_v = 0.0;
        count = 0;
        for y in 0..(h - 1) {
            for x in 0..w {
                let a = data[(y * w + x) as usize];
                let b = data[((y + 1) * w + x) as usize];
                interior_diff_v += pixel_diff(a, b);
                count += 1;
            }
        }
        interior_diff_v /= count as f64;

        // Seam should be no more than 4x the average interior difference.
        // Patterns with sharp structural boundaries (e.g. brick mortar) at the
        // tile edge raise the seam average even when tiling is correct.
        let max_ratio = 4.0;
        assert!(
            seam_diff_h <= interior_diff_h * max_ratio,
            "{name} horizontal seam too discontinuous: seam_avg={seam_diff_h:.1}, interior_avg={interior_diff_h:.1}, ratio={:.1}",
            seam_diff_h / interior_diff_h
        );
        assert!(
            seam_diff_v <= interior_diff_v * max_ratio,
            "{name} vertical seam too discontinuous: seam_avg={seam_diff_v:.1}, interior_avg={interior_diff_v:.1}, ratio={:.1}",
            seam_diff_v / interior_diff_v
        );
    }

    #[test]
    fn grass_tiles_seamlessly() {
        let tex = generate_grass(42);
        assert_texture_tiles(&tex, "grass");
    }

    #[test]
    fn dirt_tiles_seamlessly() {
        let tex = generate_dirt(42);
        assert_texture_tiles(&tex, "dirt");
    }

    #[test]
    fn stone_tiles_seamlessly() {
        let tex = generate_stone(42);
        assert_texture_tiles(&tex, "stone");
    }

    #[test]
    fn brick_tiles_seamlessly() {
        let tex = generate_brick(42);
        assert_texture_tiles(&tex, "brick");
    }
}
