use std::collections::HashMap;

use rayon::prelude::*;

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

    /// Generate and insert a single chunk (sequential). Returns dirty neighbor positions.
    pub fn generate_chunk(&mut self, cx: i32, cy: i32, cz: i32) -> Vec<(i32, i32, i32)> {
        let chunk = self.generator.generate_chunk(cx, cy, cz);
        self.chunks.insert((cx, cy, cz), chunk);

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

    /// Generate multiple chunks in parallel via rayon, then insert sequentially.
    /// Returns positions of neighbors marked dirty for re-meshing.
    pub fn generate_chunks_parallel(&mut self, positions: &[(i32, i32, i32)]) -> Vec<(i32, i32, i32)> {
        let generator = &self.generator;

        // Phase 1: parallel generation (read-only on generator)
        let generated: Vec<ChunkData> = positions
            .par_iter()
            .map(|&(cx, cy, cz)| generator.generate_chunk(cx, cy, cz))
            .collect();

        // Phase 2: sequential insertion + dirty neighbor marking
        let mut all_dirty = Vec::new();
        for chunk in generated {
            let pos = chunk.position;
            self.chunks.insert(pos, chunk);

            for (dx, dy, dz) in &[(1,0,0),(-1,0,0),(0,1,0),(0,-1,0),(0,0,1),(0,0,-1)] {
                let npos = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
                if let Some(neighbor) = self.chunks.get_mut(&npos) {
                    neighbor.dirty = true;
                    all_dirty.push(npos);
                }
            }
        }

        all_dirty
    }

    /// Unload a chunk and mark its neighbors dirty so they re-emit border faces.
    pub fn unload_chunk(&mut self, cx: i32, cy: i32, cz: i32) {
        self.chunks.remove(&(cx, cy, cz));
        for (dx, dy, dz) in &[(1,0,0),(-1,0,0),(0,1,0),(0,-1,0),(0,0,1),(0,0,-1)] {
            let npos = (cx + dx, cy + dy, cz + dz);
            if let Some(neighbor) = self.chunks.get_mut(&npos) {
                neighbor.dirty = true;
            }
        }
    }

    pub fn loaded_positions(&self) -> impl Iterator<Item = &(i32, i32, i32)> {
        self.chunks.keys()
    }

    pub fn get_dirty_chunks(&self) -> Vec<(i32, i32, i32)> {
        self.chunks.iter()
            .filter(|(_, chunk)| chunk.dirty)
            .map(|(&pos, _)| pos)
            .collect()
    }

    pub fn clear_dirty(&mut self, pos: (i32, i32, i32)) {
        if let Some(chunk) = self.chunks.get_mut(&pos) {
            chunk.dirty = false;
        }
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}
