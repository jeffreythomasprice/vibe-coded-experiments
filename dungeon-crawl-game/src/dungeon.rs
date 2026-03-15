use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;

use crate::schema_types::{DoorConfigArrangement, Room};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    pub fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
        }
    }

    pub fn offset(self) -> IVec2 {
        match self {
            Self::North => IVec2::Y,
            Self::East => IVec2::X,
            Self::South => IVec2::NEG_Y,
            Self::West => IVec2::NEG_X,
        }
    }

    pub fn rotate_cw(self, n: u8) -> Self {
        let mut d = self;
        for _ in 0..(n % 4) {
            d = match d {
                Self::North => Self::East,
                Self::East => Self::South,
                Self::South => Self::West,
                Self::West => Self::North,
            };
        }
        d
    }

    pub fn all() -> [Self; 4] {
        [Self::North, Self::East, Self::South, Self::West]
    }

    pub fn from_offset(delta: IVec2) -> Option<Self> {
        match (delta.x, delta.y) {
            (0, 1) => Some(Self::North),
            (1, 0) => Some(Self::East),
            (0, -1) => Some(Self::South),
            (-1, 0) => Some(Self::West),
            _ => None,
        }
    }
}

pub fn canonical_doors(arrangement: &DoorConfigArrangement) -> Vec<Direction> {
    match arrangement {
        DoorConfigArrangement::DeadEnd => vec![Direction::South],
        DoorConfigArrangement::Straight => vec![Direction::North, Direction::South],
        DoorConfigArrangement::Corner => vec![Direction::South, Direction::East],
        DoorConfigArrangement::TIntersection => {
            vec![Direction::East, Direction::South, Direction::West]
        }
        DoorConfigArrangement::Crossroads => {
            vec![
                Direction::North,
                Direction::East,
                Direction::South,
                Direction::West,
            ]
        }
    }
}

pub fn rotated_doors(arrangement: &DoorConfigArrangement, rotation: u8) -> HashSet<Direction> {
    canonical_doors(arrangement)
        .into_iter()
        .map(|d| d.rotate_cw(rotation))
        .collect()
}

pub fn rotation_count(arrangement: &DoorConfigArrangement) -> u8 {
    match arrangement {
        DoorConfigArrangement::Crossroads => 1,
        DoorConfigArrangement::Straight => 2,
        _ => 4,
    }
}

#[derive(Debug, Clone)]
pub struct PlacedRoom {
    pub room: Room,
    pub rotation: u8,
    pub doors: HashSet<Direction>,
}

#[derive(Resource, Default)]
pub struct Dungeon {
    pub grid: HashMap<IVec2, PlacedRoom>,
}

impl Dungeon {
    pub fn place(&mut self, pos: IVec2, room: Room, rotation: u8) {
        let doors = rotated_doors(&room.door_config.arrangement, rotation);
        self.grid.insert(pos, PlacedRoom { room, rotation, doors });
    }

    pub fn candidate_cells(&self) -> HashSet<IVec2> {
        let mut candidates = HashSet::new();
        for (pos, placed) in &self.grid {
            for dir in &placed.doors {
                let neighbor = *pos + dir.offset();
                if !self.grid.contains_key(&neighbor) {
                    candidates.insert(neighbor);
                }
            }
        }
        candidates
    }

    pub fn neighbors(&self, pos: IVec2) -> Vec<IVec2> {
        let Some(placed) = self.grid.get(&pos) else { return vec![] };
        placed.doors.iter().filter_map(|dir| {
            let neighbor = pos + dir.offset();
            let n = self.grid.get(&neighbor)?;
            if n.doors.contains(&dir.opposite()) { Some(neighbor) } else { None }
        }).collect()
    }

    pub fn shortest_path(&self, from: IVec2, to: IVec2) -> Option<Vec<IVec2>> {
        if from == to { return Some(vec![from]); }
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut parent: HashMap<IVec2, IVec2> = HashMap::new();
        visited.insert(from);
        queue.push_back(from);
        while let Some(current) = queue.pop_front() {
            for next in self.neighbors(current) {
                if visited.contains(&next) { continue; }
                parent.insert(next, current);
                if next == to {
                    let mut path = vec![to];
                    let mut cur = to;
                    while let Some(&p) = parent.get(&cur) {
                        path.push(p);
                        cur = p;
                    }
                    path.reverse();
                    return Some(path);
                }
                visited.insert(next);
                queue.push_back(next);
            }
        }
        None
    }

    pub fn reachable_within(&self, from: IVec2, max_steps: i64) -> HashSet<IVec2> {
        let mut visited = HashMap::new();
        let mut queue = VecDeque::new();
        visited.insert(from, 0i64);
        queue.push_back((from, 0i64));
        let mut result = HashSet::new();
        result.insert(from);
        while let Some((pos, dist)) = queue.pop_front() {
            if dist >= max_steps { continue; }
            let Some(placed) = self.grid.get(&pos) else { continue };
            for dir in &placed.doors {
                let next = pos + dir.offset();
                if visited.contains_key(&next) { continue; }
                if self.grid.contains_key(&next) {
                    let n = &self.grid[&next];
                    if n.doors.contains(&dir.opposite()) {
                        visited.insert(next, dist + 1);
                        result.insert(next);
                        queue.push_back((next, dist + 1));
                    }
                } else {
                    // unrevealed candidate cell at frontier
                    result.insert(next);
                    visited.insert(next, dist + 1);
                }
            }
        }
        result
    }

    pub fn find_first_valid_rotation(
        &self,
        pos: IVec2,
        arrangement: &DoorConfigArrangement,
        required_door: Direction,
    ) -> Option<u8> {
        let count = rotation_count(arrangement);
        for rot in 0..count {
            let doors = rotated_doors(arrangement, rot);
            if doors.contains(&required_door) && self.is_valid_placement(pos, arrangement, rot) {
                return Some(rot);
            }
        }
        None
    }

    pub fn find_next_valid_rotation(
        &self,
        pos: IVec2,
        arrangement: &DoorConfigArrangement,
        current: u8,
    ) -> Option<u8> {
        let count = rotation_count(arrangement);
        for i in 1..=count {
            let rot = (current + i) % count;
            if self.is_valid_placement(pos, arrangement, rot) {
                return Some(rot);
            }
        }
        None
    }

    pub fn is_valid_placement(
        &self,
        pos: IVec2,
        arrangement: &DoorConfigArrangement,
        rotation: u8,
    ) -> bool {
        let doors = rotated_doors(arrangement, rotation);

        doors.iter().any(|dir| {
            let neighbor_pos = pos + dir.offset();
            self.grid
                .get(&neighbor_pos)
                .is_some_and(|n| n.doors.contains(&dir.opposite()))
        })
    }
}
