use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::Window,
};

mod camera;
mod debug;
mod frustum;
mod game_state;
mod gpu_mesh;
mod input;
mod pipeline;
mod render;
mod renderer;
mod streaming;

use game_state::GameState;

pub const RENDER_DISTANCE: i32 = 16;
pub const WORLD_HEIGHT: i32 = 3;  // exclusive upper bound for cy (covers up to cy=2, world_y=47)
pub const WORLD_DEPTH: i32 = 4;   // cy starts at -4, world_y=-64 (covers WORLD_Y_MIN=-50)
pub const GEN_BUDGET_PER_FRAME: usize = 32;
pub const MESH_BUDGET_PER_FRAME: usize = 16;

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

fn main() {
    env_logger::init();
    EventLoop::new()
        .expect("Impossible de créer l'event loop")
        .run_app(&mut App::default())
        .expect("Erreur dans l'event loop");
}
