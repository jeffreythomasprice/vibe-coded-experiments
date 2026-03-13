use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use glam::{IVec3, Vec3};
use tokio::sync::mpsc;

use crate::chunk::{chunk_file_path, Chunk};
use crate::terrain::TerrainGenerator;
use crate::world::World;

pub enum ChunkCommand {
    UpdateCamera(Vec3),
    Shutdown,
}

pub struct ChunkResult {
    pub pos: IVec3,
    pub chunk: crate::chunk::Chunk,
    pub evict: Option<IVec3>,
}

#[derive(Debug, PartialEq)]
enum LoadAction {
    Load(IVec3),
    Swap { evict: IVec3, load: IVec3 },
}

fn camera_chunk_coord(camera_pos: Vec3) -> IVec3 {
    (camera_pos / 16.0).floor().as_ivec3()
}

fn chebyshev_distance(a: IVec3, b: IVec3) -> i32 {
    (a - b).abs().max_element()
}

fn desired_chunks(camera_chunk: IVec3, load_radius: i32) -> Vec<IVec3> {
    let mut result = Vec::new();
    for x in -load_radius..=load_radius {
        for y in -load_radius..=load_radius {
            for z in -load_radius..=load_radius {
                result.push(camera_chunk + IVec3::new(x, y, z));
            }
        }
    }
    result
}

fn compute_next_load(
    camera_chunk: IVec3,
    loaded_positions: &HashSet<IVec3>,
    desired: &[IVec3],
    max_loaded: usize,
) -> Option<LoadAction> {
    let nearest_unloaded = desired
        .iter()
        .filter(|pos| !loaded_positions.contains(pos))
        .min_by_key(|pos| chebyshev_distance(camera_chunk, **pos));

    let nearest_unloaded = match nearest_unloaded {
        Some(&pos) => pos,
        None => return None,
    };

    let nearest_dist = chebyshev_distance(camera_chunk, nearest_unloaded);

    if loaded_positions.len() < max_loaded {
        return Some(LoadAction::Load(nearest_unloaded));
    }

    let farthest_loaded = loaded_positions
        .iter()
        .max_by_key(|pos| chebyshev_distance(camera_chunk, **pos));

    if let Some(&farthest) = farthest_loaded {
        let farthest_dist = chebyshev_distance(camera_chunk, farthest);
        if farthest_dist > nearest_dist {
            return Some(LoadAction::Swap {
                evict: farthest,
                load: nearest_unloaded,
            });
        }
    }

    None
}

fn load_or_generate_chunk(storage_dir: &Path, pos: IVec3, generator: &TerrainGenerator) -> Chunk {
    let path = chunk_file_path(storage_dir, pos);
    if path.exists() {
        match Chunk::load_from_file(&path) {
            Ok(chunk) => return chunk,
            Err(e) => log::warn!("failed to load chunk at {:?}: {:#}", pos, e),
        }
    }
    generator.generate_chunk(pos)
}

