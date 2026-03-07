use rand::Rng;

#[derive(Debug, Clone)]
pub struct Arena {
    pub bounds: (f32, f32),
    pub bodies: Vec<ArenaBody>,
    pub spawn_points: Vec<SpawnPoint>,
}

#[derive(Debug, Clone)]
pub enum ArenaBody {
    Rectangle {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        rotation: f32,
    },
    Circle {
        x: f32,
        y: f32,
        radius: f32,
    },
}

#[derive(Debug, Clone)]
pub struct SpawnPoint {
    pub position: (f32, f32),
    pub facing: f32,
}

const WALL_THICKNESS: f32 = 10.0;
const SPAWN_MARGIN: f32 = 50.0;

pub fn boundary_walls(width: f32, height: f32) -> Vec<ArenaBody> {
    let half_w = width / 2.0;
    let half_h = height / 2.0;
    let half_t = WALL_THICKNESS / 2.0;

    vec![
        ArenaBody::Rectangle {
            x: half_w,
            y: -half_t,
            width,
            height: WALL_THICKNESS,
            rotation: 0.0,
        },
        ArenaBody::Rectangle {
            x: half_w,
            y: height + half_t,
            width,
            height: WALL_THICKNESS,
            rotation: 0.0,
        },
        ArenaBody::Rectangle {
            x: -half_t,
            y: half_h,
            width: WALL_THICKNESS,
            height,
            rotation: 0.0,
        },
        ArenaBody::Rectangle {
            x: width + half_t,
            y: half_h,
            width: WALL_THICKNESS,
            height,
            rotation: 0.0,
        },
    ]
}

pub fn empty_arena(width: f32, height: f32) -> Arena {
    let walls = boundary_walls(width, height);

    let spawn_points = vec![
        SpawnPoint {
            position: (SPAWN_MARGIN, SPAWN_MARGIN),
            facing: 45.0,
        },
        SpawnPoint {
            position: (width - SPAWN_MARGIN, height - SPAWN_MARGIN),
            facing: 225.0,
        },
    ];

    Arena {
        bounds: (width, height),
        bodies: walls,
        spawn_points,
    }
}

const SPAWN_CLEARANCE: f32 = 60.0;

fn too_close_to_spawns(x: f32, y: f32, spawns: &[SpawnPoint]) -> bool {
    spawns.iter().any(|sp| {
        let dx = x - sp.position.0;
        let dy = y - sp.position.1;
        (dx * dx + dy * dy).sqrt() < SPAWN_CLEARANCE
    })
}

pub fn shapes_arena(
    width: f32,
    height: f32,
    count: usize,
    min_size: f32,
    max_size: f32,
    rng: &mut impl Rng,
) -> Arena {
    let mut arena = empty_arena(width, height);
    let margin = max_size;
    let mut placed = 0;
    let mut attempts = 0;
    let max_attempts = count * 100;

    while placed < count && attempts < max_attempts {
        attempts += 1;
        let x = rng.gen_range(margin..width - margin);
        let y = rng.gen_range(margin..height - margin);

        if too_close_to_spawns(x, y, &arena.spawn_points) {
            continue;
        }

        let size = rng.gen_range(min_size..max_size);
        let body = if rng.gen_bool(0.5) {
            let w = size;
            let h = rng.gen_range(min_size..max_size);
            ArenaBody::Rectangle {
                x,
                y,
                width: w,
                height: h,
                rotation: rng.gen_range(0.0..std::f32::consts::TAU),
            }
        } else {
            ArenaBody::Circle {
                x,
                y,
                radius: size / 2.0,
            }
        };

        arena.bodies.push(body);
        placed += 1;
    }

    arena
}

pub fn maze_arena(
    width: f32,
    height: f32,
    corridor_width: f32,
    wall_thickness: f32,
    rng: &mut impl Rng,
) -> Arena {
    let cols = ((width / corridor_width) as usize).max(2);
    let rows = ((height / corridor_width) as usize).max(2);
    let cell_w = width / cols as f32;
    let cell_h = height / rows as f32;

    // Each cell tracks which walls are present: [north, east, south, west]
    let mut walls_present = vec![vec![[true; 4]; cols]; rows];
    let mut visited = vec![vec![false; cols]; rows];
    let mut stack: Vec<(usize, usize)> = Vec::new();

    // Start at (0, 0)
    visited[0][0] = true;
    stack.push((0, 0));

    while let Some(&(r, c)) = stack.last() {
        let mut neighbors = Vec::new();
        if r > 0 && !visited[r - 1][c] {
            neighbors.push((r - 1, c, 0, 2)); // go north: remove my north, their south
        }
        if c + 1 < cols && !visited[r][c + 1] {
            neighbors.push((r, c + 1, 1, 3)); // go east: remove my east, their west
        }
        if r + 1 < rows && !visited[r + 1][c] {
            neighbors.push((r + 1, c, 2, 0)); // go south: remove my south, their north
        }
        if c > 0 && !visited[r][c - 1] {
            neighbors.push((r, c - 1, 3, 1)); // go west: remove my west, their east
        }

        if neighbors.is_empty() {
            stack.pop();
        } else {
            let idx = rng.gen_range(0..neighbors.len());
            let (nr, nc, my_wall, their_wall) = neighbors[idx];
            walls_present[r][c][my_wall] = false;
            walls_present[nr][nc][their_wall] = false;
            visited[nr][nc] = true;
            stack.push((nr, nc));
        }
    }

    // Convert grid walls to rectangles
    let mut bodies = Vec::new();

    // Bounding walls (same as empty_arena)
    let half_w = width / 2.0;
    let half_h = height / 2.0;
    let bt = WALL_THICKNESS / 2.0;
    bodies.push(ArenaBody::Rectangle {
        x: half_w, y: -bt, width, height: WALL_THICKNESS, rotation: 0.0,
    });
    bodies.push(ArenaBody::Rectangle {
        x: half_w, y: height + bt, width, height: WALL_THICKNESS, rotation: 0.0,
    });
    bodies.push(ArenaBody::Rectangle {
        x: -bt, y: half_h, width: WALL_THICKNESS, height, rotation: 0.0,
    });
    bodies.push(ArenaBody::Rectangle {
        x: width + bt, y: half_h, width: WALL_THICKNESS, height, rotation: 0.0,
    });

    for (r, row) in walls_present.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            let cx = c as f32 * cell_w;
            let cy = r as f32 * cell_h;

            if cell[0] && r > 0 {
                bodies.push(ArenaBody::Rectangle {
                    x: cx + cell_w / 2.0,
                    y: cy,
                    width: cell_w + wall_thickness,
                    height: wall_thickness,
                    rotation: 0.0,
                });
            }

            // East wall (right of cell)
            if cell[1] && c + 1 < cols {
                bodies.push(ArenaBody::Rectangle {
                    x: cx + cell_w,
                    y: cy + cell_h / 2.0,
                    width: wall_thickness,
                    height: cell_h + wall_thickness,
                    rotation: 0.0,
                });
            }
        }
    }

    // Spawn at start (0,0) and end (rows-1, cols-1)
    let spawn_points = vec![
        SpawnPoint {
            position: (cell_w / 2.0, cell_h / 2.0),
            facing: 45.0,
        },
        SpawnPoint {
            position: (
                (cols as f32 - 0.5) * cell_w,
                (rows as f32 - 0.5) * cell_h,
            ),
            facing: 225.0,
        },
    ];

    Arena {
        bounds: (width, height),
        bodies,
        spawn_points,
    }
}

