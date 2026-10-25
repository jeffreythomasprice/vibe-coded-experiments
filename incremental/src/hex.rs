use std::collections::HashMap;
use std::ops::{Add, Mul, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Axial {
    pub q: i32,
    pub r: i32,
}

impl Axial {
    pub const ORIGIN: Axial = Axial::new(0, 0);

    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    pub fn neighbors(self) -> [Axial; 6] {
        DIRECTIONS.map(|d| self + d)
    }

    pub fn distance(self, other: Axial) -> u32 {
        let dq = self.q.abs_diff(other.q);
        let dr = self.r.abs_diff(other.r);
        let ds = (self.q + self.r).abs_diff(other.q + other.r);
        dq.max(dr).max(ds)
    }
}

impl Add for Axial {
    type Output = Axial;

    fn add(self, rhs: Axial) -> Axial {
        Axial::new(self.q + rhs.q, self.r + rhs.r)
    }
}

pub const DIRECTIONS: [Axial; 6] = [
    Axial::new(1, 0),
    Axial::new(1, -1),
    Axial::new(0, -1),
    Axial::new(-1, 0),
    Axial::new(-1, 1),
    Axial::new(0, 1),
];

// One of each opposite pair, so every adjacent pair is seen from exactly one side.
pub const FORWARD_DIRS: [usize; 3] = [0, 5, 4];

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl Add for Point {
    type Output = Point;

    fn add(self, rhs: Point) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Point;

    fn sub(self, rhs: Point) -> Point {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Point {
    type Output = Point;

    fn mul(self, rhs: f64) -> Point {
        Point::new(self.x * rhs, self.y * rhs)
    }
}

const SQRT3: f64 = 1.732_050_807_568_877_2;

/// Pointy-top layout in screen coordinates (y grows downward).
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    /// Distance from a hex center to each corner.
    pub size: f64,
}

impl Layout {
    pub const fn new(size: f64) -> Self {
        Self { size }
    }

    pub fn hex_to_pixel(self, a: Axial) -> Point {
        let (q, r) = (f64::from(a.q), f64::from(a.r));
        Point::new(self.size * SQRT3 * (q + r / 2.0), self.size * 1.5 * r)
    }

    pub fn pixel_to_hex(self, p: Point) -> Axial {
        let r = p.y / (self.size * 1.5);
        let q = p.x / (self.size * SQRT3) - r / 2.0;
        round_axial(q, r)
    }

    pub fn corners(self, a: Axial) -> [Point; 6] {
        let center = self.hex_to_pixel(a);
        std::array::from_fn(|i| {
            let angle = (60.0 * i as f64 - 30.0).to_radians();
            center + Point::new(angle.cos(), angle.sin()) * self.size
        })
    }

    /// The edge shared with the neighbor in direction `DIRECTIONS[dir]`.
    pub fn edge(self, a: Axial, dir: usize) -> (Point, Point) {
        let corners = self.corners(a);
        (corners[(6 - dir) % 6], corners[(7 - dir) % 6])
    }

    /// Every hex that may intersect the rectangle (slightly over-inclusive).
    pub fn covering(self, min: Point, max: Point) -> impl Iterator<Item = Axial> {
        let row_height = self.size * 1.5;
        let col_width = self.size * SQRT3;
        let r_min = ((min.y - self.size) / row_height).floor() as i32;
        let r_max = ((max.y + self.size) / row_height).ceil() as i32;
        (r_min..=r_max).flat_map(move |r| {
            let shift = f64::from(r) / 2.0;
            let q_min = (min.x / col_width - shift).floor() as i32 - 1;
            let q_max = (max.x / col_width - shift).ceil() as i32 + 1;
            (q_min..=q_max).map(move |q| Axial::new(q, r))
        })
    }
}

fn round_axial(q: f64, r: f64) -> Axial {
    let s = -q - r;
    let (mut rq, mut rr, rs) = (q.round(), r.round(), s.round());
    let (dq, dr, ds) = ((rq - q).abs(), (rr - r).abs(), (rs - s).abs());
    if dq > dr && dq > ds {
        rq = -rr - rs;
    } else if dr > ds {
        rr = -rq - rs;
    }
    Axial::new(rq as i32, rr as i32)
}

#[derive(Debug, Clone)]
pub struct HexGrid<T> {
    cells: HashMap<Axial, T>,
}

impl<T> Default for HexGrid<T> {
    fn default() -> Self {
        Self {
            cells: HashMap::new(),
        }
    }
}

impl<T> HexGrid<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_origin(value: T) -> Self {
        let mut grid = Self::new();
        grid.insert(Axial::ORIGIN, value);
        grid
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub fn get(&self, pos: Axial) -> Option<&T> {
        self.cells.get(&pos)
    }

    pub fn get_mut(&mut self, pos: Axial) -> Option<&mut T> {
        self.cells.get_mut(&pos)
    }

    pub fn contains(&self, pos: Axial) -> bool {
        self.cells.contains_key(&pos)
    }

    pub fn insert(&mut self, pos: Axial, value: T) -> Option<T> {
        self.cells.insert(pos, value)
    }

    pub fn remove(&mut self, pos: Axial) -> Option<T> {
        self.cells.remove(&pos)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Axial, &T)> {
        self.cells.iter().map(|(&pos, v)| (pos, v))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Axial, &mut T)> {
        self.cells.iter_mut().map(|(&pos, v)| (pos, v))
    }

    pub fn neighbors(&self, pos: Axial) -> impl Iterator<Item = (Axial, &T)> {
        pos.neighbors()
            .into_iter()
            .filter_map(|n| self.cells.get(&n).map(|v| (n, v)))
    }

    /// Positions adjacent to at least one occupied cell but not occupied themselves.
    pub fn frontier(&self) -> impl Iterator<Item = Axial> {
        let mut seen = std::collections::HashSet::new();
        self.cells
            .keys()
            .flat_map(|pos| pos.neighbors())
            .filter(|n| !self.cells.contains_key(n))
            .filter(move |n| seen.insert(*n))
    }

    /// Each unordered pair of adjacent occupied cells, exactly once.
    pub fn adjacent_pairs(&self) -> impl Iterator<Item = (Axial, &T, Axial, &T)> {
        self.cells.iter().flat_map(move |(&a, va)| {
            FORWARD_DIRS.iter().filter_map(move |&d| {
                let b = a + DIRECTIONS[d];
                self.cells.get(&b).map(|vb| (a, va, b, vb))
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(n: i32) -> HexGrid<i32> {
        let mut grid = HexGrid::new();
        for q in 0..n {
            grid.insert(Axial::new(q, 0), q);
        }
        grid
    }

    #[test]
    fn starts_with_single_hex() {
        let grid = HexGrid::with_origin(7);
        assert_eq!(grid.len(), 1);
        assert_eq!(grid.get(Axial::ORIGIN), Some(&7));
        assert_eq!(grid.adjacent_pairs().count(), 0);
    }

    #[test]
    fn insert_replaces_and_remove_returns() {
        let mut grid = HexGrid::with_origin(1);
        assert_eq!(grid.insert(Axial::ORIGIN, 2), Some(1));
        assert_eq!(grid.remove(Axial::ORIGIN), Some(2));
        assert!(grid.is_empty());
    }

    #[test]
    fn directions_are_six_distinct_unit_steps() {
        for (i, a) in DIRECTIONS.iter().enumerate() {
            assert_eq!(Axial::ORIGIN.distance(*a), 1);
            for b in &DIRECTIONS[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn line_pairs_counted_once() {
        assert_eq!(line(5).adjacent_pairs().count(), 4);
    }

    #[test]
    fn full_flower_has_twelve_pairs() {
        let mut grid = HexGrid::with_origin(0);
        for n in Axial::ORIGIN.neighbors() {
            grid.insert(n, 0);
        }
        // 6 spokes + 6 ring edges
        assert_eq!(grid.adjacent_pairs().count(), 12);
    }

    #[test]
    fn pairs_are_actually_adjacent_and_unique() {
        let mut grid = HexGrid::with_origin(0);
        for n in Axial::ORIGIN.neighbors() {
            grid.insert(n, 0);
        }
        let mut seen = std::collections::HashSet::new();
        for (a, _, b, _) in grid.adjacent_pairs() {
            assert_eq!(a.distance(b), 1);
            let key = if (a.q, a.r) < (b.q, b.r) {
                (a, b)
            } else {
                (b, a)
            };
            assert!(seen.insert(key));
        }
    }

    #[test]
    fn non_adjacent_cells_are_not_paired() {
        let mut grid = HexGrid::new();
        grid.insert(Axial::new(0, 0), ());
        grid.insert(Axial::new(2, 0), ());
        assert_eq!(grid.adjacent_pairs().count(), 0);
    }

    const LAYOUT: Layout = Layout::new(10.0);

    #[test]
    fn pixel_round_trip() {
        for q in -5..=5 {
            for r in -5..=5 {
                let a = Axial::new(q, r);
                assert_eq!(LAYOUT.pixel_to_hex(LAYOUT.hex_to_pixel(a)), a);
            }
        }
    }

    #[test]
    fn points_near_center_round_to_hex() {
        let a = Axial::new(3, -2);
        let c = LAYOUT.hex_to_pixel(a);
        for (dx, dy) in [(4.0, 0.0), (-4.0, 0.0), (0.0, 4.0), (0.0, -4.0), (3.0, 3.0)] {
            assert_eq!(LAYOUT.pixel_to_hex(c + Point::new(dx, dy)), a);
        }
    }

    #[test]
    fn edge_midpoint_is_between_neighbor_centers() {
        let a = Axial::new(1, 2);
        for (d, dir) in DIRECTIONS.iter().enumerate() {
            let (p, q) = LAYOUT.edge(a, d);
            let mid = (p + q) * 0.5;
            let expected = (LAYOUT.hex_to_pixel(a) + LAYOUT.hex_to_pixel(a + *dir)) * 0.5;
            assert!((mid.x - expected.x).abs() < 1e-9 && (mid.y - expected.y).abs() < 1e-9);
        }
    }

    #[test]
    fn covering_includes_all_centers_in_rect() {
        let (min, max) = (Point::new(-57.0, -33.0), Point::new(81.0, 49.0));
        let covered: std::collections::HashSet<_> = LAYOUT.covering(min, max).collect();
        for q in -20..=20 {
            for r in -20..=20 {
                let a = Axial::new(q, r);
                let c = LAYOUT.hex_to_pixel(a);
                if c.x >= min.x && c.x <= max.x && c.y >= min.y && c.y <= max.y {
                    assert!(covered.contains(&a), "{a:?}");
                }
            }
        }
    }

    #[test]
    fn neighbors_returns_only_occupied() {
        let grid = line(3);
        let found: Vec<_> = grid.neighbors(Axial::new(1, 0)).map(|(p, _)| p).collect();
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn frontier_of_single_hex_is_its_ring() {
        let grid = HexGrid::with_origin(());
        let frontier: std::collections::HashSet<_> = grid.frontier().collect();
        assert_eq!(frontier, Axial::ORIGIN.neighbors().into_iter().collect());
    }

    #[test]
    fn frontier_has_no_duplicates_or_occupied() {
        let grid = line(4);
        let all: Vec<_> = grid.frontier().collect();
        let unique: std::collections::HashSet<_> = all.iter().copied().collect();
        assert_eq!(all.len(), unique.len());
        assert!(all.iter().all(|p| !grid.contains(*p)));
        assert_eq!(all.len(), 12);
    }
}
