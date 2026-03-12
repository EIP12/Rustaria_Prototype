use rustaria_core::chunk::CHUNK_SIZE;

/// View-frustum built from the combined view-projection matrix.
///
/// Each of the 6 planes is stored as [a, b, c, d] where the half-space
/// "inside" satisfies:  a·x + b·y + c·z + d >= 0
///
/// Plane extraction uses the Gribb/Hartmann method for wgpu NDC (z: 0..1).
/// Column-major matrix layout expected (glam `to_cols_array_2d` format).
pub struct Frustum {
    planes: [[f32; 4]; 6],
}

impl Frustum {
    pub fn from_view_proj(m: &[[f32; 4]; 4]) -> Self {
        // m[col][row] — build rows from columns
        let row = |i: usize| -> [f32; 4] { [m[0][i], m[1][i], m[2][i], m[3][i]] };
        let add = |a: [f32; 4], b: [f32; 4]| -> [f32; 4] {
            [a[0]+b[0], a[1]+b[1], a[2]+b[2], a[3]+b[3]]
        };
        let sub = |a: [f32; 4], b: [f32; 4]| -> [f32; 4] {
            [a[0]-b[0], a[1]-b[1], a[2]-b[2], a[3]-b[3]]
        };

        let r0 = row(0);
        let r1 = row(1);
        let r2 = row(2);
        let r3 = row(3);

        Self {
            planes: [
                add(r3, r0), // left   (x/w >= -1)
                sub(r3, r0), // right  (x/w <=  1)
                add(r3, r1), // bottom (y/w >= -1)
                sub(r3, r1), // top    (y/w <=  1)
                r2,          // near   (z/w >=  0) — wgpu depth 0..1
                sub(r3, r2), // far    (z/w <=  1)
            ],
        }
    }

    /// Returns false if the chunk AABB is fully outside any frustum plane → safe to cull.
    pub fn contains_chunk(&self, cx: i32, cy: i32, cz: i32) -> bool {
        let s = CHUNK_SIZE as f32;
        let min = [cx as f32 * s, cy as f32 * s, cz as f32 * s];
        let max = [min[0] + s, min[1] + s, min[2] + s];

        for plane in &self.planes {
            let [a, b, c, d] = *plane;
            // "p-vertex": corner of AABB furthest in the direction of the plane normal
            let px = if a >= 0.0 { max[0] } else { min[0] };
            let py = if b >= 0.0 { max[1] } else { min[1] };
            let pz = if c >= 0.0 { max[2] } else { min[2] };
            // If even the most-positive corner is outside, the whole AABB is outside
            if a * px + b * py + c * pz + d < 0.0 {
                return false;
            }
        }
        true
    }
}
