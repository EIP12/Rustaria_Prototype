use std::collections::HashSet;
use std::time::Instant;

use rustaria_core::{chunk::CHUNK_SIZE, mesh};

use crate::{
    camera::Camera,
    game_state::GameState,
    gpu_mesh::GpuMesh,
    GEN_BUDGET_PER_FRAME, MESH_BUDGET_PER_FRAME, RENDER_DISTANCE, WORLD_DEPTH, WORLD_HEIGHT,
};

impl GameState {
    /// Recompute desired chunk set when camera chunk changes,
    /// unload out-of-range chunks, and enqueue new chunks to load.
    pub(crate) fn update_streaming(&mut self) {
        let cam_chunk = camera_chunk_pos(&self.camera);
        if self.last_cam_chunk == Some(cam_chunk) {
            return;
        }
        self.last_cam_chunk = Some(cam_chunk);
        let (cam_cx, _, cam_cz) = cam_chunk;

        let desired_set = compute_desired_set(cam_cx, cam_cz);

        // Unload chunks outside the render distance
        let unload: Vec<_> = self.loaded_chunks.iter()
            .filter(|p| !desired_set.contains(p))
            .copied()
            .collect();
        for pos in unload {
            self.loaded_chunks.remove(&pos);
            self.world.unload_chunk(pos.0, pos.1, pos.2);
            self.gpu_meshes.remove(&pos);
        }

        // Build sorted load queue (closest first)
        let mut load_list: Vec<_> = desired_set.into_iter()
            .filter(|p| !self.loaded_chunks.contains(p))
            .collect();
        let (_, cam_cy, _) = cam_chunk;
        load_list.sort_by_key(|&(cx, cy, cz)| {
            let dx = (cx - cam_cx).abs();
            let dy = (cy - cam_cy).abs();
            let dz = (cz - cam_cz).abs();
            dx * dx + dy * dy + dz * dz
        });

        self.pending_queue.clear();
        for pos in load_list {
            self.pending_queue.push_back(pos);
        }
    }

    /// Generate and mesh chunks, budget-capped per frame.
    pub(crate) fn load_chunks(&mut self) {
        self.update_streaming();

        // Pop up to GEN_BUDGET_PER_FRAME positions
        let mut batch: Vec<(i32, i32, i32)> = Vec::with_capacity(GEN_BUDGET_PER_FRAME);
        while batch.len() < GEN_BUDGET_PER_FRAME {
            let Some(pos) = self.pending_queue.pop_front() else { break };
            if !self.loaded_chunks.contains(&pos) {
                batch.push(pos);
            }
        }
        if batch.is_empty() {
            return;
        }

        // Generate in parallel
        let t0 = Instant::now();
        self.world.generate_chunks_parallel(&batch);
        for &pos in &batch {
            self.loaded_chunks.insert(pos);
        }
        let gen_count = batch.len();
        let gen_time = t0.elapsed();

        // Build mesh queue: newly generated chunks first, then dirty neighbor chunks
        let batch_set: HashSet<(i32, i32, i32)> = batch.iter().copied().collect();
        let mut mesh_queue: Vec<(i32, i32, i32)> = batch.iter().copied().collect();
        let mut extra = 0usize;
        for pos in self.world.get_dirty_chunks() {
            if !batch_set.contains(&pos) {
                if extra >= MESH_BUDGET_PER_FRAME { break; }
                mesh_queue.push(pos);
                extra += 1;
            }
        }

        // Mesh in parallel
        let t1 = Instant::now();
        let mesh_results = mesh::mesh_chunks_parallel(&mesh_queue, &self.world, &self.registry);
        let mesh_count = mesh_results.len();
        let mesh_time = t1.elapsed();

        // Upload GPU meshes; track which positions produced geometry
        let mut produced: HashSet<(i32, i32, i32)> = HashSet::with_capacity(mesh_results.len());
        for (pos, vertices, indices) in mesh_results {
            self.gpu_meshes.insert(pos, GpuMesh::new(&self.renderer.device, &vertices, &indices));
            self.world.clear_dirty(pos);
            produced.insert(pos);
        }
        // Clear dirty and remove stale GPU mesh for now-empty chunks
        for &pos in &mesh_queue {
            if !produced.contains(&pos) {
                self.world.clear_dirty(pos);
                self.gpu_meshes.remove(&pos);
            }
        }

        log::debug!(
            "gen: {:.2}ms ({} chunks), mesh: {:.2}ms ({} chunks), loaded: {}, gpu: {}",
            gen_time.as_secs_f64() * 1000.0, gen_count,
            mesh_time.as_secs_f64() * 1000.0, mesh_count,
            self.world.chunk_count(),
            self.gpu_meshes.len(),
        );
    }
}

pub fn camera_chunk_pos(camera: &Camera) -> (i32, i32, i32) {
    let pos = camera.position;
    (
        (pos.x / CHUNK_SIZE as f32).floor() as i32,
        (pos.y / CHUNK_SIZE as f32).floor() as i32,
        (pos.z / CHUNK_SIZE as f32).floor() as i32,
    )
}

pub fn compute_desired_set(cam_cx: i32, cam_cz: i32) -> HashSet<(i32, i32, i32)> {
    let mut set = HashSet::new();
    for cy in -WORLD_DEPTH..WORLD_HEIGHT {
        for dz in -RENDER_DISTANCE..=RENDER_DISTANCE {
            for dx in -RENDER_DISTANCE..=RENDER_DISTANCE {
                set.insert((cam_cx + dx, cy, cam_cz + dz));
            }
        }
    }
    set
}
