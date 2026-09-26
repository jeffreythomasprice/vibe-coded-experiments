const FIRST_CHAR: u32 = 0x20;
const LAST_CHAR: u32 = 0x7E;
pub const GLYPH_COUNT: usize = (LAST_CHAR - FIRST_CHAR + 1) as usize;
const COLS: u32 = 16;
const ROWS: u32 = 6;
const PADDING: u32 = 2;

#[derive(Copy, Clone)]
pub struct GlyphInfo {
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub size_px: [f32; 2],
    pub bearing: [f32; 2],
    pub advance: f32,
}

pub struct Atlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub ascent: f32,
    pub descent: f32,
    glyphs: [GlyphInfo; GLYPH_COUNT],
}

impl Atlas {
    pub fn glyph(&self, ch: char) -> Option<&GlyphInfo> {
        let code = ch as u32;
        if !(FIRST_CHAR..=LAST_CHAR).contains(&code) {
            return None;
        }
        Some(&self.glyphs[(code - FIRST_CHAR) as usize])
    }
}

struct Rasterized {
    metrics: fontdue::Metrics,
    bitmap: Vec<u8>,
}

pub fn build(device: &wgpu::Device, queue: &wgpu::Queue, font: &fontdue::Font, px: f32) -> Atlas {
    let mut rasters: Vec<Rasterized> = Vec::with_capacity(GLYPH_COUNT);
    let mut max_w: u32 = 1;
    let mut max_h: u32 = 1;
    let mut advance_sum = 0.0f32;
    let mut advance_count = 0u32;

    for code in FIRST_CHAR..=LAST_CHAR {
        let ch = char::from_u32(code).unwrap();
        let missing = ch != ' ' && font.lookup_glyph_index(ch) == 0;
        let (metrics, bitmap) = font.rasterize(ch, px);

        if !missing {
            max_w = max_w.max(metrics.width as u32);
            max_h = max_h.max(metrics.height as u32);
            advance_sum += metrics.advance_width;
            advance_count += 1;
        }

        rasters.push(Rasterized { metrics, bitmap });
    }

    let avg_advance = if advance_count > 0 {
        advance_sum / advance_count as f32
    } else {
        px * 0.6
    };

    // Substitute a solid, clearly-visible box for glyphs the font has no
    // outline for, rather than trusting the font's own (often blank)
    // .notdef glyph. The TR2N font is missing a handful of punctuation
    // characters; HUD text avoids them, but the atlas must not error.
    for (i, raster) in rasters.iter_mut().enumerate() {
        let code = FIRST_CHAR + i as u32;
        let ch = char::from_u32(code).unwrap();
        let missing = ch != ' ' && font.lookup_glyph_index(ch) == 0;
        if missing {
            raster.metrics.width = max_w as usize;
            raster.metrics.height = max_h as usize;
            raster.metrics.xmin = 0;
            raster.metrics.ymin = 0;
            raster.metrics.advance_width = avg_advance;
            raster.bitmap = vec![255u8; (max_w * max_h) as usize];
        }
    }

    let cell_w = max_w + PADDING * 2;
    let cell_h = max_h + PADDING * 2;
    let atlas_w = COLS * cell_w;
    let atlas_h = ROWS * cell_h;

    let mut pixels = vec![0u8; (atlas_w * atlas_h) as usize];
    let mut glyphs = [GlyphInfo {
        uv_min: [0.0, 0.0],
        uv_max: [0.0, 0.0],
        size_px: [0.0, 0.0],
        bearing: [0.0, 0.0],
        advance: 0.0,
    }; GLYPH_COUNT];

    for (i, raster) in rasters.iter().enumerate() {
        let col = i as u32 % COLS;
        let row = i as u32 / COLS;
        let origin_x = col * cell_w + PADDING;
        let origin_y = row * cell_h + PADDING;

        let w = raster.metrics.width as u32;
        let h = raster.metrics.height as u32;
        for y in 0..h {
            let src_row = &raster.bitmap[(y * w) as usize..((y + 1) * w) as usize];
            let dst_start = ((origin_y + y) * atlas_w + origin_x) as usize;
            pixels[dst_start..dst_start + w as usize].copy_from_slice(src_row);
        }

        glyphs[i] = GlyphInfo {
            uv_min: [origin_x as f32 / atlas_w as f32, origin_y as f32 / atlas_h as f32],
            uv_max: [
                (origin_x + w) as f32 / atlas_w as f32,
                (origin_y + h) as f32 / atlas_h as f32,
            ],
            size_px: [w as f32, h as f32],
            bearing: [raster.metrics.xmin as f32, raster.metrics.ymin as f32],
            advance: raster.metrics.advance_width,
        };
    }

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("font atlas"),
        size: wgpu::Extent3d {
            width: atlas_w,
            height: atlas_h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(atlas_w),
            rows_per_image: Some(atlas_h),
        },
        wgpu::Extent3d {
            width: atlas_w,
            height: atlas_h,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("font atlas sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("font atlas bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("font atlas bind group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    let line_metrics = font
        .horizontal_line_metrics(px)
        .unwrap_or(fontdue::LineMetrics {
            ascent: px * 0.8,
            descent: -px * 0.2,
            line_gap: 0.0,
            new_line_size: px,
        });

    Atlas {
        texture,
        view,
        sampler,
        bind_group_layout,
        bind_group,
        ascent: line_metrics.ascent,
        descent: line_metrics.descent,
        glyphs,
    }
}
