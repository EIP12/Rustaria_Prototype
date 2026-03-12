use crate::block::{BlockColor, BlockId, BlockRegistry, BlockType};

pub fn register_defaults(r: &mut BlockRegistry) {
    // AIR must be index 0
    r.register(BlockType { id: BlockId::AIR,   name: "Air",   color: BlockColor { r: 0.0, g: 0.0, b: 0.0 }, solid: false });
    r.register(BlockType { id: BlockId::STONE,  name: "Stone", color: BlockColor { r: 0.5, g: 0.5, b: 0.5 }, solid: true  });
    r.register(BlockType { id: BlockId::DIRT,   name: "Dirt",  color: BlockColor { r: 0.6, g: 0.4, b: 0.2 }, solid: true  });
    r.register(BlockType { id: BlockId::GRASS,   name: "Grass",   color: BlockColor { r: 0.2, g: 0.7, b: 0.2  }, solid: true  });
    r.register(BlockType { id: BlockId::WATER,   name: "Water",   color: BlockColor { r: 0.2, g: 0.4, b: 0.8  }, solid: true  });
    r.register(BlockType { id: BlockId::BEDROCK, name: "Bedrock", color: BlockColor { r: 0.15, g: 0.15, b: 0.15 }, solid: true  });
}
