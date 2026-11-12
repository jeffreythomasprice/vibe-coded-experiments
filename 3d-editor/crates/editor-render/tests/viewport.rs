use std::collections::{BTreeSet, HashSet};
use std::sync::OnceLock;

use editor_cmd::Session;
use editor_core::{Document, ObjectId};
use editor_geom::camera::viewport_camera;
use editor_render::{BACKGROUND, PreparedScene, Renderer, ViewportOptions};
use serde_json::json;

const SIZE: (u32, u32) = (96, 64);

fn renderer() -> &'static Renderer {
    static R: OnceLock<Renderer> = OnceLock::new();
    R.get_or_init(|| Renderer::new().expect("these tests need a wgpu adapter (see CLAUDE.md)"))
}

fn session_with_box() -> (Session, ObjectId) {
    let mut s = Session::new(Document::new("viewport"));
    let v = s
        .execute_value(
            json!({"op": "brush.create_box", "args": {"min": [0, 0, 0], "max": [64, 64, 64]}}),
            "test",
        )
        .to_value();
    let id = v["result"]["id"].as_str().unwrap().parse().unwrap();
    (s, id)
}

fn prepared(s: &Session) -> PreparedScene {
    let textures = Box::leak(Box::default());
    let faces = Box::leak(Box::default());
    let looks = Box::leak(Box::default());
    PreparedScene::new(
        s.document(),
        &BTreeSet::new(),
        None,
        &HashSet::new(),
        &editor_render::SceneEnv {
            scene_dir: None,
            textures,
            faces,
            looks,
            links: &[],
        },
    )
}

fn top_view(p: &PreparedScene) -> editor_geom::camera::View {
    let camera = viewport_camera("viewport.top", p.bounds(), SIZE.0, SIZE.1).unwrap();
    p.view(&camera, SIZE.0, SIZE.1)
}

#[test]
fn pick_map_finds_the_object_under_a_pixel() {
    let (s, id) = session_with_box();
    let p = prepared(&s);
    let map = renderer().pick_map(&p, &top_view(&p), SIZE).unwrap();
    assert_eq!(map.at(SIZE.0 / 2, SIZE.1 / 2), Some(id));
    assert_eq!(map.at(0, 0), None);
    assert_eq!(map.at(SIZE.0, 0), None);
}

#[test]
fn viewport_draws_into_a_caller_texture() {
    let (s, _) = session_with_box();
    let p = prepared(&s);
    let r = renderer();
    let (w, h) = SIZE;
    let texture = r.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: Renderer::COLOR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    r.draw_viewport(
        &p,
        &top_view(&p),
        ViewportOptions {
            grid: None,
            axes: false,
        },
        SIZE,
        &texture.create_view(&Default::default()),
    );

    let padded = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = r.device().create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(padded * h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = r.device().create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        texture.size(),
    );
    r.queue().submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    r.device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap();
    let pixel = |x: u32, y: u32| {
        let i = (y * padded + x * 4) as usize;
        [data[i], data[i + 1], data[i + 2]]
    };
    assert_eq!(pixel(1, 1), BACKGROUND);
    assert_ne!(pixel(w / 2, h / 2), BACKGROUND);
}
