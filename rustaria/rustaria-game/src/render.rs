use std::time::{Instant, SystemTime, UNIX_EPOCH};

use bytemuck;
use rustaria_core::{chunk::CHUNK_SIZE, mesh::Vertex, world_manager::WorldManager};

use crate::{frustum, game_state::GameState, pipeline};

impl GameState {
    pub fn render(&mut self) {
        self.renderer.window.request_redraw();

        // FPS counter (exponential moving average)
        let dt = self.last_frame.elapsed().as_secs_f64();
        self.last_frame = Instant::now();
        if dt > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / dt) * 0.1;
        }

        if !self.renderer.is_surface_configured {
            return;
        }

        // Progressive chunk streaming
        self.load_chunks();

        // Toggle day/night on L
        if self.input.toggle_light {
            self.input.toggle_light = false;
            self.is_night = !self.is_night;
            self.day_time = if self.is_night { 0.75 } else { 0.25 };
        }

        // Regenerate world on R
        if self.input.regen_world {
            self.input.regen_world = false;
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos();
            log::info!("Regenerating world with seed: {}", seed);
            self.world = WorldManager::new(seed);
            self.gpu_meshes.clear();
            self.loaded_chunks.clear();
            self.pending_queue.clear();
            self.last_cam_chunk = None;
        }

        self.camera.update(&self.input);
        self.input.mouse_dx = 0.0;
        self.input.mouse_dy = 0.0;
        self.camera.upload(&self.renderer.queue, &self.camera_buffer);

        let light_data = pipeline::day_night_light(self.day_time);
        self.renderer.queue.write_buffer(&self.light_buffer, 0, bytemuck::cast_slice(&light_data));

        let output = match self.renderer.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                let size = self.renderer.window.inner_size();
                self.renderer.resize(size.width, size.height);
                return;
            }
            Err(e) => {
                log::error!("Surface error: {}", e);
                return;
            }
        };

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.renderer.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") }
        );

        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(pipeline::sky_color(self.day_time)),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            let pipeline = if self.debug.show_wireframe() { &self.wireframe_pipeline } else { &self.render_pipeline };
            rp.set_pipeline(pipeline);
            rp.set_bind_group(0, &self.camera_bind_group, &[]);

            // Build frustum from current view-proj for chunk culling
            let vp = self.camera.build_uniform().view_proj;
            let frustum = frustum::Frustum::from_view_proj(&vp);

            let mut draw_calls = 0u32;
            for (&(cx, cy, cz), mesh) in &self.gpu_meshes {
                if !frustum.contains_chunk(cx, cy, cz) {
                    continue;
                }
                rp.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                rp.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                rp.draw_indexed(0..mesh.num_indices, 0, 0..1);
                draw_calls += 1;
            }

            // Chunk border overlay in debug mode
            if self.debug.show_chunk_borders() {
                let s = CHUNK_SIZE as f32;
                let mut border_verts: Vec<Vertex> = Vec::with_capacity(self.loaded_chunks.len() * 24);

                for &(cx, cy, cz) in &self.loaded_chunks {
                    if !frustum.contains_chunk(cx, cy, cz) { continue; }

                    let x0 = cx as f32 * s; let y0 = cy as f32 * s; let z0 = cz as f32 * s;
                    let x1 = x0 + s;        let y1 = y0 + s;        let z1 = z0 + s;

                    // Yellow = chunk with terrain, cyan = air chunk
                    let color = if self.gpu_meshes.contains_key(&(cx, cy, cz)) {
                        [1.0_f32, 0.9, 0.0]
                    } else {
                        [0.0_f32, 0.8, 0.9]
                    };
                    let n = [0.0, 1.0, 0.0];
                    let v = |p: [f32; 3]| Vertex { position: p, color, normal: n, ao: 1.0 };

                    // Bottom face
                    border_verts.extend_from_slice(&[v([x0,y0,z0]),v([x1,y0,z0]), v([x1,y0,z0]),v([x1,y0,z1]),
                                                     v([x1,y0,z1]),v([x0,y0,z1]), v([x0,y0,z1]),v([x0,y0,z0])]);
                    // Top face
                    border_verts.extend_from_slice(&[v([x0,y1,z0]),v([x1,y1,z0]), v([x1,y1,z0]),v([x1,y1,z1]),
                                                     v([x1,y1,z1]),v([x0,y1,z1]), v([x0,y1,z1]),v([x0,y1,z0])]);
                    // Vertical edges
                    border_verts.extend_from_slice(&[v([x0,y0,z0]),v([x0,y1,z0]), v([x1,y0,z0]),v([x1,y1,z0]),
                                                     v([x1,y0,z1]),v([x1,y1,z1]), v([x0,y0,z1]),v([x0,y1,z1])]);
                }

                if !border_verts.is_empty() {
                    use wgpu::util::DeviceExt;
                    let buf = self.renderer.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Chunk Border Buffer"),
                        contents: bytemuck::cast_slice(&border_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    rp.set_pipeline(&self.chunk_border_pipeline);
                    rp.set_vertex_buffer(0, buf.slice(..));
                    rp.draw(0..border_verts.len() as u32, 0..1);
                }
            }

            // Update title with draw stats
            drop(rp);
            let pos = self.camera.position;
            self.renderer.window.set_title(&format!(
                "Rustaria Alpha | {:.0} FPS | x: {:.1} y: {:.1} z: {:.1} | draw: {}/{} chunks",
                self.fps, pos.x, pos.y, pos.z, draw_calls, self.gpu_meshes.len(),
            ));
        }

        self.renderer.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}
