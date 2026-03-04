use bytemuck::{Pod, Zeroable};

use crate::block::BlockRegistry;
use crate::chunk::{ChunkData, CHUNK_SIZE};

// ─────────────────────────────────────────────
// Vertex : position + couleur
// Pod + Zeroable = bytemuck peut le caster directement en &[u8] pour le GPU
// ─────────────────────────────────────────────
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

// ─────────────────────────────────────────────
// Les 6 faces d'un cube, chacune = 4 vertices + 2 triangles
// Ordre : +X, -X, +Y, -Y, +Z, -Z
// ─────────────────────────────────────────────
//
// Multiplicateurs de luminosité par face :
//   Dessus (+Y)  : x1.0  (la plus claire)
//   Côtés        : x0.75
//   Dessous (-Y) : x0.5  (la plus sombre)
// → Permet de distinguer les faces sans shader d'éclairage

struct FaceDef {
    // direction du voisin à vérifier (si air → on ajoute la face)
    neighbor: (i32, i32, i32),
    // 4 coins du quad dans le repère local du bloc
    corners: [[f32; 3]; 4],
    // multiplicateur de lumière
    light: f32,
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
        light: 0.75,
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
        light: 0.75,
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
        light: 1.0,
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
        light: 0.5,
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
        light: 0.75,
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
        light: 0.75,
    },
];

// ─────────────────────────────────────────────
// mesh_chunk : transforme les données du chunk en triangles pour le GPU
//
// Algorithme :
//   Pour chaque bloc non-air du chunk
//     Pour chacune de ses 6 faces
//       Si le voisin dans cette direction est de l'air (ou hors chunk)
//         → ajouter un quad (4 vertices, 2 triangles = 6 indices)
//
// Signature stable : ne changera pas quand on passera au greedy meshing
// ─────────────────────────────────────────────
pub fn mesh_chunk(chunk: &ChunkData, registry: &BlockRegistry) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for z in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block_id = chunk.get(x, y, z);

                // Bloc air → rien à rendre
                if block_id.is_air() {
                    continue;
                }

                // Récupère la couleur de base depuis le registry
                let base_color = match registry.get(block_id) {
                    Some(bt) => [bt.color.r, bt.color.g, bt.color.b],
                    None => [1.0, 0.0, 1.0], // magenta = bloc inconnu, debug visible
                };

                for face in &FACES {
                    // Coordonnées du voisin dans cette direction
                    let nx = x as i32 + face.neighbor.0;
                    let ny = y as i32 + face.neighbor.1;
                    let nz = z as i32 + face.neighbor.2;

                    // Voisin hors chunk → on considère que c'est de l'air
                    // (pour l'alpha avec un seul bloc c'est toujours le cas)
                    let neighbor_is_air = if nx < 0
                        || ny < 0
                        || nz < 0
                        || nx >= CHUNK_SIZE as i32
                        || ny >= CHUNK_SIZE as i32
                        || nz >= CHUNK_SIZE as i32
                    {
                        true
                    } else {
                        chunk.get(nx as usize, ny as usize, nz as usize).is_air()
                    };

                    if !neighbor_is_air {
                        continue; // Face cachée → on ne l'ajoute pas
                    }

                    // Applique le multiplicateur de luminosité (faux éclairage)
                    let color = [
                        base_color[0] * face.light,
                        base_color[1] * face.light,
                        base_color[2] * face.light,
                    ];

                    // Index du premier vertex de ce quad dans le Vec
                    let base_index = vertices.len() as u32;

                    // 4 vertices du quad (position = coin + offset du bloc dans le chunk)
                    for corner in &face.corners {
                        vertices.push(Vertex {
                            position: [
                                x as f32 + corner[0],
                                y as f32 + corner[1],
                                z as f32 + corner[2],
                            ],
                            color,
                        });
                    }

                    // 2 triangles = 6 indices (sens anti-horaire = front face CCW)
                    //   0──1
                    //   │\ │
                    //   │ \│
                    //   3──2
                    indices.extend_from_slice(&[
                        base_index,
                        base_index + 1,
                        base_index + 2,
                        base_index,
                        base_index + 2,
                        base_index + 3,
                    ]);
                }
            }
        }
    }

    (vertices, indices)
}
