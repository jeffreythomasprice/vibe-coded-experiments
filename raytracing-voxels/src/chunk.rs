const SIZE: usize = 16;
const VOLUME: usize = SIZE * SIZE * SIZE;

pub struct Chunk {
    voxels: [u8; VOLUME],
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            voxels: [0; VOLUME],
        }
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> u8 {
        if x >= SIZE || y >= SIZE || z >= SIZE {
            return 0;
        }
        self.voxels[x + y * SIZE + z * SIZE * SIZE]
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, value: u8) {
        if x >= SIZE || y >= SIZE || z >= SIZE {
            return;
        }
        self.voxels[x + y * SIZE + z * SIZE * SIZE] = value;
    }

    pub fn data(&self) -> &[u8; VOLUME] {
        &self.voxels
    }
}

pub fn generate_test_chunk() -> Chunk {
    let mut chunk = Chunk::new();
    for x in 0..SIZE {
        for y in 0..SIZE {
            for z in 0..SIZE {
                let at_x_boundary = x == 0 || x == SIZE - 1;
                let at_y_boundary = y == 0 || y == SIZE - 1;
                let at_z_boundary = z == 0 || z == SIZE - 1;
                let boundary_count =
                    at_x_boundary as u8 + at_y_boundary as u8 + at_z_boundary as u8;
                if boundary_count >= 2 {
                    chunk.set(x, y, z, 1);
                }
            }
        }
    }
    chunk
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_all_zeros() {
        let chunk = Chunk::new();
        assert!(chunk.data().iter().all(|&v| v == 0));
    }

    #[test]
    fn set_get_roundtrip() {
        let mut chunk = Chunk::new();
        chunk.set(3, 7, 11, 42);
        assert_eq!(chunk.get(3, 7, 11), 42);
    }

    #[test]
    fn out_of_bounds_get_returns_zero() {
        let chunk = Chunk::new();
        assert_eq!(chunk.get(16, 0, 0), 0);
        assert_eq!(chunk.get(0, 16, 0), 0);
        assert_eq!(chunk.get(0, 0, 16), 0);
        assert_eq!(chunk.get(100, 100, 100), 0);
    }

    #[test]
    fn out_of_bounds_set_is_noop() {
        let mut chunk = Chunk::new();
        chunk.set(16, 0, 0, 1);
        chunk.set(0, 16, 0, 1);
        chunk.set(0, 0, 16, 1);
        assert!(chunk.data().iter().all(|&v| v == 0));
    }

    #[test]
    fn test_chunk_has_solid_voxels() {
        let chunk = generate_test_chunk();
        assert!(chunk.data().iter().any(|&v| v != 0));
    }

    #[test]
    fn test_chunk_edges_are_solid() {
        let chunk = generate_test_chunk();
        // Corners should be solid (all 3 coords at boundary)
        assert_ne!(chunk.get(0, 0, 0), 0);
        assert_ne!(chunk.get(15, 15, 15), 0);
        // Edges should be solid (2 coords at boundary)
        assert_ne!(chunk.get(0, 0, 8), 0);
        assert_ne!(chunk.get(0, 8, 0), 0);
        assert_ne!(chunk.get(8, 0, 0), 0);
        // Interior should be air
        assert_eq!(chunk.get(8, 8, 8), 0);
        // Face centers should be air (only 1 coord at boundary)
        assert_eq!(chunk.get(0, 8, 8), 0);
    }
}
