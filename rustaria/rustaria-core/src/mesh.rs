use bytemuck::{Pod, Zeroable};

use crate::block::BlockRegistry;
use crate::chunk::{ChunkData, CHUNK_SIZE};

/// Vertex envoyé au GPU — correspond au `VertexInput` du shader WGSL (40 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color:    [f32; 3],
    pub normal:   [f32; 3],
    pub ao:       f32,
}

/// Définition d'une face de cube : voisin à tester, 4 coins et normale.
struct FaceDef {
    neighbor: (i32, i32, i32),
    corners:  [[f32; 3]; 4],
    normal:   [f32; 3],
}

const FACES: [FaceDef; 6] = [
    // +X (droite)
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
    // -X (gauche)
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
    // +Y (dessus)
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
    // -Y (dessous)
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
    // +Z (avant)
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
    // -Z (arrière)
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

/// Transforme les données d'un chunk en triangles prêts pour le GPU.
/// Algorithme : pour chaque bloc non-air, ajoute les faces dont le voisin est de l'air.
pub fn mesh_chunk(chunk: &ChunkData, registry: &BlockRegistry) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices:  Vec<u32>    = Vec::new();

    for z in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block_id = chunk.get(x, y, z);
                if block_id.is_air() { continue; }

                let color = match registry.get(block_id) {
                    Some(bt) => [bt.color.r, bt.color.g, bt.color.b],
                    None     => [1.0, 0.0, 1.0], // magenta = bloc inconnu
                };

                for face in &FACES {
                    let (nx, ny, nz) = (
                        x as i32 + face.neighbor.0,
                        y as i32 + face.neighbor.1,
                        z as i32 + face.neighbor.2,
                    );

                    let in_bounds = nx >= 0 && ny >= 0 && nz >= 0
                        && nx < CHUNK_SIZE as i32
                        && ny < CHUNK_SIZE as i32
                        && nz < CHUNK_SIZE as i32;

                    let neighbor_is_air = !in_bounds
                        || chunk.get(nx as usize, ny as usize, nz as usize).is_air();

                    if !neighbor_is_air { continue; }

                    let base = vertices.len() as u32;

                    for corner in &face.corners {
                        vertices.push(Vertex {
                            position: [x as f32 + corner[0], y as f32 + corner[1], z as f32 + corner[2]],
                            color,
                            normal: face.normal,
                            ao: 1.0,
                        });
                    }

                    // Quad → 2 triangles (CCW)
                    indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
                }
            }
        }
    }

    (vertices, indices)
}