async fn chunk_loader_task(
    load_radius: i32,
    max_loaded: usize,
    cycle_budget: Duration,
    storage_dir: PathBuf,
    seed: u32,
    mut cmd_rx: mpsc::UnboundedReceiver<ChunkCommand>,
    result_tx: mpsc::UnboundedSender<ChunkResult>,
) {
    let generator = TerrainGenerator::new(seed);
    let mut camera_pos = Vec3::ZERO;
    let mut loaded_positions: HashSet<IVec3> = HashSet::new();

    loop {
        // Drain all pending commands
        loop {
            match cmd_rx.try_recv() {
                Ok(ChunkCommand::UpdateCamera(pos)) => camera_pos = pos,
                Ok(ChunkCommand::Shutdown) => return,
                Err(_) => break,
            }
        }

        let camera_chunk = camera_chunk_coord(camera_pos);
        let desired = desired_chunks(camera_chunk, load_radius);

        let cycle_start = tokio::time::Instant::now();
        let mut did_work = false;

        loop {
            let action = compute_next_load(camera_chunk, &loaded_positions, &desired, max_loaded);
            match action {
                Some(LoadAction::Load(pos)) => {
                    let chunk = load_or_generate_chunk(&storage_dir, pos, &generator);
                    loaded_positions.insert(pos);
                    if result_tx
                        .send(ChunkResult {
                            pos,
                            chunk,
                            evict: None,
                        })
                        .is_err()
                    {
                        return;
                    }
                    did_work = true;
                }
                Some(LoadAction::Swap { evict, load }) => {
                    let chunk = load_or_generate_chunk(&storage_dir, load, &generator);
                    loaded_positions.remove(&evict);
                    loaded_positions.insert(load);
                    if result_tx
                        .send(ChunkResult {
                            pos: load,
                            chunk,
                            evict: Some(evict),
                        })
                        .is_err()
                    {
                        return;
                    }
                    did_work = true;
                }
                None => break,
            }

            if cycle_start.elapsed() >= cycle_budget {
                break;
            }
        }

        if !did_work {
            // Nothing to load — wait for new commands
            match cmd_rx.recv().await {
                Some(ChunkCommand::UpdateCamera(pos)) => camera_pos = pos,
                Some(ChunkCommand::Shutdown) | None => return,
            }
        } else {
            let elapsed = cycle_start.elapsed();
            if elapsed < cycle_budget {
                tokio::time::sleep(cycle_budget - elapsed).await;
            }
        }
    }
}

pub struct ChunkManager {
    cmd_tx: mpsc::UnboundedSender<ChunkCommand>,
    result_rx: mpsc::UnboundedReceiver<ChunkResult>,
    _worker: Option<std::thread::JoinHandle<()>>,
    load_radius: i32,
    max_loaded: usize,
    storage_dir: PathBuf,
}

