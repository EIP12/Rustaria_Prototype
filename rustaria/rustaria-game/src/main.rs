use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use bytemuck;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::Window,
};

use rustaria_core::{
    block::BlockRegistry,
    chunk::CHUNK_SIZE,
    mesh::{self, ChunkNeighbors},
    world_manager::WorldManager,
};

mod camera;
mod debug;
mod gpu_mesh;
mod input;
mod pipeline;
mod renderer;

use camera::Camera;
use gpu_mesh::GpuMesh;
use input::InputState;
use pipeline::PipelineBundle;
use renderer::Renderer;

// World bounds: 16x4x16 chunks
const WORLD_CX: i32 = 16;
const WORLD_CY: i32 = 4;
const WORLD_CZ: i32 = 16;

// How many chunks to generate + mesh per frame
const CHUNKS_PER_FRAME: usize = 4;

#[derive(Default)]
pub struct App {
    state: Option<GameState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Rustaria Alpha")
                        .with_inner_size(winit::dpi::PhysicalSize::new(1280u32, 720u32)),
                )
                .expect("Impossible de créer la fenêtre"),
        );
        self.state = Some(pollster::block_on(GameState::new(window)));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else { return };

        if input::handle_keyboard(&event, event_loop, &mut state.debug, &mut state.input) {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                state.renderer.resize(size.width, size.height);
                state.depth_texture_view = pipeline::create_depth_texture_view(
                    &state.renderer.device,
                    state.renderer.config.width,
                    state.renderer.config.height,
                );
                state.camera.resize(
                    state.renderer.config.width,
                    state.renderer.config.height,
                );
            }

            WindowEvent::RedrawRequested => state.render(),

            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let Some(state) = &mut self.state {
            input::handle_device_event(&event, &mut state.input);
        }
    }
}

pub struct GameState {
    renderer: Renderer,

    gpu_meshes: HashMap<(i32, i32, i32), GpuMesh>,

    render_pipeline: wgpu::RenderPipeline,
    wireframe_pipeline: wgpu::RenderPipeline,

    camera: Camera,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,

    day_time: f32,
    is_night: bool,
    light_buffer: wgpu::Buffer,

    input: InputState,

    depth_texture_view: wgpu::TextureView,

    debug: debug::DebugOverlay,

    // World state
    world: WorldManager,
    registry: BlockRegistry,
    pending_chunks: Vec<(i32, i32, i32)>,
}

impl GameState {
    async fn new(window: Arc<Window>) -> Self {
        let renderer = Renderer::new(window).await;

        let registry = BlockRegistry::new();
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        log::info!("World seed: {}", seed);
        let world = WorldManager::new(seed);

        // Build list of all chunk positions to load, sorted by distance to camera
        let cam_cx = (128.0 / CHUNK_SIZE as f32) as i32;
        let cam_cy = (60.0 / CHUNK_SIZE as f32) as i32;
        let cam_cz = (128.0 / CHUNK_SIZE as f32) as i32;

        let mut pending_chunks: Vec<(i32, i32, i32)> = Vec::new();
        for cy in 0..WORLD_CY {
            for cz in 0..WORLD_CZ {
                for cx in 0..WORLD_CX {
                    pending_chunks.push((cx, cy, cz));
                }
            }
        }

        // Sort farthest-first so pop() yields closest chunks first
        pending_chunks.sort_by(|a, b| {
            let dist_a = (a.0 - cam_cx).pow(2) + (a.1 - cam_cy).pow(2) + (a.2 - cam_cz).pow(2);
            let dist_b = (b.0 - cam_cx).pow(2) + (b.1 - cam_cy).pow(2) + (b.2 - cam_cz).pow(2);
            dist_b.cmp(&dist_a)
        });

        let PipelineBundle {
            fill_pipeline: render_pipeline,
            wireframe_pipeline,
            camera_bind_group,
            camera_buffer,
            light_buffer,
        } = pipeline::create_pipeline(
            &renderer.device,
            renderer.config.format,
            renderer.config.width,
            renderer.config.height,
        );

        let camera = Camera::new(renderer.config.width, renderer.config.height);
        let depth_texture_view = pipeline::create_depth_texture_view(
            &renderer.device,
            renderer.config.width,
            renderer.config.height,
        );

        Self {
            renderer,
            gpu_meshes: HashMap::new(),
            render_pipeline,
            wireframe_pipeline,
            camera,
            camera_buffer,
            camera_bind_group,
            day_time: 0.25, // start at noon
            is_night: false,
            light_buffer,
            input: InputState::default(),
            depth_texture_view,
            debug: debug::DebugOverlay::new(),
            world,
            registry,
            pending_chunks,
        }
    }

