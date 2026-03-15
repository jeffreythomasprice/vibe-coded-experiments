use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;

use crate::schema_types::{DoorConfigArrangement, Room};
use crate::types::ActiveEffect;

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
    pub active_effects: Vec<ActiveEffect>,
}

#[derive(Resource, Default)]
pub struct Dungeon {
    pub grid: HashMap<IVec2, PlacedRoom>,
}

impl Dungeon {
    pub fn place(&mut self, pos: IVec2, room: Room, rotation: u8) {
        let doors = rotated_doors(&room.door_config.arrangement, rotation);
        self.grid.insert(pos, PlacedRoom { room, rotation, doors, active_effects: Vec::new() });
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::schema_types::{DoorConfig, Room};

    fn make_room(arrangement: DoorConfigArrangement) -> Room {
        Room {
            name: String::new(),
            description: String::new(),
            door_config: DoorConfig { arrangement },
            magnitude: 0.5,
            content: None,
        }
    }

    #[test]
    fn direction_opposite() {
        assert_eq!(Direction::North.opposite(), Direction::South);
        assert_eq!(Direction::East.opposite(), Direction::West);
        assert_eq!(Direction::South.opposite(), Direction::North);
        assert_eq!(Direction::West.opposite(), Direction::East);
    }

    #[test]
    fn direction_offset() {
        assert_eq!(Direction::North.offset(), IVec2::Y);
        assert_eq!(Direction::East.offset(), IVec2::X);
        assert_eq!(Direction::South.offset(), IVec2::NEG_Y);
        assert_eq!(Direction::West.offset(), IVec2::NEG_X);
    }

    #[test]
    fn direction_rotate_cw() {
        assert_eq!(Direction::North.rotate_cw(1), Direction::East);
        assert_eq!(Direction::North.rotate_cw(2), Direction::South);
        assert_eq!(Direction::North.rotate_cw(4), Direction::North);
        assert_eq!(Direction::West.rotate_cw(1), Direction::North);
    }

    #[test]
    fn direction_from_offset() {
        assert_eq!(Direction::from_offset(IVec2::new(0, 1)), Some(Direction::North));
        assert_eq!(Direction::from_offset(IVec2::new(1, 1)), None);
    }

    #[test]
    fn canonical_doors_dead_end() {
        let doors = canonical_doors(&DoorConfigArrangement::DeadEnd);
        assert_eq!(doors, vec![Direction::South]);
    }

    #[test]
    fn canonical_doors_crossroads() {
        let doors = canonical_doors(&DoorConfigArrangement::Crossroads);
        assert_eq!(doors.len(), 4);
    }

    #[test]
    fn rotated_doors_corner() {
        let doors = rotated_doors(&DoorConfigArrangement::Corner, 0);
        assert!(doors.contains(&Direction::South));
        assert!(doors.contains(&Direction::East));
        let rotated = rotated_doors(&DoorConfigArrangement::Corner, 1);
        assert!(rotated.contains(&Direction::West));
        assert!(rotated.contains(&Direction::South));
    }

    #[test]
    fn rotation_count_values() {
        assert_eq!(rotation_count(&DoorConfigArrangement::Crossroads), 1);
        assert_eq!(rotation_count(&DoorConfigArrangement::Straight), 2);
        assert_eq!(rotation_count(&DoorConfigArrangement::DeadEnd), 4);
        assert_eq!(rotation_count(&DoorConfigArrangement::Corner), 4);
        assert_eq!(rotation_count(&DoorConfigArrangement::TIntersection), 4);
    }

    #[test]
    fn candidate_cells_from_single_room() {
        let mut dungeon = Dungeon::default();
        dungeon.place(IVec2::ZERO, make_room(DoorConfigArrangement::Crossroads), 0);
        let candidates = dungeon.candidate_cells();
        assert_eq!(candidates.len(), 4);
        assert!(candidates.contains(&IVec2::new(0, 1)));
        assert!(candidates.contains(&IVec2::new(1, 0)));
        assert!(candidates.contains(&IVec2::new(0, -1)));
        assert!(candidates.contains(&IVec2::new(-1, 0)));
    }

    #[test]
    fn candidate_cells_excludes_occupied() {
        let mut dungeon = Dungeon::default();
        dungeon.place(IVec2::ZERO, make_room(DoorConfigArrangement::Straight), 0);
        // doors: N and S
        dungeon.place(IVec2::new(0, 1), make_room(DoorConfigArrangement::Straight), 0);
        let candidates = dungeon.candidate_cells();
        // (0,1) is occupied, so candidates are (0,-1) and (0,2)
        assert!(candidates.contains(&IVec2::new(0, -1)));
        assert!(candidates.contains(&IVec2::new(0, 2)));
        assert!(!candidates.contains(&IVec2::new(0, 1)));
    }

    #[test]
    fn neighbors_connected_doors() {
        let mut dungeon = Dungeon::default();
        dungeon.place(IVec2::ZERO, make_room(DoorConfigArrangement::Straight), 0);
        dungeon.place(IVec2::new(0, 1), make_room(DoorConfigArrangement::Straight), 0);
        let n = dungeon.neighbors(IVec2::ZERO);
        assert!(n.contains(&IVec2::new(0, 1)));
    }

    #[test]
    fn neighbors_no_matching_door() {
        let mut dungeon = Dungeon::default();
        // straight room at origin: doors N, S
        dungeon.place(IVec2::ZERO, make_room(DoorConfigArrangement::Straight), 0);
        // dead end at (1,0): door S only — no door facing West toward origin
        dungeon.place(IVec2::new(1, 0), make_room(DoorConfigArrangement::DeadEnd), 0);
        let n = dungeon.neighbors(IVec2::ZERO);
        assert!(!n.contains(&IVec2::new(1, 0)));
    }

    #[test]
    fn shortest_path_same_cell() {
        let dungeon = Dungeon::default();
        let path = dungeon.shortest_path(IVec2::ZERO, IVec2::ZERO);
        assert_eq!(path, Some(vec![IVec2::ZERO]));
    }

    #[test]
    fn shortest_path_linear() {
        let mut dungeon = Dungeon::default();
        for y in 0..3 {
            dungeon.place(IVec2::new(0, y), make_room(DoorConfigArrangement::Straight), 0);
        }
        let path = dungeon.shortest_path(IVec2::ZERO, IVec2::new(0, 2)).unwrap();
        assert_eq!(path, vec![IVec2::new(0, 0), IVec2::new(0, 1), IVec2::new(0, 2)]);
    }

    #[test]
    fn shortest_path_unreachable() {
        let mut dungeon = Dungeon::default();
        dungeon.place(IVec2::ZERO, make_room(DoorConfigArrangement::DeadEnd), 0);
        dungeon.place(IVec2::new(5, 5), make_room(DoorConfigArrangement::DeadEnd), 0);
        assert!(dungeon.shortest_path(IVec2::ZERO, IVec2::new(5, 5)).is_none());
    }

    #[test]
    fn reachable_within_steps() {
        let mut dungeon = Dungeon::default();
        for y in 0..5 {
            dungeon.place(IVec2::new(0, y), make_room(DoorConfigArrangement::Straight), 0);
        }
        let reachable = dungeon.reachable_within(IVec2::ZERO, 2);
        assert!(reachable.contains(&IVec2::ZERO));
        assert!(reachable.contains(&IVec2::new(0, 1)));
        assert!(reachable.contains(&IVec2::new(0, 2)));
        // (0,-1) is a candidate cell (unrevealed), reachable at dist 1
        assert!(reachable.contains(&IVec2::new(0, -1)));
        // (0,3) should be reachable at dist 3 — beyond limit
        assert!(!reachable.contains(&IVec2::new(0, 3)));
    }

    #[test]
    fn is_valid_placement_connects() {
        let mut dungeon = Dungeon::default();
        dungeon.place(IVec2::ZERO, make_room(DoorConfigArrangement::Straight), 0);
        // straight at (0,1) with rotation 0 has N/S doors — S matches origin's N
        assert!(dungeon.is_valid_placement(IVec2::new(0, 1), &DoorConfigArrangement::Straight, 0));
        // straight at (1,0) has N/S — no door facing W to connect to origin's E (origin has no E door)
        assert!(!dungeon.is_valid_placement(IVec2::new(1, 0), &DoorConfigArrangement::Straight, 0));
    }

    #[test]
    fn find_first_valid_rotation_found() {
        let mut dungeon = Dungeon::default();
        dungeon.place(IVec2::ZERO, make_room(DoorConfigArrangement::Crossroads), 0);
        // need a dead end at (0,1) with door facing South
        let rot = dungeon.find_first_valid_rotation(
            IVec2::new(0, 1),
            &DoorConfigArrangement::DeadEnd,
            Direction::South,
        );
        assert!(rot.is_some());
        let doors = rotated_doors(&DoorConfigArrangement::DeadEnd, rot.unwrap());
        assert!(doors.contains(&Direction::South));
    }

    #[test]
    fn find_first_valid_rotation_none() {
        let dungeon = Dungeon::default();
        // no neighbors, so no valid placement
        let rot = dungeon.find_first_valid_rotation(
            IVec2::ZERO,
            &DoorConfigArrangement::DeadEnd,
            Direction::South,
        );
        assert!(rot.is_none());
    }
}