pub fn grid_arena(
    width: f32,
    height: f32,
    spacing: f32,
    pillar_size: f32,
    removal_probability: f32,
    rng: &mut impl Rng,
) -> Arena {
    let mut arena = empty_arena(width, height);
    let margin = spacing;

    let mut x = margin;
    while x <= width - margin {
        let mut y = margin;
        while y <= height - margin {
            if !too_close_to_spawns(x, y, &arena.spawn_points)
                && !rng.gen_bool(removal_probability as f64)
            {
                arena.bodies.push(ArenaBody::Rectangle {
                    x,
                    y,
                    width: pillar_size,
                    height: pillar_size,
                    rotation: 0.0,
                });
            }
            y += spacing;
        }
        x += spacing;
    }

    arena
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_arena_has_four_walls() {
        let arena = empty_arena(800.0, 600.0);
        assert_eq!(arena.bodies.len(), 4);
        for body in &arena.bodies {
            assert!(matches!(body, ArenaBody::Rectangle { .. }));
        }
    }

    #[test]
    fn empty_arena_has_two_spawn_points() {
        let arena = empty_arena(800.0, 600.0);
        assert_eq!(arena.spawn_points.len(), 2);
    }

    #[test]
    fn empty_arena_bounds_match_input() {
        let arena = empty_arena(800.0, 600.0);
        assert_eq!(arena.bounds, (800.0, 600.0));
    }

    #[test]
    fn spawn_points_are_inside_bounds() {
        let arena = empty_arena(800.0, 600.0);
        for sp in &arena.spawn_points {
            assert!(sp.position.0 > 0.0 && sp.position.0 < 800.0);
            assert!(sp.position.1 > 0.0 && sp.position.1 < 600.0);
        }
    }

    #[test]
    fn spawn_points_face_each_other() {
        let arena = empty_arena(800.0, 600.0);
        let diff = (arena.spawn_points[0].facing - arena.spawn_points[1].facing).abs();
        assert_eq!(diff, 180.0);
    }

    #[test]
    fn spawn_points_at_opposite_corners() {
        let arena = empty_arena(800.0, 600.0);
        let sp0 = arena.spawn_points[0].position;
        let sp1 = arena.spawn_points[1].position;
        assert!(sp0.0 < 400.0 && sp0.1 < 300.0);
        assert!(sp1.0 > 400.0 && sp1.1 > 300.0);
    }

    #[test]
    fn shapes_arena_generates_requested_obstacles() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let arena = shapes_arena(800.0, 600.0, 10, 20.0, 50.0, &mut rng);
        // 4 walls + 10 shapes
        assert_eq!(arena.bodies.len(), 4 + 10);
    }

    #[test]
    fn maze_arena_produces_walls() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let arena = maze_arena(800.0, 600.0, 100.0, 5.0, &mut rng);
        // Must have more than just the 4 bounding walls
        assert!(arena.bodies.len() > 4);
        assert_eq!(arena.spawn_points.len(), 2);
    }

    #[test]
    fn grid_arena_respects_removal_probability() {
        use rand::SeedableRng;
        let mut rng_none = rand::rngs::StdRng::seed_from_u64(42);
        let arena_full = grid_arena(800.0, 600.0, 100.0, 20.0, 0.0, &mut rng_none);
        let pillars_full = arena_full.bodies.len() - 4; // subtract walls

        let mut rng_all = rand::rngs::StdRng::seed_from_u64(42);
        let arena_empty = grid_arena(800.0, 600.0, 100.0, 20.0, 1.0, &mut rng_all);
        let pillars_empty = arena_empty.bodies.len() - 4;

        assert!(pillars_full > 0);
        assert_eq!(pillars_empty, 0);
    }
}
