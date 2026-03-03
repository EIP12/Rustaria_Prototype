use crate::block::BlockId;

pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

pub enum StorageMode {
    Dense(Box<[BlockId; CHUNK_VOLUME]>),
    Compressed,
}

pub struct ChunkData {
    pub storage: StorageMode,
    pub position: (i32, i32, i32),
    pub dirty: bool,
}

impl ChunkData {

    pub fn new(position: (i32, i32, i32)) -> Self {
        Self {
            storage: StorageMode::Dense(
                Box::new([BlockId::AIR; CHUNK_VOLUME])
            ),
            position,
            dirty: false,
        }
    }

    fn index(x: usize, y: usize, z: usize) -> usize {
        x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> BlockId {
        match &self.storage {
            StorageMode::Dense(blocks) => blocks[Self::index(x, y, z)],
            StorageMode::Compressed => todo!("not implemented yet"),
        }
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, block: BlockId) {
        match &mut self.storage {
            StorageMode::Dense(blocks) => {
                blocks[Self::index(x, y, z)] = block;
                self.dirty = true;
            }
            StorageMode::Compressed => todo!("not implemented yet"),
        }
    }

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
