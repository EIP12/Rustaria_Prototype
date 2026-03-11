use noise::{NoiseFn, Perlin};

use crate::block::BlockId;
use crate::chunk::{ChunkData, CHUNK_SIZE};

pub struct TerrainGenerator {
    perlin: Perlin,
}

impl TerrainGenerator {
    pub fn new(seed: u32) -> Self {
        Self {
            perlin: Perlin::new(seed),
        }
    }

    /// Returns terrain height (0..48) for the given world-space (x, z) column.
    fn height_at(&self, world_x: i32, world_z: i32) -> i32 {
        let scale1 = 0.02;
        let scale2 = 0.05;
        let scale3 = 0.1;
        let continent_scale = 0.005;

        let x = world_x as f64;
        let z = world_z as f64;

        // Low-frequency continent mask: determines plains vs mountains regions
        // Offset coordinates to sample different noise values than the detail layers
        let continent = (self.perlin.get([x * continent_scale + 500.0, z * continent_scale + 500.0]) + 1.0) / 2.0; // 0..1

        // 3 octaves of detail noise
        let n1 = self.perlin.get([x * scale1, z * scale1]);
        let n2 = self.perlin.get([x * scale2, z * scale2]) * 0.5;
        let n3 = self.perlin.get([x * scale3, z * scale3]) * 0.25;

        let combined = (n1 + n2 + n3) / 1.75; // roughly -1..1
        let normalized = (combined + 1.0) / 2.0; // 0..1

        // Plains: low flat terrain (~8..14). Mountains: full range (~0..48).
        // continent < 0.4 → plains, continent > 0.6 → mountains, smooth blend in between.
        let continent_blend = ((continent - 0.4) / 0.2).clamp(0.0, 1.0);
        let plain_height = 0.15 + normalized * 0.08; // 0.15..0.23
        let mountain_height = normalized;              // 0.0..1.0
        let blended = plain_height + (mountain_height - plain_height) * continent_blend;

        (blended * 48.0) as i32
    }

    /// Generate a chunk at the given chunk coordinates (cx, cy, cz).
    /// Each chunk is generated independently using the 2D heightmap.
    pub fn generate_chunk(&self, cx: i32, cy: i32, cz: i32) -> ChunkData {
        let mut chunk = ChunkData::new((cx, cy, cz));

        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let world_x = cx * CHUNK_SIZE as i32 + lx as i32;
                let world_z = cz * CHUNK_SIZE as i32 + lz as i32;
                let height = self.height_at(world_x, world_z);

                for ly in 0..CHUNK_SIZE {
                    let world_y = cy * CHUNK_SIZE as i32 + ly as i32;

                    let block = if world_y > height {
                        BlockId::AIR
                    } else if world_y == height {
                        BlockId::GRASS
                    } else if world_y > height - 4 {
                        BlockId::DIRT
                    } else {
                        BlockId::STONE
                    };

                    if !block.is_air() {
                        chunk.set(lx, ly, lz, block);
                    }
                }
            }
        }

        chunk.dirty = true;
        chunk
    }
}

// Keep old test generators for backward compatibility
impl ChunkData {
    pub fn generate_single_block_test() -> Self {
        let mut chunk = Self::new((0, 0, 0));
        chunk.set(0, 0, 0, BlockId::STONE);
        chunk.dirty = false;
        chunk
    }

    pub fn generate_flat_test() -> Self {
        let mut chunk = Self::new((0, 0, 0));
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                chunk.set(x, 0, z, BlockId::STONE);
                chunk.set(x, 1, z, BlockId::DIRT);
                chunk.set(x, 2, z, BlockId::DIRT);
                chunk.set(x, 3, z, BlockId::DIRT);
                chunk.set(x, 4, z, BlockId::GRASS);
            }
        }
        chunk.dirty = false;
        chunk
    }
}
