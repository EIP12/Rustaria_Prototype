use std::sync::Arc;

use bytemuck;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::Window,
};

use rustaria_core::{block::BlockRegistry, chunk::ChunkData, mesh::mesh_chunk};

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

/// Toutes les ressources GPU + données moteur.
pub struct GameState {
    renderer: Renderer,

    mesh: GpuMesh,

    render_pipeline: wgpu::RenderPipeline,
    wireframe_pipeline: wgpu::RenderPipeline,

    camera: Camera,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,

    day_time: f32,
    light_buffer: wgpu::Buffer,

    input: InputState,

    depth_texture_view: wgpu::TextureView,

    debug: debug::DebugOverlay,
}

impl GameState {
    async fn new(window: Arc<Window>) -> Self {
        let renderer = Renderer::new(window).await;

        let registry = BlockRegistry::new();
        let chunk = ChunkData::generate_flat_test();
        let (vertices, indices) = mesh_chunk(&chunk, &registry);

        let mesh = GpuMesh::new(&renderer.device, &vertices, &indices);

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
            mesh,
            render_pipeline,
            wireframe_pipeline,
            camera,
            camera_buffer,
            camera_bind_group,
            day_time: 0.1,
            light_buffer,
            input: InputState::default(),
            depth_texture_view,
            debug: debug::DebugOverlay::new(),
        }
    }

    fn render(&mut self) {
        self.renderer.window.request_redraw();

        if !self.renderer.is_surface_configured {
            return;
        }

        self.camera.update(&self.input);
        self.input.mouse_dx = 0.0;
        self.input.mouse_dy = 0.0;
        self.camera.upload(&self.renderer.queue, &self.camera_buffer);

        self.day_time = (self.day_time + 0.00083) % 1.0;
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
            rp.set_vertex_buffer(0, self.mesh.vertex_buffer.slice(..));
            rp.set_index_buffer(self.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            rp.draw_indexed(0..self.mesh.num_indices, 0, 0..1);
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