    /// Generate N chunks per frame and mesh them + re-mesh dirty neighbors.
    fn load_chunks(&mut self) {
        let mut chunks_to_mesh: Vec<(i32, i32, i32)> = Vec::new();

        // Generate up to CHUNKS_PER_FRAME new chunks
        for _ in 0..CHUNKS_PER_FRAME {
            let Some(pos) = self.pending_chunks.pop() else { break };
            let dirty_neighbors = self.world.generate_chunk(pos.0, pos.1, pos.2);
            chunks_to_mesh.push(pos);
            // Also re-mesh neighbors that were affected
            for npos in dirty_neighbors {
                if !chunks_to_mesh.contains(&npos) {
                    chunks_to_mesh.push(npos);
                }
            }
        }

        // Also find already-loaded chunks that are dirty (from previous neighbor insertions)
        // but limit the scan to avoid doing too much per frame
        // (dirty neighbors from generate_chunk are already in chunks_to_mesh)

        // Mesh all chunks that need it
        for pos in chunks_to_mesh {
            self.mesh_and_upload(pos.0, pos.1, pos.2);
        }
    }

    fn mesh_and_upload(&mut self, cx: i32, cy: i32, cz: i32) {
        let chunk = match self.world.get_chunk(cx, cy, cz) {
            Some(c) => c,
            None => return,
        };

        let neighbors = ChunkNeighbors {
            pos_x: self.world.get_chunk(cx + 1, cy, cz),
            neg_x: self.world.get_chunk(cx - 1, cy, cz),
            pos_y: self.world.get_chunk(cx, cy + 1, cz),
            neg_y: self.world.get_chunk(cx, cy - 1, cz),
            pos_z: self.world.get_chunk(cx, cy, cz + 1),
            neg_z: self.world.get_chunk(cx, cy, cz - 1),
        };

        let (vertices, indices) = mesh::mesh_chunk(chunk, &self.registry, &neighbors);

        if indices.is_empty() {
            self.gpu_meshes.remove(&(cx, cy, cz));
            return;
        }

        let gpu_mesh = GpuMesh::new(&self.renderer.device, &vertices, &indices);
        self.gpu_meshes.insert((cx, cy, cz), gpu_mesh);
    }

    fn render(&mut self) {
        self.renderer.window.request_redraw();

        if !self.renderer.is_surface_configured {
            return;
        }

        // Progressive chunk loading
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
            let cam_cx = (128.0 / CHUNK_SIZE as f32) as i32;
            let cam_cy = (60.0  / CHUNK_SIZE as f32) as i32;
            let cam_cz = (128.0 / CHUNK_SIZE as f32) as i32;
            self.pending_chunks.clear();
            for cy in 0..WORLD_CY {
                for cz in 0..WORLD_CZ {
                    for cx in 0..WORLD_CX {
                        self.pending_chunks.push((cx, cy, cz));
                    }
                }
            }
            self.pending_chunks.sort_by(|a, b| {
                let da = (a.0-cam_cx).pow(2) + (a.1-cam_cy).pow(2) + (a.2-cam_cz).pow(2);
                let db = (b.0-cam_cx).pow(2) + (b.1-cam_cy).pow(2) + (b.2-cam_cz).pow(2);
                db.cmp(&da)
            });
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

            let pipeline = if self.debug.wireframe { &self.wireframe_pipeline } else { &self.render_pipeline };
            rp.set_pipeline(pipeline);
            rp.set_bind_group(0, &self.camera_bind_group, &[]);

            for mesh in self.gpu_meshes.values() {
                rp.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                rp.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                rp.draw_indexed(0..mesh.num_indices, 0, 0..1);
            }
        }

        self.renderer.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}

fn main() {
    env_logger::init();
    EventLoop::new()
        .expect("Impossible de créer l'event loop")
        .run_app(&mut App::default())
        .expect("Erreur dans l'event loop");
}
