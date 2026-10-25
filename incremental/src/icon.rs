use std::f64::consts::{PI, TAU};

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

use crate::config::game::ICON_AREA;
use crate::hex::{Axial, Layout, Point};
use crate::resource::Resource;

const SEED_SALT: u64 = 0x9E37_79B9_7F4A_7C15;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),
    CubicTo(Point, Point, Point),
    Close,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub segments: Vec<Segment>,
    pub fill: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Icon {
    pub layers: Vec<Layer>,
}

impl Icon {
    pub fn generate(resource: Resource, seed: u64) -> Self {
        let salt = (resource.index() as u64 + 1).wrapping_mul(SEED_SALT);
        let mut rng = Pcg64Mcg::seed_from_u64(seed ^ salt);
        let layers = match resource {
            Resource::Wood => wood(&mut rng),
            Resource::Fire => fire(&mut rng),
            Resource::Earth => earth(&mut rng),
            Resource::Metal => metal(&mut rng),
            Resource::Water => water(&mut rng),
        };
        Icon { layers }
    }

    pub fn to_svg(&self) -> String {
        let mut out = String::from(r#"<svg viewBox="-1 -1 2 2" width="100%" height="100%">"#);
        for layer in &self.layers {
            out.push_str(&format!(
                r#"<path fill="{}" d="{}"/>"#,
                layer.fill,
                path_data(&layer.segments)
            ));
        }
        out.push_str("</svg>");
        out
    }
}

fn path_data(segments: &[Segment]) -> String {
    let pt = |q: &Point| format!("{:.3} {:.3}", q.x, q.y);
    segments
        .iter()
        .map(|s| match s {
            Segment::MoveTo(a) => format!("M{}", pt(a)),
            Segment::LineTo(a) => format!("L{}", pt(a)),
            Segment::QuadTo(c, a) => format!("Q{} {}", pt(c), pt(a)),
            Segment::CubicTo(c1, c2, a) => format!("C{} {} {}", pt(c1), pt(c2), pt(a)),
            Segment::Close => "Z".to_string(),
        })
        .collect::<Vec<_>>()
        .join("")
}

// Unit-disk placements (offset, radius) so that n icons share one fixed footprint.
pub fn icon_slots(n: usize) -> Vec<(Point, f64)> {
    match n {
        0 => Vec::new(),
        1 => vec![(Point::default(), 1.0)],
        _ => {
            let s = (PI / n as f64).sin();
            let radius = s / (1.0 + s);
            let ring = 1.0 - radius;
            let start = if n == 2 { 0.0 } else { -PI / 2.0 };
            (0..n)
                .map(|i| {
                    let angle = start + TAU * i as f64 / n as f64;
                    (Point::new(ring * angle.cos(), ring * angle.sin()), radius)
                })
                .collect()
        }
    }
}

pub fn icon_placements(layout: Layout, a: Axial, n: usize) -> Vec<(Point, f64)> {
    let center = layout.hex_to_pixel(a);
    let area = layout.size * ICON_AREA;
    icon_slots(n)
        .into_iter()
        .map(|(offset, radius)| (center + offset * area, radius * area))
        .collect()
}

pub fn icon_at(layout: Layout, a: Axial, n: usize, world: Point) -> Option<usize> {
    icon_placements(layout, a, n)
        .into_iter()
        .position(|(origin, scale)| {
            let d = world - origin;
            d.x.hypot(d.y) <= scale
        })
}

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn polygon(points: &[Point]) -> Vec<Segment> {
    let mut segments = vec![Segment::MoveTo(points[0])];
    segments.extend(points[1..].iter().map(|&q| Segment::LineTo(q)));
    segments.push(Segment::Close);
    segments
}

fn bar(a: Point, b: Point, half_width: f64) -> Vec<Segment> {
    let d = b - a;
    let len = d.x.hypot(d.y);
    let n = p(-d.y / len * half_width, d.x / len * half_width);
    polygon(&[a + n, b + n, b - n, a - n])
}

fn rotate(segments: &mut [Segment], angle: f64) {
    let (sin, cos) = angle.sin_cos();
    let rot = |q: Point| p(q.x * cos - q.y * sin, q.x * sin + q.y * cos);
    for segment in segments {
        *segment = match *segment {
            Segment::MoveTo(a) => Segment::MoveTo(rot(a)),
            Segment::LineTo(a) => Segment::LineTo(rot(a)),
            Segment::QuadTo(c, a) => Segment::QuadTo(rot(c), rot(a)),
            Segment::CubicTo(c1, c2, a) => Segment::CubicTo(rot(c1), rot(c2), rot(a)),
            Segment::Close => Segment::Close,
        };
    }
}

fn wood(rng: &mut impl Rng) -> Vec<Layer> {
    let width = rng.random_range(0.25..0.4);
    let tilt = rng.random_range(-0.5..0.5);
    let veins = rng.random_range(1..=3);
    let mut leaf = vec![
        Segment::MoveTo(p(0.0, -0.8)),
        Segment::QuadTo(p(width * 2.0, -0.1), p(0.0, 0.8)),
        Segment::QuadTo(p(-width * 2.0, -0.1), p(0.0, -0.8)),
        Segment::Close,
    ];
    let mut detail = bar(p(0.0, -0.5), p(0.0, 0.95), 0.04);
    for i in 0..veins {
        let y = -0.3 + 0.3 * f64::from(i);
        for side in [-1.0, 1.0] {
            detail.extend(bar(p(0.0, y + 0.1), p(side * width * 0.8, y - 0.15), 0.03));
        }
    }
    rotate(&mut leaf, tilt);
    rotate(&mut detail, tilt);
    vec![
        Layer {
            segments: leaf,
            fill: Resource::Wood.color(),
        },
        Layer {
            segments: detail,
            fill: Resource::Wood.accent(),
        },
    ]
}

fn fire(rng: &mut impl Rng) -> Vec<Layer> {
    let tongues = rng.random_range(2..=4);
    let base_sway = rng.random_range(-0.3..0.3);
    (0..tongues)
        .map(|i| {
            let s = 1.0 - 0.28 * f64::from(i);
            let sway = base_sway + rng.random_range(-0.1..0.1);
            let at = |x: f64, y: f64| p(x * s, 0.9 - (0.9 - y) * s);
            let segments = vec![
                Segment::MoveTo(at(0.0, 0.9)),
                Segment::CubicTo(at(0.85, 0.85), at(0.7, 0.0), at(sway, -0.9)),
                Segment::CubicTo(at(-0.7, 0.0), at(-0.85, 0.85), at(0.0, 0.9)),
                Segment::Close,
            ];
            Layer {
                segments,
                fill: if i % 2 == 0 {
                    Resource::Fire.color()
                } else {
                    Resource::Fire.accent()
                },
            }
        })
        .collect()
}

fn earth(rng: &mut impl Rng) -> Vec<Layer> {
    let peaks: usize = rng.random_range(2..=3);
    let step = 1.8 / (2 * peaks) as f64;
    let mut outline = vec![p(-0.9, 0.8)];
    let mut peak_points = Vec::new();
    for j in 0..peaks {
        let peak = p(
            -0.9 + step * (2 * j + 1) as f64,
            rng.random_range(-0.6..-0.1),
        );
        peak_points.push((outline.len(), peak));
        outline.push(peak);
        outline.push(if j + 1 == peaks {
            p(0.9, 0.8)
        } else {
            p(-0.9 + step * (2 * j + 2) as f64, rng.random_range(0.1..0.5))
        });
    }
    let (at, top) = peak_points
        .iter()
        .copied()
        .min_by(|a, b| a.1.y.total_cmp(&b.1.y))
        .expect("at least two peaks");
    let slope = |q: Point| (q.y - top.y) / (q.x - top.x).abs();
    let steepest = slope(outline[at - 1]).max(slope(outline[at + 1]));
    let cap_height = 0.22;
    let cap_half = cap_height / steepest;
    let cap = polygon(&[
        p(top.x - cap_half, top.y + cap_height),
        top,
        p(top.x + cap_half, top.y + cap_height),
    ]);
    vec![
        Layer {
            segments: polygon(&outline),
            fill: Resource::Earth.color(),
        },
        Layer {
            segments: cap,
            fill: Resource::Earth.accent(),
        },
    ]
}

fn metal(rng: &mut impl Rng) -> Vec<Layer> {
    let sides: usize = rng.random_range(5..=8);
    let rotation = rng.random_range(0.0..TAU / sides as f64);
    let facet = rng.random_range(0..sides);
    let vertex = |i: usize| {
        let angle = rotation + TAU * (i % sides) as f64 / sides as f64;
        p(0.9 * angle.cos(), 0.9 * angle.sin())
    };
    let outline: Vec<Point> = (0..sides).map(vertex).collect();
    vec![
        Layer {
            segments: polygon(&outline),
            fill: Resource::Metal.color(),
        },
        Layer {
            segments: polygon(&[p(0.0, 0.0), vertex(facet), vertex(facet + 1)]),
            fill: Resource::Metal.accent(),
        },
    ]
}

fn water(rng: &mut impl Rng) -> Vec<Layer> {
    let w = rng.random_range(0.5..0.7);
    let drop = vec![
        Segment::MoveTo(p(0.0, -0.9)),
        Segment::CubicTo(p(0.1, -0.5), p(w, -0.1), p(w, 0.3)),
        Segment::CubicTo(p(w, 0.65), p(0.35, 0.9), p(0.0, 0.9)),
        Segment::CubicTo(p(-0.35, 0.9), p(-w, 0.65), p(-w, 0.3)),
        Segment::CubicTo(p(-w, -0.1), p(-0.1, -0.5), p(0.0, -0.9)),
        Segment::Close,
    ];
    let x = -w * 0.55;
    let h = rng.random_range(0.2..0.35);
    let shine = vec![
        Segment::MoveTo(p(x, 0.15)),
        Segment::QuadTo(p(x - 0.1, 0.15 + h), p(x + 0.12, 0.15 + h)),
        Segment::QuadTo(p(x + 0.05, 0.15 + h * 0.4), p(x, 0.15)),
        Segment::Close,
    ];
    vec![
        Layer {
            segments: drop,
            fill: Resource::Water.color(),
        },
        Layer {
            segments: shine,
            fill: Resource::Water.accent(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points(icon: &Icon) -> Vec<Point> {
        icon.layers
            .iter()
            .flat_map(|l| l.segments.iter())
            .flat_map(|s| match *s {
                Segment::MoveTo(a) | Segment::LineTo(a) => vec![a],
                Segment::QuadTo(c, a) => vec![c, a],
                Segment::CubicTo(c1, c2, a) => vec![c1, c2, a],
                Segment::Close => vec![],
            })
            .collect()
    }

    #[test]
    fn generation_is_deterministic() {
        for r in Resource::ALL {
            assert_eq!(Icon::generate(r, 42), Icon::generate(r, 42));
        }
    }

    #[test]
    fn seeds_change_the_icon() {
        for r in Resource::ALL {
            let distinct = (0..20)
                .map(|s| Icon::generate(r, s))
                .any(|i| i != Icon::generate(r, 0));
            assert!(distinct, "{r:?}");
        }
    }

    #[test]
    fn icons_stay_inside_the_unit_box() {
        for r in Resource::ALL {
            for seed in 0..200 {
                let icon = Icon::generate(r, seed);
                assert!(!icon.layers.is_empty());
                for q in points(&icon) {
                    assert!(q.x.abs() <= 1.0 && q.y.abs() <= 1.0, "{r:?} {seed} {q:?}");
                }
            }
        }
    }

    #[test]
    fn svg_has_one_path_per_layer() {
        let icon = Icon::generate(Resource::Fire, 7);
        let svg = icon.to_svg();
        assert_eq!(svg.matches("<path").count(), icon.layers.len());
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
    }

    #[test]
    fn slots_fit_in_disk_without_overlap() {
        for n in 1..=5 {
            let slots = icon_slots(n);
            assert_eq!(slots.len(), n);
            for (i, (a, ra)) in slots.iter().enumerate() {
                assert!(a.x.hypot(a.y) + ra <= 1.0 + 1e-9);
                for (b, rb) in &slots[i + 1..] {
                    assert!((*a - *b).x.hypot((*a - *b).y) >= ra + rb - 1e-9);
                }
            }
        }
    }

    #[test]
    fn slots_shrink_as_count_grows() {
        let radii: Vec<f64> = (1..=5).map(|n| icon_slots(n)[0].1).collect();
        assert!(radii.windows(2).all(|w| w[0] > w[1]));
    }

    #[test]
    fn icon_at_hits_slot_centres() {
        let layout = Layout::new(32.0);
        let a = Axial::new(2, -1);
        for n in 1..=5 {
            for (i, (origin, _)) in icon_placements(layout, a, n).into_iter().enumerate() {
                assert_eq!(icon_at(layout, a, n, origin), Some(i));
            }
        }
    }

    #[test]
    fn icon_at_misses_gaps() {
        let layout = Layout::new(32.0);
        let a = Axial::ORIGIN;
        assert_eq!(icon_at(layout, a, 3, layout.hex_to_pixel(a)), None);
        assert_eq!(icon_at(layout, a, 1, layout.hex_to_pixel(a)), Some(0));
        assert_eq!(icon_at(layout, a, 1, Point::new(31.0, 0.0)), None);
    }
}
