use bytemuck::{Pod, Zeroable};

use crate::block::BlockRegistry;
use crate::chunk::{ChunkData, CHUNK_SIZE};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color:    [f32; 3],
    pub normal:   [f32; 3],
    pub ao:       f32,
}

/// Optional neighbor chunk data for the 6 faces.
/// Used for inter-chunk hidden-face culling.
pub struct ChunkNeighbors<'a> {
    pub pos_x: Option<&'a ChunkData>, // +X neighbor
    pub neg_x: Option<&'a ChunkData>, // -X neighbor
    pub pos_y: Option<&'a ChunkData>, // +Y neighbor
    pub neg_y: Option<&'a ChunkData>, // -Y neighbor
    pub pos_z: Option<&'a ChunkData>, // +Z neighbor
    pub neg_z: Option<&'a ChunkData>, // -Z neighbor
}

impl<'a> ChunkNeighbors<'a> {
    pub fn empty() -> Self {
        Self {
            pos_x: None, neg_x: None,
            pos_y: None, neg_y: None,
            pos_z: None, neg_z: None,
        }
    }
}

struct FaceDef {
    neighbor: (i32, i32, i32),
    corners:  [[f32; 3]; 4],
    normal:   [f32; 3],
}

const FACES: [FaceDef; 6] = [
    // +X
    FaceDef {
        neighbor: (1, 0, 0),
        corners: [
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
            [1.0, 0.0, 1.0],
        ],
        normal: [1.0, 0.0, 0.0],
    },
    // -X
    FaceDef {
        neighbor: (-1, 0, 0),
        corners: [
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
        ],
        normal: [-1.0, 0.0, 0.0],
    },
    // +Y
    FaceDef {
        neighbor: (0, 1, 0),
        corners: [
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
        ],
        normal: [0.0, 1.0, 0.0],
    },
    // -Y
    FaceDef {
        neighbor: (0, -1, 0),
        corners: [
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
        ],
        normal: [0.0, -1.0, 0.0],
    },
    // +Z
    FaceDef {
        neighbor: (0, 0, 1),
        corners: [
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        normal: [0.0, 0.0, 1.0],
    },
    // -Z
    FaceDef {
        neighbor: (0, 0, -1),
        corners: [
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ],
        normal: [0.0, 0.0, -1.0],
    },
];

/// Index into ChunkNeighbors by face direction
fn get_neighbor_for_face<'a>(neighbors: &'a ChunkNeighbors, face_idx: usize) -> Option<&'a ChunkData> {
    match face_idx {
        0 => neighbors.pos_x,
        1 => neighbors.neg_x,
        2 => neighbors.pos_y,
        3 => neighbors.neg_y,
        4 => neighbors.pos_z,
        5 => neighbors.neg_z,
        _ => None,
    }
}

pub fn mesh_chunk(
    chunk: &ChunkData,
    registry: &BlockRegistry,
    neighbors: &ChunkNeighbors,
) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices:  Vec<u32>    = Vec::new();

    let (cx, cy, cz) = chunk.position;
    let offset_x = cx as f32 * CHUNK_SIZE as f32;
    let offset_y = cy as f32 * CHUNK_SIZE as f32;
    let offset_z = cz as f32 * CHUNK_SIZE as f32;

    for z in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block_id = chunk.get(x, y, z);
                if block_id.is_air() { continue; }

                let color = match registry.get(block_id) {
                    Some(bt) => [bt.color.r, bt.color.g, bt.color.b],
                    None     => [1.0, 0.0, 1.0],
                };

                for (face_idx, face) in FACES.iter().enumerate() {
                    let (nx, ny, nz) = (
                        x as i32 + face.neighbor.0,
                        y as i32 + face.neighbor.1,
                        z as i32 + face.neighbor.2,
                    );

                    let neighbor_is_air = if nx >= 0 && ny >= 0 && nz >= 0
                        && nx < CHUNK_SIZE as i32
                        && ny < CHUNK_SIZE as i32
                        && nz < CHUNK_SIZE as i32
                    {
                        // Inside same chunk
                        chunk.get(nx as usize, ny as usize, nz as usize).is_air()
                    } else {
                        // At chunk boundary — check neighbor chunk
                        match get_neighbor_for_face(neighbors, face_idx) {
                            Some(neighbor_chunk) => {
                                // Wrap coordinate to neighbor's local space
                                let lx = nx.rem_euclid(CHUNK_SIZE as i32) as usize;
                                let ly = ny.rem_euclid(CHUNK_SIZE as i32) as usize;
                                let lz = nz.rem_euclid(CHUNK_SIZE as i32) as usize;
                                neighbor_chunk.get(lx, ly, lz).is_air()
                            }
                            None => true, // No neighbor loaded — emit face (safe default)
                        }
                    };

                    if !neighbor_is_air { continue; }

                    let base = vertices.len() as u32;

                    for corner in &face.corners {
                        vertices.push(Vertex {
                            position: [
                                offset_x + x as f32 + corner[0],
                                offset_y + y as f32 + corner[1],
                                offset_z + z as f32 + corner[2],
                            ],
                            color,
                            normal: face.normal,
                            ao: 1.0,
                        });
                    }

                    indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
                }
            }
        }
    }

    (vertices, indices)
}
