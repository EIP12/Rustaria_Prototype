use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rustaria_core::{block::BlockRegistry, world_manager::WorldManager};
use winit::window::Window;

use crate::{
    camera::Camera,
    debug,
    gpu_mesh::GpuMesh,
    input::InputState,
    pipeline::{self, PipelineBundle},
    renderer::Renderer,
};

pub struct GameState {
    pub(crate) renderer: Renderer,

    pub(crate) gpu_meshes: HashMap<(i32, i32, i32), GpuMesh>,

    pub(crate) render_pipeline: wgpu::RenderPipeline,
    pub(crate) wireframe_pipeline: wgpu::RenderPipeline,
    pub(crate) chunk_border_pipeline: wgpu::RenderPipeline,

    pub(crate) camera: Camera,
    pub(crate) camera_buffer: wgpu::Buffer,
    pub(crate) camera_bind_group: wgpu::BindGroup,

    pub(crate) day_time: f32,
    pub(crate) is_night: bool,
    pub(crate) light_buffer: wgpu::Buffer,

    pub(crate) input: InputState,

    pub(crate) depth_texture_view: wgpu::TextureView,

    pub(crate) debug: debug::DebugOverlay,

    // World state
    pub(crate) world: WorldManager,
    pub(crate) registry: BlockRegistry,

    // Streaming state
    pub(crate) pending_queue: VecDeque<(i32, i32, i32)>,
    pub(crate) loaded_chunks: HashSet<(i32, i32, i32)>,
    pub(crate) last_cam_chunk: Option<(i32, i32, i32)>,

    // FPS counter
    pub(crate) last_frame: Instant,
    pub(crate) fps: f64,
}

impl GameState {
    pub async fn new(window: Arc<Window>) -> Self {
        let renderer = Renderer::new(window).await;

        let registry = BlockRegistry::new();
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        log::info!("World seed: {}", seed);
        let world = WorldManager::new(seed);

        let PipelineBundle {
            fill_pipeline: render_pipeline,
            wireframe_pipeline,
            chunk_border_pipeline,
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
            chunk_border_pipeline,
            camera,
            camera_buffer,
            camera_bind_group,
            day_time: 0.25,
            is_night: false,
            light_buffer,
            input: InputState::default(),
            depth_texture_view,
            debug: debug::DebugOverlay::new(),
            world,
            registry,
            pending_queue: VecDeque::new(),
            loaded_chunks: HashSet::new(),
            last_cam_chunk: None,
            last_frame: Instant::now(),
            fps: 0.0,
        }
    }
}
