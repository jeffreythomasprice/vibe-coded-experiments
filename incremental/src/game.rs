use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

use crate::hex::{Axial, HexGrid};
use crate::icon::Icon;
use crate::resource::{self, RESOURCE_COUNT, Resource, ResourceState};

#[derive(Debug, Clone)]
pub struct Cell {
    resources: [Option<ResourceState>; RESOURCE_COUNT],
    icons: [Option<Icon>; RESOURCE_COUNT],
    seed: u64,
}

impl Cell {
    pub fn random(rng: &mut impl Rng) -> Self {
        let seed = rng.random();
        let mut cell = Cell {
            resources: [None; RESOURCE_COUNT],
            icons: Default::default(),
            seed,
        };
        for r in resource::random_set(rng) {
            cell.resources[r.index()] = Some(ResourceState::default());
            cell.icons[r.index()] = Some(Icon::generate(r, seed));
        }
        cell
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn resources(&self) -> impl Iterator<Item = (Resource, &ResourceState, &Icon)> {
        Resource::ALL
            .into_iter()
            .zip(self.resources.iter().zip(&self.icons))
            .filter_map(|(r, (state, icon))| Some((r, state.as_ref()?, icon.as_ref()?)))
    }

    fn gather(&mut self, dt: f64) {
        for state in self.resources.iter_mut().flatten() {
            state.gather(dt);
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum GameError {
    #[error("source hex is not owned")]
    SourceNotOwned,
    #[error("target hex is already owned")]
    TargetOccupied,
    #[error("hexes are not adjacent")]
    NotAdjacent,
    #[error("hex is not owned")]
    NotOwned,
}

#[derive(Debug, Clone)]
pub struct Game {
    grid: HexGrid<Cell>,
    rng: Pcg64Mcg,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let mut rng = Pcg64Mcg::seed_from_u64(seed);
        let origin = Cell::random(&mut rng);
        Self {
            grid: HexGrid::with_origin(origin),
            rng,
        }
    }

    pub fn cell(&self, pos: Axial) -> Option<&Cell> {
        self.grid.get(pos)
    }

    pub fn gather(&mut self, pos: Axial, dt: f64) -> Result<(), GameError> {
        self.grid
            .get_mut(pos)
            .ok_or(GameError::NotOwned)?
            .gather(dt);
        Ok(())
    }

    pub fn grid(&self) -> &HexGrid<Cell> {
        &self.grid
    }

    pub fn is_owned(&self, pos: Axial) -> bool {
        self.grid.contains(pos)
    }

    pub fn expand(&mut self, from: Axial, to: Axial) -> Result<(), GameError> {
        if !self.is_owned(from) {
            return Err(GameError::SourceNotOwned);
        }
        if from.distance(to) != 1 {
            return Err(GameError::NotAdjacent);
        }
        if self.is_owned(to) {
            return Err(GameError::TargetOccupied);
        }
        self.grid.insert(to, Cell::random(&mut self.rng));
        Ok(())
    }

    pub fn remove(&mut self, pos: Axial) -> Result<Cell, GameError> {
        self.grid.remove(pos).ok_or(GameError::NotOwned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_adds_adjacent_hex() {
        let mut game = Game::new(0);
        assert_eq!(game.expand(Axial::ORIGIN, Axial::new(1, 0)), Ok(()));
        assert!(game.is_owned(Axial::new(1, 0)));
    }

    #[test]
    fn expand_rejects_invalid_moves() {
        let mut game = Game::new(0);
        assert_eq!(
            game.expand(Axial::new(5, 5), Axial::new(5, 6)),
            Err(GameError::SourceNotOwned)
        );
        assert_eq!(
            game.expand(Axial::ORIGIN, Axial::new(2, 0)),
            Err(GameError::NotAdjacent)
        );
        game.expand(Axial::ORIGIN, Axial::new(1, 0)).unwrap();
        assert_eq!(
            game.expand(Axial::ORIGIN, Axial::new(1, 0)),
            Err(GameError::TargetOccupied)
        );
    }

    #[test]
    fn remove_makes_hex_empty() {
        let mut game = Game::new(0);
        assert!(game.remove(Axial::ORIGIN).is_ok());
        assert!(matches!(
            game.remove(Axial::ORIGIN),
            Err(GameError::NotOwned)
        ));
    }

    #[test]
    fn new_cells_have_matching_resources_and_icons() {
        let mut game = Game::new(5);
        game.expand(Axial::ORIGIN, Axial::new(1, 0)).unwrap();
        for pos in [Axial::ORIGIN, Axial::new(1, 0)] {
            assert!(game.cell(pos).unwrap().resources().count() >= 1);
        }
    }

    #[test]
    fn gather_fills_every_resource_of_the_hex() {
        let mut game = Game::new(9);
        game.gather(Axial::ORIGIN, 2.0).unwrap();
        for (_, state, _) in game.cell(Axial::ORIGIN).unwrap().resources() {
            assert_eq!(state.current, 2.0);
        }
        assert_eq!(game.gather(Axial::new(4, 4), 1.0), Err(GameError::NotOwned));
    }
}
