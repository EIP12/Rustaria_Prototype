use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

use rustaria_core::{block::BlockRegistry, chunk::ChunkData, mesh::mesh_chunk};

mod pipeline;
mod renderer;

use renderer::Renderer;

// ─────────────────────────────────────────────
// App : coquille winit, suit le tutoriel learn-wgpu
// ─────────────────────────────────────────────
pub struct App {
    state: Option<GameState>,
}

impl App {
    pub fn new() -> Self {
        Self { state: None }
    }
}

impl ApplicationHandler for App {
    // Appelé quand la fenêtre est prête (équivalent ancien EventLoop::run)
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attrs = Window::default_attributes()
            .with_title("Rustaria Alpha")
            .with_inner_size(winit::dpi::PhysicalSize::new(1280u32, 720u32));

        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("Impossible to create window"),
        );

        // pollster::block_on : exécute l'async wgpu de façon bloquante (natif seulement)
        // State::new() est async à cause de request_adapter
        self.state = Some(pollster::block_on(GameState::new(window)));
    }

    // Tous les événements fenêtre arrivent ici
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let state = match &mut self.state {
            Some(s) => s,
            None => return, // Fenêtre pas encore prête → on ignore
        };

        match event {
            // ── Fermeture ──────────────────────────────────────
            WindowEvent::CloseRequested => event_loop.exit(),

            // ── Échap aussi = fermeture ─────────────────────────
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => event_loop.exit(),

            // ── Redimensionnement ───────────────────────────────
            // Obligatoire même si on ne resize pas :
            // wgpu plante si la surface n'est pas reconfigurée
            WindowEvent::Resized(size) => {
                state.renderer.resize(size.width, size.height);
                // Recréer le depth buffer à la nouvelle taille
                state.depth_texture_view = pipeline::create_depth_texture_view(
                    &state.renderer.device,
                    state.renderer.config.width,
                    state.renderer.config.height,
                );
            }

            // ── Rendu ───────────────────────────────────────────
            // request_redraw() est appelé dans render()
            WindowEvent::RedrawRequested => {
                state.render();
            }

            _ => {}
        }
    }
}

// ─────────────────────────────────────────────
// GameState : toutes les ressources GPU + données moteur
// ─────────────────────────────────────────────
pub struct GameState {
    renderer: Renderer,

    // Buffers GPU générés depuis rustaria-core
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,

    // Pipeline de rendu + caméra
    render_pipeline: wgpu::RenderPipeline,
    camera_bind_group: wgpu::BindGroup,

    // Depth buffer (indispensable pour l'ordre des faces 3D)
    depth_texture_view: wgpu::TextureView,
}

impl GameState {
    // Tout l'init se fait ici, une seule fois au démarrage
    async fn new(window: Arc<Window>) -> Self {
        // ── 1. Renderer : Instance → Surface → Adapter → Device + Queue ──
        // Voir learn-wgpu tuto 2 → State::new()
        let renderer = Renderer::new(window).await;

        // ── 2. Données depuis rustaria-core ──────────────────────────────
        // On ne touche pas au renderer ici : données pures, aucun GPU
        let registry = BlockRegistry::new();
        let chunk = ChunkData::generate_single_block_test();
        let (vertices, indices) = mesh_chunk(&chunk, &registry);

        // ── 3. Upload vertex buffer ──────────────────────────────────────
        use wgpu::util::DeviceExt; // requis pour create_buffer_init
        let vertex_buffer =
            renderer
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Vertex Buffer"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

        // ── 4. Upload index buffer ───────────────────────────────────────
        let index_buffer =
            renderer
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Index Buffer"),
                    contents: bytemuck::cast_slice(&indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
        let num_indices = indices.len() as u32;

        // ── 5. Pipeline + uniform buffer MVP (caméra fixe) ───────────────
        // La matrice MVP est calculée UNE SEULE FOIS ici, jamais mise à jour
        // Caméra : (3, 2, 3) → (0, 0, 0), FOV 70°
        let (render_pipeline, camera_bind_group) = pipeline::create_pipeline(
            &renderer.device,
            renderer.config.format,
            renderer.config.width,
            renderer.config.height,
        );
        // ── 6. Depth buffer ──────────────────────────────────────────────
        let depth_texture_view = pipeline::create_depth_texture_view(
            &renderer.device,
            renderer.config.width,
            renderer.config.height,
        );

        Self {
            renderer,
            vertex_buffer,
            index_buffer,
            num_indices,
            render_pipeline,
            camera_bind_group,
            depth_texture_view,
        }
    }

    fn render(&mut self) {
        // request_redraw ici = boucle continue, pattern learn-wgpu tuto 2
        self.renderer.window.request_redraw();

        // Surface pas encore configurée → on skip ce frame
        if !self.renderer.is_surface_configured {
            return;
        }

        // ── 1. Texture courante (le backbuffer) ──────────────────────────
        let output = match self.renderer.surface.get_current_texture() {
            Ok(texture) => texture,
            // Surface perdue (resize race condition, alt-tab, etc.) → reconfigure
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                let size = self.renderer.window.inner_size();
                self.renderer.resize(size.width, size.height);
                return;
            }
            Err(e) => {
                log::error!("Error Surface : {}", e);
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // ── 2. Command encoder ───────────────────────────────────────────
        let mut encoder =
            self.renderer
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                });

        // ── 3. Render pass ───────────────────────────────────────────────
        // Le bloc {} est important : il drop render_pass avant encoder.finish()
        // (borrow checker Rust — voir learn-wgpu tuto 2)
        {
            let mut render_pass =
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            // Fond bleu nuit : charte graphique Rustaria (#010409)
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.004,
                                g: 0.016,
                                b: 0.035,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    // Depth buffer obligatoire : sans lui les faces se dessinent
                    // dans le mauvais ordre (z-fighting)
                    depth_stencil_attachment: Some(
                        wgpu::RenderPassDepthStencilAttachment {
                            view: &self.depth_texture_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        },
                    ),
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });

            // ── 4. Bind pipeline + ressources ────────────────────────────
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(
                self.index_buffer.slice(..),
                wgpu::IndexFormat::Uint32,
            );

            // ── 5. Draw ──────────────────────────────────────────────────
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);

        } // ← drop(render_pass) ici, libère le borrow sur encoder

        // ── 6. Submit + Present ──────────────────────────────────────────
        self.renderer
            .queue
            .submit(std::iter::once(encoder.finish()));
        output.present();
    }
}

// ─────────────────────────────────────────────
// Point d'entrée
// ─────────────────────────────────────────────
fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().expect("Impossible to create event loop");
    let mut app = App::new();

    // run_app = nouveau pattern winit 0.30 (remplace l'ancien event_loop.run())
    event_loop
        .run_app(&mut app)
        .expect("Error in event loop");
}
