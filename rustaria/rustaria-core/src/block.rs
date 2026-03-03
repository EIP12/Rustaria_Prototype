#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u16);

impl BlockId {
    pub const AIR: Self = Self(0);
    pub const STONE: Self = Self(1);
    pub const DIRT: Self = Self(2);
    pub const GRASS: Self = Self(3);

    pub fn is_air(self) -> bool { self == Self::AIR }
}

#[derive(Debug, Clone, Copy)]
pub struct BlockColor { pub r: f32, pub g: f32, pub b: f32 }

impl From<BlockColor> for [f32; 3] {
    fn from(c: BlockColor) -> Self { [c.r, c.g, c.b] }
}

pub trait Block {
    fn id(&self) -> BlockId;
    fn is_solid(&self) -> bool;
    fn color(&self) -> BlockColor;
}

#[derive(Debug, Clone)]
pub struct BlockType {
    pub id: BlockId,
    pub name: &'static str,
    pub color: BlockColor,
    pub solid: bool,
}

impl Block for BlockType {
    fn id(&self) -> BlockId { self.id }
    fn is_solid(&self) -> bool { self.solid }
    fn color(&self) -> BlockColor { self.color }
}

pub struct BlockRegistry { blocks: Vec<BlockType> }

impl BlockRegistry {
    pub fn new() -> Self {
        let mut r = Self { blocks: Vec::new() };
        // Enregistrement dans l'ordre — AIR doit être index 0
        r.register(BlockType { id: BlockId::AIR,   name: "Air",   color: BlockColor { r:0.0, g:0.0, b:0.0 }, solid: false });
        r.register(BlockType { id: BlockId::STONE,  name: "Stone", color: BlockColor { r:0.5, g:0.5, b:0.5 }, solid: true  });
        r.register(BlockType { id: BlockId::DIRT,   name: "Dirt",  color: BlockColor { r:0.6, g:0.4, b:0.2 }, solid: true  });
        r.register(BlockType { id: BlockId::GRASS,  name: "Grass", color: BlockColor { r:0.2, g:0.7, b:0.2 }, solid: true  });
        r
    }

    pub fn register(&mut self, block: BlockType) -> BlockId {
        let id = block.id;
        self.blocks.push(block);
        id
    }

    pub fn get(&self, id: BlockId) -> Option<&BlockType> {
        self.blocks.get(id.0 as usize)
    }
}