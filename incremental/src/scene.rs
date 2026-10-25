use crate::camera::Camera;
use crate::config::theme::{Color, Theme};
use crate::game::Game;
use crate::hex::{Axial, Layout, Point};
use crate::icon::icon_placements;
use crate::prim::IconPrim;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridUniforms {
    pub center: Point,
    pub zoom: f64,
    pub viewport: Point,
    pub hex_size: f64,
    pub background: Color,
    pub fill: Color,
    pub border: Color,
    pub border_width: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HexLayer {
    Fill,
    Border,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HexInstance {
    pub center: Point,
    pub radius: f64,
    pub color: Color,
    pub border_width: f64,
    pub layer: HexLayer,
}

pub fn grid_uniforms(
    camera: &Camera,
    layout: Layout,
    viewport: Point,
    theme: &Theme,
) -> GridUniforms {
    GridUniforms {
        center: camera.center,
        zoom: camera.zoom,
        viewport,
        hex_size: layout.size,
        background: theme.background,
        fill: theme.empty_fill,
        border: theme.empty_border,
        border_width: theme.border_width,
    }
}

fn is_visible(center: Point, radius: f64, viewport: Point) -> bool {
    center.x >= -radius
        && center.y >= -radius
        && center.x <= viewport.x + radius
        && center.y <= viewport.y + radius
}

fn visible_owned(
    game: &Game,
    camera: &Camera,
    layout: Layout,
    viewport: Point,
) -> impl Iterator<Item = (Axial, Point, f64)> {
    let radius = layout.size * camera.zoom;
    let camera = *camera;
    game.grid().iter().filter_map(move |(pos, _)| {
        let center = camera.world_to_screen(layout.hex_to_pixel(pos), viewport);
        is_visible(center, radius * 1.1, viewport).then_some((pos, center, radius))
    })
}

pub fn hex_instances(
    game: &Game,
    camera: &Camera,
    layout: Layout,
    viewport: Point,
    theme: &Theme,
    selected: Option<Axial>,
) -> Vec<HexInstance> {
    let owned: Vec<_> = visible_owned(game, camera, layout, viewport).collect();
    let mut out = Vec::with_capacity(owned.len() * 2 + 2);
    let instance = |center, radius, color, border_width, layer| HexInstance {
        center,
        radius,
        color,
        border_width,
        layer,
    };
    for &(_, center, radius) in owned.iter().filter(|(p, ..)| Some(*p) != selected) {
        out.push(instance(
            center,
            radius,
            theme.owned_fill,
            0.0,
            HexLayer::Fill,
        ));
    }
    for &(_, center, radius) in &owned {
        out.push(instance(
            center,
            radius,
            theme.owned_border,
            theme.border_width,
            HexLayer::Border,
        ));
    }
    if let Some(&(_, center, radius)) = owned.iter().find(|(p, ..)| Some(*p) == selected) {
        out.push(instance(
            center,
            radius,
            theme.selected_fill,
            0.0,
            HexLayer::Fill,
        ));
        out.push(instance(
            center,
            radius,
            theme.selected_border,
            theme.selected_border_width,
            HexLayer::Border,
        ));
    }
    out
}

pub fn world_icons<'a>(
    game: &'a Game,
    camera: &Camera,
    layout: Layout,
    viewport: Point,
) -> Vec<IconPrim<'a>> {
    let mut out = Vec::new();
    for (pos, ..) in visible_owned(game, camera, layout, viewport) {
        let Some(cell) = game.cell(pos) else {
            continue;
        };
        let placements = icon_placements(layout, pos, cell.resources().count());
        for ((_, _, icon), (origin, scale)) in cell.resources().zip(placements) {
            out.push(IconPrim {
                mesh: &icon.mesh,
                center: camera.world_to_screen(origin, viewport),
                radius: scale * camera.zoom,
                opacity: 1.0,
                tint: None,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::theme::DARK;

    const VIEWPORT: Point = Point::new(800.0, 600.0);

    fn setup() -> (Game, Camera, Layout) {
        (Game::new(1), Camera::default(), Layout::new(32.0))
    }

    #[test]
    fn empty_space_produces_no_instances() {
        let (game, camera, layout) = setup();
        let far = Camera {
            center: Point::new(1.0e5, 1.0e5),
            ..camera
        };
        assert!(hex_instances(&game, &far, layout, VIEWPORT, &DARK, None).is_empty());
        assert!(world_icons(&game, &far, layout, VIEWPORT).is_empty());
    }

    #[test]
    fn unselected_origin_has_fill_then_border() {
        let (game, camera, layout) = setup();
        let out = hex_instances(&game, &camera, layout, VIEWPORT, &DARK, None);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].layer, HexLayer::Fill);
        assert_eq!(out[0].color, DARK.owned_fill);
        assert_eq!(out[1].layer, HexLayer::Border);
        assert_eq!(out[1].color, DARK.owned_border);
        assert_eq!(out[0].center, Point::new(400.0, 300.0));
    }

    #[test]
    fn selected_hex_comes_last_with_selected_colors() {
        let (game, camera, layout) = setup();
        let out = hex_instances(&game, &camera, layout, VIEWPORT, &DARK, Some(Axial::ORIGIN));
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].color, DARK.owned_border);
        assert_eq!(out[1].color, DARK.selected_fill);
        assert_eq!(out[1].layer, HexLayer::Fill);
        assert_eq!(out[2].color, DARK.selected_border);
    }

    #[test]
    fn selected_border_is_the_last_instance() {
        let (game, camera, layout) = setup();
        let out = hex_instances(&game, &camera, layout, VIEWPORT, &DARK, Some(Axial::ORIGIN));
        let last = out.last().unwrap();
        assert_eq!(last.layer, HexLayer::Border);
        assert_eq!(last.color, DARK.selected_border);
        assert_eq!(last.border_width, DARK.selected_border_width);
    }

    #[test]
    fn icons_follow_resource_count() {
        let (game, camera, layout) = setup();
        let count = game.cell(Axial::ORIGIN).unwrap().resources().count();
        let icons = world_icons(&game, &camera, layout, VIEWPORT);
        assert_eq!(icons.len(), count);
        assert!(icons.iter().all(|i| i.radius > 0.0));
    }

    #[test]
    fn instance_radius_scales_with_zoom() {
        let (game, mut camera, layout) = setup();
        camera.zoom = 2.0;
        let out = hex_instances(&game, &camera, layout, VIEWPORT, &DARK, None);
        assert_eq!(out[0].radius, 64.0);
    }
}
