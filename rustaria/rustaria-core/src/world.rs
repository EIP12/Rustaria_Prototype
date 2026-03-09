use crate::block::BlockId;
use crate::chunk::{ChunkData, CHUNK_SIZE};

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