impl ChunkManager {
    pub fn new(
        load_radius: i32,
        max_loaded: usize,
        updates_per_second: f32,
        storage_dir: PathBuf,
        seed: u32,
    ) -> Self {
        let cycle_budget = Duration::from_secs_f32(1.0 / updates_per_second);

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (result_tx, result_rx) = mpsc::unbounded_channel();

        let task_storage_dir = storage_dir.clone();
        let worker = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .expect("failed to create tokio runtime");
            rt.block_on(chunk_loader_task(
                load_radius,
                max_loaded,
                cycle_budget,
                task_storage_dir,
                seed,
                cmd_rx,
                result_tx,
            ));
        });

        Self {
            cmd_tx,
            result_rx,
            _worker: Some(worker),
            load_radius,
            max_loaded,
            storage_dir,
        }
    }

    pub fn update_camera(&self, camera_pos: Vec3) {
        let _ = self.cmd_tx.send(ChunkCommand::UpdateCamera(camera_pos));
    }

    pub fn drain_results(&mut self, world: &mut World) {
        while let Ok(result) = self.result_rx.try_recv() {
            if let Some(evict_pos) = &result.evict {
                self.save_before_evict(world, evict_pos);
                world.remove(evict_pos);
            }
            world.insert(result.pos, result.chunk);
        }
    }

    pub fn load_radius(&self) -> i32 {
        self.load_radius
    }

    pub fn max_loaded(&self) -> usize {
        self.max_loaded
    }

    pub fn desired_count(&self) -> usize {
        let side = 2 * self.load_radius + 1;
        (side * side * side) as usize
    }

    fn save_before_evict(&self, world: &mut World, evict_pos: &IVec3) {
        if world.is_chunk_modified(evict_pos)
            && let Some(chunk) = world.get(evict_pos)
        {
            let path = chunk_file_path(&self.storage_dir, *evict_pos);
            if let Err(e) = chunk.save_to_file(&path) {
                log::warn!("failed to save chunk {:?} on eviction: {e:#}", evict_pos);
            } else {
                world.mark_chunk_saved(evict_pos);
            }
        }
    }

    pub fn save_modified(&self, world: &mut World) {
        for pos in world.take_modified() {
            if let Some(chunk) = world.get(&pos) {
                let path = chunk_file_path(&self.storage_dir, pos);
                if let Err(e) = chunk.save_to_file(&path) {
                    log::warn!("failed to save modified chunk {:?}: {e:#}", pos);
                }
            }
        }
    }

    pub fn save_all_modified(&self, world: &mut World) {
        self.save_modified(world);
    }

    pub fn load_initial(&mut self, camera_pos: Vec3, world: &mut World) {
        self.update_camera(camera_pos);

        let quiet_timeout = Duration::from_millis(100);
        let mut last_recv = std::time::Instant::now();

        loop {
            match self.result_rx.try_recv() {
                Ok(result) => {
                    if let Some(evict_pos) = &result.evict {
                        self.save_before_evict(world, evict_pos);
                        world.remove(evict_pos);
                    }
                    world.insert(result.pos, result.chunk);
                    last_recv = std::time::Instant::now();
                }
                Err(_) => {
                    if last_recv.elapsed() >= quiet_timeout {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }
}

impl Drop for ChunkManager {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(ChunkCommand::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_chunk_coord_at_origin() {
        assert_eq!(camera_chunk_coord(Vec3::ZERO), IVec3::ZERO);
    }

    #[test]
    fn camera_chunk_coord_positive() {
        assert_eq!(
            camera_chunk_coord(Vec3::new(24.0, 20.0, 40.0)),
            IVec3::new(1, 1, 2)
        );
    }

    #[test]
    fn camera_chunk_coord_negative() {
        assert_eq!(
            camera_chunk_coord(Vec3::new(-1.0, 0.0, 0.0)),
            IVec3::new(-1, 0, 0)
        );
    }

    #[test]
    fn chebyshev_distance_nonzero() {
        assert_eq!(
            chebyshev_distance(IVec3::ZERO, IVec3::new(3, 1, 2)),
            3
        );
    }

    #[test]
    fn chebyshev_distance_zero() {
        assert_eq!(chebyshev_distance(IVec3::ZERO, IVec3::ZERO), 0);
    }

    #[test]
    fn desired_chunks_radius_0() {
        let desired = desired_chunks(IVec3::new(5, 0, 5), 0);
        assert_eq!(desired.len(), 1);
        assert_eq!(desired[0], IVec3::new(5, 0, 5));
    }

    #[test]
    fn desired_chunks_radius_1() {
        let desired = desired_chunks(IVec3::ZERO, 1);
        assert_eq!(desired.len(), 27);
        for pos in &desired {
            assert!(chebyshev_distance(IVec3::ZERO, *pos) <= 1);
        }
    }

    #[test]
    fn desired_chunks_all_within_radius() {
        let center = IVec3::new(10, 5, -3);
        let desired = desired_chunks(center, 3);
        for pos in &desired {
            assert!(chebyshev_distance(center, *pos) <= 3);
        }
    }

    #[test]
    fn compute_next_load_empty_returns_load() {
        let camera_chunk = IVec3::ZERO;
        let loaded = HashSet::new();
        let desired = desired_chunks(camera_chunk, 1);
        let action = compute_next_load(camera_chunk, &loaded, &desired, 100);
        assert!(matches!(action, Some(LoadAction::Load(_))));
        if let Some(LoadAction::Load(pos)) = action {
            assert_eq!(pos, IVec3::ZERO);
        }
    }

    #[test]
    fn compute_next_load_all_loaded_returns_none() {
        let camera_chunk = IVec3::ZERO;
        let desired = desired_chunks(camera_chunk, 1);
        let loaded: HashSet<IVec3> = desired.iter().copied().collect();
        let action = compute_next_load(camera_chunk, &loaded, &desired, 100);
        assert_eq!(action, None);
    }

    #[test]
    fn compute_next_load_at_capacity_swaps() {
        let camera_chunk = IVec3::new(5, 0, 0);
        let desired = desired_chunks(camera_chunk, 0);
        let mut loaded = HashSet::new();
        loaded.insert(IVec3::ZERO); // far away
        let action = compute_next_load(camera_chunk, &loaded, &desired, 1);
        assert_eq!(
            action,
            Some(LoadAction::Swap {
                evict: IVec3::ZERO,
                load: IVec3::new(5, 0, 0),
            })
        );
    }

    #[test]
    fn compute_next_load_at_capacity_no_swap_if_nearer() {
        let camera_chunk = IVec3::ZERO;
        let desired = desired_chunks(camera_chunk, 1);
        let mut loaded = HashSet::new();
        loaded.insert(IVec3::ZERO);
        // max_loaded=1, farthest loaded (ZERO) is distance 0, nearest unloaded is distance 1
        let action = compute_next_load(camera_chunk, &loaded, &desired, 1);
        assert_eq!(action, None);
    }

    #[test]
    fn load_initial_fills_all_desired() {
        let mut cm = ChunkManager::new(2, 125, 5.0, PathBuf::from("/nonexistent"), 12345);
        let mut world = World::new();
        cm.load_initial(Vec3::new(8.0, 8.0, 8.0), &mut world);
        assert_eq!(world.chunk_count(), 125);
    }

    #[test]
    fn load_initial_respects_max_loaded() {
        let mut cm = ChunkManager::new(2, 50, 5.0, PathBuf::from("/nonexistent"), 12345);
        let mut world = World::new();
        let cam = Vec3::new(8.0, 8.0, 8.0);
        cm.load_initial(cam, &mut world);
        assert_eq!(world.chunk_count(), 50);
        let camera_chunk = camera_chunk_coord(cam);
        let desired = desired_chunks(camera_chunk, 2);
        for (&pos, _) in world.iter() {
            assert!(desired.contains(&pos));
        }
    }

    #[test]
    fn drain_results_applies_chunks() {
        let mut cm = ChunkManager::new(1, 27, 1000.0, PathBuf::from("/nonexistent"), 12345);
        let mut world = World::new();
        cm.update_camera(Vec3::new(8.0, 8.0, 8.0));
        // Give the background task time to generate
        std::thread::sleep(Duration::from_millis(200));
        cm.drain_results(&mut world);
        assert!(world.chunk_count() > 0);
    }

    #[test]
    fn camera_move_swaps_chunks() {
        let mut cm = ChunkManager::new(1, 27, 5.0, PathBuf::from("/nonexistent"), 12345);
        let mut world = World::new();
        let cam1 = Vec3::new(8.0, 8.0, 8.0);
        cm.load_initial(cam1, &mut world);
        assert_eq!(world.chunk_count(), 27);

        let cam2 = Vec3::new(200.0, 8.0, 8.0);
        cm.update_camera(cam2);
        // Give background task time to process
        std::thread::sleep(Duration::from_millis(500));
        cm.drain_results(&mut world);

        let new_camera_chunk = camera_chunk_coord(cam2);
        for (&pos, _) in world.iter() {
            assert!(chebyshev_distance(new_camera_chunk, pos) <= 1);
        }
    }

    #[test]
    fn load_or_generate_uses_terrain() {
        let generator = TerrainGenerator::new(42);
        let chunk = load_or_generate_chunk(Path::new("/nonexistent"), IVec3::ZERO, &generator);
        // Terrain generator should produce solid voxels at y=0 (below surface)
        assert!(chunk.data().iter().any(|&v| v != 0));
        // Should not match the old wireframe pattern (edges-only)
        // Interior ground-level voxels should be solid with terrain
        assert_ne!(chunk.get(8, 0, 8), 0, "interior ground should be solid terrain");
    }
}
