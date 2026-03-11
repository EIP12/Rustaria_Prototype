use std::collections::HashMap;

use crate::chunk::ChunkData;
use crate::world::TerrainGenerator;

pub struct WorldManager {
    chunks: HashMap<(i32, i32, i32), ChunkData>,
    generator: TerrainGenerator,
}

impl WorldManager {
    pub fn new(seed: u32) -> Self {
        Self {
            chunks: HashMap::new(),
            generator: TerrainGenerator::new(seed),
        }
    }

    pub fn get_chunk(&self, cx: i32, cy: i32, cz: i32) -> Option<&ChunkData> {
        self.chunks.get(&(cx, cy, cz))
    }

    pub fn has_chunk(&self, cx: i32, cy: i32, cz: i32) -> bool {
        self.chunks.contains_key(&(cx, cy, cz))
    }

    /// Generate and insert a chunk. Returns a list of neighbor positions
    /// that should be marked dirty (for re-meshing).
    pub fn generate_chunk(&mut self, cx: i32, cy: i32, cz: i32) -> Vec<(i32, i32, i32)> {
        let chunk = self.generator.generate_chunk(cx, cy, cz);
        self.chunks.insert((cx, cy, cz), chunk);

        // Mark existing neighbors as dirty so they re-mesh with the new neighbor data
        let mut dirty_neighbors = Vec::new();
        for (dx, dy, dz) in &[(1,0,0),(-1,0,0),(0,1,0),(0,-1,0),(0,0,1),(0,0,-1)] {
            let npos = (cx + dx, cy + dy, cz + dz);
            if let Some(neighbor) = self.chunks.get_mut(&npos) {
                neighbor.dirty = true;
                dirty_neighbors.push(npos);
            }
        }

        dirty_neighbors
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}
