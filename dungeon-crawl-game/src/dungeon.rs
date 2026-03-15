use std::collections::{HashMap, HashSet};

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
